// Windows ウィンドウのスクリーンショットを取得する MCP サーバー (stdio transport)。
// tool: capture_window(title, full_size?, output_dir?)
//   - EnumWindows でトップレベル可視ウィンドウを列挙
//   - title 部分一致 (case-insensitive)。複数マッチはエラー + 候補返却
//   - PrintWindow + PW_RENDERFULLCONTENT で DWM/レイヤード合成に対応
//   - GetDIBits で top-down BGRA を取り出し RGBA 変換 → image crate で PNG 保存
//   - デフォルト 1280px 幅にリサイズ、full_size=true でフル解像度
//   - 窓単体なので、layered window (画素ごとの alpha) の透明部分は黒になる
// tool: capture_region(title? + margin? | x,y,width,height, backdrop?, full_size?, output_dir?)
//   - 画面に合成された結果を撮る。layered window は BitBlt に CAPTUREBLT がないと写らない
//   - title の窓の四角 + margin、または画面の物理 px の範囲
//   - backdrop: 撮る間だけ title の窓のすぐ後ろに単色の窓を敷く (影・半透明が暗い背景で見えない対策)

use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use rmcp::{
    ErrorData as McpError, ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, Content, Implementation, ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router,
    transport::stdio,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, TRUE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLACK_BRUSH, BitBlt, CAPTUREBLT, CreateCompatibleBitmap,
    CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC, DeleteObject, GET_STOCK_OBJECT_FLAGS, GetDC,
    GetDIBits, GetStockObject, HBRUSH, HDC, HGDIOBJ, ROP_CODE, ReleaseDC, SRCCOPY, SelectObject,
    UpdateWindow, WHITE_BRUSH,
};
use windows::Win32::Storage::Xps::{PRINT_WINDOW_FLAGS, PrintWindow};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, EnumWindows, GWL_EXSTYLE,
    GetWindowLongW, GetWindowRect, GetWindowTextLengthW, GetWindowTextW, IsIconic, IsWindowVisible,
    MSG, PM_REMOVE, PW_RENDERFULLCONTENT, PeekMessageW, RegisterClassW, SWP_NOACTIVATE,
    SWP_SHOWWINDOW, SetWindowPos, TranslateMessage, WNDCLASSW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_POPUP,
};
use windows::core::{PCWSTR, w};

const PW_FULLCONTENT_FLAGS: PRINT_WINDOW_FLAGS = PRINT_WINDOW_FLAGS(PW_RENDERFULLCONTENT);
const DEFAULT_MAX_WIDTH: u32 = 1280;
/// capture_region で title の窓の四角の外に足す既定の幅 (物理 px)。影が入る程度
const DEFAULT_REGION_MARGIN: i32 = 24;
/// backdrop を出してから DWM の合成に載るまで待つ時間
const BACKDROP_SETTLE: Duration = Duration::from_millis(300);

pub fn run() {
    if let Err(e) = run_inner() {
        eprintln!("mcp-screenshot: {e:#}");
        std::process::exit(1);
    }
}

fn run_inner() -> Result<()> {
    // PrintWindow 経由のキャプチャがスケール表示で正しいピクセル数を返すよう、
    // プロセス起動時に Per-Monitor V2 DPI awareness を立てる。
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("build tokio runtime")?;

    rt.block_on(async move {
        let service = ScreenshotServer::new().serve(stdio()).await?;
        service.waiting().await?;
        Ok::<_, anyhow::Error>(())
    })
}

#[derive(Debug, Clone)]
pub struct ScreenshotServer {
    tool_router: ToolRouter<ScreenshotServer>,
}

impl ScreenshotServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CaptureWindowRequest {
    /// ウィンドウタイトルの部分一致 (case-insensitive)
    pub title: String,
    /// true で元解像度のまま保存。デフォルト false で 1280px 幅に縮小
    #[serde(default)]
    pub full_size: bool,
    /// 出力ディレクトリ。未指定は $TEMP/dotcli-screenshots/
    #[serde(default)]
    pub output_dir: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CaptureRegionRequest {
    /// 撮る窓のタイトルの部分一致 (case-insensitive)。その窓の四角に margin を足した範囲を撮る
    #[serde(default)]
    pub title: Option<String>,
    /// title の窓の四角の外に足す幅 (物理 px)。既定 24 (影など窓の外側も入れる)
    #[serde(default)]
    pub margin: Option<i32>,
    /// title を使わないときの範囲の左 (画面の物理 px)
    #[serde(default)]
    pub x: Option<i32>,
    /// title を使わないときの範囲の上 (画面の物理 px)
    #[serde(default)]
    pub y: Option<i32>,
    /// title を使わないときの範囲の幅 (物理 px)
    #[serde(default)]
    pub width: Option<i32>,
    /// title を使わないときの範囲の高さ (物理 px)
    #[serde(default)]
    pub height: Option<i32>,
    /// "white" か "black"。撮る間だけ title の窓のすぐ後ろに単色の窓を敷く (影・半透明の確認用、title が必要)
    #[serde(default)]
    pub backdrop: Option<String>,
    /// true で元解像度のまま保存。デフォルト false で 1280px 幅に縮小
    #[serde(default)]
    pub full_size: bool,
    /// 出力ディレクトリ。未指定は $TEMP/dotcli-screenshots/
    #[serde(default)]
    pub output_dir: Option<String>,
}

#[derive(Debug, Serialize)]
struct RegionResult {
    path: String,
    matched_title: Option<String>,
    x: i32,
    y: i32,
    original_width: u32,
    original_height: u32,
    saved_width: u32,
    saved_height: u32,
}

#[derive(Debug, Serialize)]
struct CaptureResult {
    path: String,
    matched_title: String,
    original_width: u32,
    original_height: u32,
    saved_width: u32,
    saved_height: u32,
}

#[tool_router]
impl ScreenshotServer {
    #[tool(
        description = "Capture a screenshot of a Windows top-level window matched by partial title (case-insensitive). Saves a PNG and returns its path. Errors with the candidate list if zero or multiple windows match. Transparent parts of layered (per-pixel alpha) windows come out black; use capture_region for those."
    )]
    fn capture_window(
        &self,
        Parameters(req): Parameters<CaptureWindowRequest>,
    ) -> Result<CallToolResult, McpError> {
        let result = capture(&req).map_err(|e| McpError::internal_error(format!("{e:#}"), None))?;
        let payload = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(payload)]))
    }

    #[tool(
        description = "Capture a rectangle of the screen as composited, including layered (per-pixel alpha) windows such as popups with rounded corners or shadows. Give `title` (partial, case-insensitive) to capture that window plus `margin` physical pixels around it (default 24), or x/y/width/height in physical screen pixels. `backdrop` \"white\" or \"black\" (needs title) places a plain window right behind the target while capturing, so shadows and translucency show regardless of what is behind. Saves a PNG and returns its path."
    )]
    fn capture_region(
        &self,
        Parameters(req): Parameters<CaptureRegionRequest>,
    ) -> Result<CallToolResult, McpError> {
        let result = capture_region_png(&req)
            .map_err(|e| McpError::internal_error(format!("{e:#}"), None))?;
        let payload = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(payload)]))
    }
}

#[tool_handler]
impl ServerHandler for ScreenshotServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            server_info: Implementation {
                name: "dotcli-screenshot".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                ..Default::default()
            },
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            instructions: Some(
                "Capture a Windows top-level window's pixels and save as PNG. \
                 Use the capture_window tool with a partial-title substring. \
                 For layered windows (popups with transparency or shadows), \
                 use capture_region."
                    .to_string(),
            ),
            ..Default::default()
        }
    }
}

fn capture(req: &CaptureWindowRequest) -> Result<CaptureResult> {
    let target = find_window(&req.title)?;
    let hwnd = HWND(target.hwnd as _);

    unsafe {
        if IsIconic(hwnd).as_bool() {
            bail!(
                "window {:?} is minimized; restore it before capture",
                target.title
            );
        }
    }

    let pixels = unsafe { capture_window_pixels(hwnd) }?;
    let (orig_w, orig_h) = (pixels.width, pixels.height);
    let saved = save_png(
        pixels,
        req.full_size,
        req.output_dir.as_deref(),
        &format!("window-{}", sanitize_filename(&target.title)),
    )?;

    Ok(CaptureResult {
        path: saved.path,
        matched_title: target.title,
        original_width: orig_w,
        original_height: orig_h,
        saved_width: saved.width,
        saved_height: saved.height,
    })
}

fn capture_region_png(req: &CaptureRegionRequest) -> Result<RegionResult> {
    let (rect, target) = match req.title.as_deref() {
        Some(title) => {
            let target = find_window(title)?;
            let hwnd = HWND(target.hwnd as _);
            let mut r = RECT::default();
            unsafe { GetWindowRect(hwnd, &mut r) }.context("GetWindowRect")?;
            let m = req.margin.unwrap_or(DEFAULT_REGION_MARGIN).max(0);
            let rect = RECT {
                left: r.left - m,
                top: r.top - m,
                right: r.right + m,
                bottom: r.bottom + m,
            };
            (rect, Some((hwnd, target.title)))
        }
        None => match (req.x, req.y, req.width, req.height) {
            (Some(x), Some(y), Some(w), Some(h)) => (
                RECT {
                    left: x,
                    top: y,
                    right: x + w,
                    bottom: y + h,
                },
                None,
            ),
            _ => bail!("give title, or all of x, y, width and height"),
        },
    };
    let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
    if width <= 0 || height <= 0 {
        bail!("region has zero size");
    }

    let brush = match req.backdrop.as_deref() {
        None => None,
        Some("white") => Some(WHITE_BRUSH),
        Some("black") => Some(BLACK_BRUSH),
        Some(other) => bail!("backdrop must be \"white\" or \"black\", got {other:?}"),
    };
    let backdrop = match (brush, &target) {
        (Some(brush), Some((hwnd, _))) => Some(unsafe { Backdrop::show(*hwnd, &rect, brush) }?),
        (Some(_), None) => bail!("backdrop needs title: it is placed right behind that window"),
        (None, _) => None,
    };

    let pixels = unsafe {
        grab(None, width, height, |src, mem| {
            BitBlt(
                mem,
                0,
                0,
                width,
                height,
                Some(src),
                rect.left,
                rect.top,
                ROP_CODE(SRCCOPY.0 | CAPTUREBLT.0),
            )
            .context("BitBlt from the screen")
        })
    }?;
    drop(backdrop);

    let (orig_w, orig_h) = (pixels.width, pixels.height);
    let name = match &target {
        Some((_, title)) => format!("region-{}", sanitize_filename(title)),
        None => format!("region-{}_{}_{}x{}", rect.left, rect.top, width, height),
    };
    let saved = save_png(pixels, req.full_size, req.output_dir.as_deref(), &name)?;

    Ok(RegionResult {
        path: saved.path,
        matched_title: target.map(|(_, title)| title),
        x: rect.left,
        y: rect.top,
        original_width: orig_w,
        original_height: orig_h,
        saved_width: saved.width,
        saved_height: saved.height,
    })
}

struct SavedPng {
    path: String,
    width: u32,
    height: u32,
}

/// PNG で保存する。full_size でなければ DEFAULT_MAX_WIDTH 幅に縮める。
fn save_png(
    pixels: CapturedPixels,
    full_size: bool,
    output_dir: Option<&str>,
    stem: &str,
) -> Result<SavedPng> {
    let (orig_w, orig_h) = (pixels.width, pixels.height);
    let img = image::RgbaImage::from_raw(orig_w, orig_h, pixels.rgba)
        .ok_or_else(|| anyhow!("RgbaImage::from_raw failed: buffer size mismatch"))?;

    let final_img = if !full_size && orig_w > DEFAULT_MAX_WIDTH {
        let new_h = (orig_h as f64 * DEFAULT_MAX_WIDTH as f64 / orig_w as f64).round() as u32;
        image::imageops::resize(
            &img,
            DEFAULT_MAX_WIDTH,
            new_h.max(1),
            image::imageops::FilterType::Triangle,
        )
    } else {
        img
    };

    let out_dir = match output_dir {
        Some(p) => PathBuf::from(p),
        None => std::env::temp_dir().join("dotcli-screenshots"),
    };
    std::fs::create_dir_all(&out_dir)
        .with_context(|| format!("create output dir {}", out_dir.display()))?;

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let file = out_dir.join(format!("{stem}-{ts}.png"));
    final_img
        .save(&file)
        .with_context(|| format!("save PNG to {}", file.display()))?;

    Ok(SavedPng {
        path: file.to_string_lossy().into_owned(),
        width: final_img.width(),
        height: final_img.height(),
    })
}

/// タイトルの部分一致 (case-insensitive) でちょうど 1 つの可視トップレベル窓を探す。
fn find_window(title: &str) -> Result<WindowInfo> {
    let needle = title.trim();
    if needle.is_empty() {
        bail!("title must not be empty");
    }

    let windows = enum_visible_windows()?;
    let needle_lower = needle.to_lowercase();
    let matches: Vec<&WindowInfo> = windows
        .iter()
        .filter(|w| w.title.to_lowercase().contains(&needle_lower))
        .collect();

    match matches.len() {
        0 => {
            let sample: Vec<String> = windows.iter().take(20).map(|w| w.title.clone()).collect();
            bail!(
                "no window title matched {:?}. visible top-level titles (up to 20): {:?}",
                needle,
                sample
            );
        }
        1 => {}
        _ => {
            let candidates: Vec<String> = matches.iter().map(|w| w.title.clone()).collect();
            bail!(
                "{} windows matched {:?}; refine the title. candidates: {:?}",
                matches.len(),
                needle,
                candidates
            );
        }
    }

    Ok(matches[0].clone())
}

#[derive(Debug, Clone)]
struct WindowInfo {
    hwnd: isize,
    title: String,
}

fn enum_visible_windows() -> Result<Vec<WindowInfo>> {
    let mut acc: Vec<WindowInfo> = Vec::new();
    let ptr = &mut acc as *mut Vec<WindowInfo> as isize;
    unsafe {
        EnumWindows(Some(enum_proc), LPARAM(ptr))?;
    }
    Ok(acc)
}

unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> windows::core::BOOL {
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() {
            return TRUE;
        }
        let len = GetWindowTextLengthW(hwnd);
        if len <= 0 {
            return TRUE;
        }
        let mut buf = vec![0u16; (len as usize) + 1];
        let written = GetWindowTextW(hwnd, &mut buf);
        if written <= 0 {
            return TRUE;
        }
        let title = String::from_utf16_lossy(&buf[..written as usize]);
        let acc = &mut *(lparam.0 as *mut Vec<WindowInfo>);
        acc.push(WindowInfo {
            hwnd: hwnd.0 as isize,
            title,
        });
        TRUE
    }
}

struct CapturedPixels {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

unsafe fn capture_window_pixels(hwnd: HWND) -> Result<CapturedPixels> {
    let mut rect = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut rect) }.context("GetWindowRect")?;
    let width = (rect.right - rect.left).max(0);
    let height = (rect.bottom - rect.top).max(0);
    if width == 0 || height == 0 {
        bail!("window has zero size");
    }

    unsafe {
        grab(Some(hwnd), width, height, |_, mem_dc| {
            if PrintWindow(hwnd, mem_dc, PW_FULLCONTENT_FLAGS).as_bool() {
                Ok(())
            } else {
                bail!("PrintWindow returned FALSE (window may be DWM-occluded or protected)")
            }
        })
    }
}

/// `owner` の DC (None は画面全体) と互換の width × height の bitmap を作り、`draw(src, mem)` で
/// 描かせて RGBA で取り出す。
unsafe fn grab(
    owner: Option<HWND>,
    width: i32,
    height: i32,
    draw: impl FnOnce(HDC, HDC) -> Result<()>,
) -> Result<CapturedPixels> {
    // Source DC は対象ウィンドウ (か画面) のものを使う。memory DC に対して CreateCompatibleBitmap
    // すると 1bpp モノクロが返り真っ黒な PNG になるので必ず window DC を渡す。
    let src_dc = unsafe { GetDC(owner) };
    if src_dc.is_invalid() {
        bail!("GetDC returned invalid DC");
    }
    let mem_dc = unsafe { CreateCompatibleDC(Some(src_dc)) };
    if mem_dc.is_invalid() {
        unsafe { ReleaseDC(owner, src_dc) };
        bail!("CreateCompatibleDC failed");
    }
    let bitmap = unsafe { CreateCompatibleBitmap(src_dc, width, height) };
    if bitmap.is_invalid() {
        unsafe {
            let _ = DeleteDC(mem_dc);
            ReleaseDC(owner, src_dc);
        }
        bail!("CreateCompatibleBitmap failed");
    }

    // 解放は 2 段に分ける。`release` は bitmap / memory DC / window DC の破棄、`deselect` は
    // bitmap の選択解除 (SelectObject(old))。deselect を後に宣言するので、早期 return でも
    // drop 順は「選択解除 → DeleteObject → DeleteDC → ReleaseDC」になる。
    let release = scopeguard(move || unsafe {
        let _ = DeleteObject(HGDIOBJ(bitmap.0));
        let _ = DeleteDC(mem_dc);
        ReleaseDC(owner, src_dc);
    });
    let old_obj = unsafe { SelectObject(mem_dc, HGDIOBJ(bitmap.0)) };
    let deselect = scopeguard(move || unsafe {
        SelectObject(mem_dc, old_obj);
    });

    let drawn = draw(src_dc, mem_dc);
    // GetDIBits は「bitmap が DC に選択されていないこと」を要求するので、ここで選択を解除する。
    // draw (PrintWindow / BitBlt) のどちらの経路もここを通る。
    drop(deselect);
    if let Err(e) = drawn {
        drop(release);
        return Err(e);
    }

    // GetDIBits: biHeight を負にすると top-down BGRA (左上原点) で取り出せる。
    let mut bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            biSizeImage: 0,
            biXPelsPerMeter: 0,
            biYPelsPerMeter: 0,
            biClrUsed: 0,
            biClrImportant: 0,
        },
        bmiColors: Default::default(),
    };
    let pixel_count = (width as usize) * (height as usize);
    let mut buf = vec![0u8; pixel_count * 4];
    let copied = unsafe {
        GetDIBits(
            mem_dc,
            bitmap,
            0,
            height as u32,
            Some(buf.as_mut_ptr() as *mut _),
            &mut bmi,
            DIB_RGB_COLORS,
        )
    };
    drop(release);
    if copied == 0 {
        bail!("GetDIBits copied 0 scanlines");
    }

    // GDI は BGRA、image crate は RGBA を期待。B と R を入れ替える。
    // alpha は GDI が保証しないため (DWM/ドライバ依存で 0 が返り画像が完全透明=黒くなる) 0xFF を強制。
    for px in buf.chunks_exact_mut(4) {
        px.swap(0, 2);
        px[3] = 0xFF;
    }

    Ok(CapturedPixels {
        width: width as u32,
        height: height as u32,
        rgba: buf,
    })
}

/// 撮る間だけ対象の窓のすぐ後ろに敷く単色の窓。drop で消す。
struct Backdrop(HWND);

impl Backdrop {
    /// `behind` のすぐ後ろ (Z 順) に `rect` の大きさで出し、DWM の合成に載るまで待つ。
    /// 活性化はしない。`behind` が最前面 (topmost) の窓なら、同じ帯に入るよう topmost で作る。
    unsafe fn show(behind: HWND, rect: &RECT, brush: GET_STOCK_OBJECT_FLAGS) -> Result<Self> {
        let class: PCWSTR = if brush == WHITE_BRUSH {
            w!("dotcli-screenshot-backdrop-white")
        } else {
            w!("dotcli-screenshot-backdrop-black")
        };
        let instance = unsafe { GetModuleHandleW(None) }.context("GetModuleHandleW")?;
        let wc = WNDCLASSW {
            lpfnWndProc: Some(backdrop_proc),
            hInstance: instance.into(),
            hbrBackground: HBRUSH(unsafe { GetStockObject(brush) }.0),
            lpszClassName: class,
            ..Default::default()
        };
        // 2 回目以降は登録済みで 0 が返るが、そのまま使える。
        unsafe { RegisterClassW(&wc) };

        let mut ex = WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE;
        if unsafe { GetWindowLongW(behind, GWL_EXSTYLE) } as u32 & WS_EX_TOPMOST.0 != 0 {
            ex |= WS_EX_TOPMOST;
        }
        let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
        let hwnd = unsafe {
            CreateWindowExW(
                ex,
                class,
                w!(""),
                WS_POPUP,
                rect.left,
                rect.top,
                width,
                height,
                None,
                None,
                Some(instance.into()),
                None,
            )
        }
        .context("CreateWindowExW for the backdrop")?;
        let backdrop = Backdrop(hwnd);
        unsafe {
            SetWindowPos(
                hwnd,
                Some(behind),
                rect.left,
                rect.top,
                width,
                height,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            )
        }
        .context("SetWindowPos for the backdrop")?;
        unsafe {
            let _ = UpdateWindow(hwnd);
            pump(BACKDROP_SETTLE);
        }
        Ok(backdrop)
    }
}

impl Drop for Backdrop {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.0);
            pump(Duration::ZERO);
        }
    }
}

unsafe extern "system" fn backdrop_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// このスレッドの窓のメッセージを `duration` の間 (0 なら溜まっている分だけ) 処理する。
unsafe fn pump(duration: Duration) {
    let end = Instant::now() + duration;
    loop {
        let mut msg = MSG::default();
        while unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE) }.as_bool() {
            unsafe {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        if Instant::now() >= end {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn sanitize_filename(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches('_');
    let bounded: String = trimmed.chars().take(60).collect();
    if bounded.is_empty() {
        "untitled".into()
    } else {
        bounded
    }
}

// 小さな scope guard。drop 時にクロージャを必ず実行する。
struct ScopeGuard<F: FnOnce()> {
    f: Option<F>,
}
impl<F: FnOnce()> Drop for ScopeGuard<F> {
    fn drop(&mut self) {
        if let Some(f) = self.f.take() {
            f();
        }
    }
}
fn scopeguard<F: FnOnce()>(f: F) -> ScopeGuard<F> {
    ScopeGuard { f: Some(f) }
}
