// Windows ウィンドウのスクリーンショットを取得する MCP サーバー (stdio transport)。
// tool: capture_window(title, full_size?, output_dir?)
//   - EnumWindows でトップレベル可視ウィンドウを列挙
//   - title 部分一致 (case-insensitive)。複数マッチはエラー + 候補返却
//   - PrintWindow + PW_RENDERFULLCONTENT で DWM/レイヤード合成に対応
//   - GetDIBits で top-down BGRA を取り出し RGBA 変換 → image crate で PNG 保存
//   - デフォルト 1280px 幅にリサイズ、full_size=true でフル解像度

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

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

use windows::Win32::Foundation::{HWND, LPARAM, RECT, TRUE};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleBitmap, CreateCompatibleDC,
    DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetDIBits, HGDIOBJ, ReleaseDC, SelectObject,
};
use windows::Win32::Storage::Xps::{PRINT_WINDOW_FLAGS, PrintWindow};
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowRect, GetWindowTextLengthW, GetWindowTextW, IsIconic, IsWindowVisible,
    PW_RENDERFULLCONTENT,
};

const PW_FULLCONTENT_FLAGS: PRINT_WINDOW_FLAGS = PRINT_WINDOW_FLAGS(PW_RENDERFULLCONTENT);
const DEFAULT_MAX_WIDTH: u32 = 1280;

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
        description = "Capture a screenshot of a Windows top-level window matched by partial title (case-insensitive). Saves a PNG and returns its path. Errors with the candidate list if zero or multiple windows match."
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
                 Use the capture_window tool with a partial-title substring."
                    .to_string(),
            ),
            ..Default::default()
        }
    }
}

fn capture(req: &CaptureWindowRequest) -> Result<CaptureResult> {
    let needle = req.title.trim();
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

    let target = matches[0];
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

    let img = image::RgbaImage::from_raw(orig_w, orig_h, pixels.rgba)
        .ok_or_else(|| anyhow!("RgbaImage::from_raw failed: buffer size mismatch"))?;

    let final_img = if !req.full_size && orig_w > DEFAULT_MAX_WIDTH {
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
    let (saved_w, saved_h) = (final_img.width(), final_img.height());

    let out_dir = match req.output_dir.as_deref() {
        Some(p) => PathBuf::from(p),
        None => std::env::temp_dir().join("dotcli-screenshots"),
    };
    std::fs::create_dir_all(&out_dir)
        .with_context(|| format!("create output dir {}", out_dir.display()))?;

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let safe_title = sanitize_filename(&target.title);
    let file = out_dir.join(format!("window-{safe_title}-{ts}.png"));
    final_img
        .save(&file)
        .with_context(|| format!("save PNG to {}", file.display()))?;

    Ok(CaptureResult {
        path: file.to_string_lossy().into_owned(),
        matched_title: target.title.clone(),
        original_width: orig_w,
        original_height: orig_h,
        saved_width: saved_w,
        saved_height: saved_h,
    })
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

    // Source DC は対象ウィンドウのものを使う。memory DC に対して CreateCompatibleBitmap
    // すると 1bpp モノクロが返り真っ黒な PNG になるので必ず window DC を渡す。
    let src_dc = unsafe { GetDC(Some(hwnd)) };
    if src_dc.is_invalid() {
        bail!("GetDC returned invalid DC");
    }
    let mem_dc = unsafe { CreateCompatibleDC(Some(src_dc)) };
    if mem_dc.is_invalid() {
        unsafe { ReleaseDC(Some(hwnd), src_dc) };
        bail!("CreateCompatibleDC failed");
    }
    let bitmap = unsafe { CreateCompatibleBitmap(src_dc, width, height) };
    if bitmap.is_invalid() {
        unsafe {
            let _ = DeleteDC(mem_dc);
            ReleaseDC(Some(hwnd), src_dc);
        }
        bail!("CreateCompatibleBitmap failed");
    }

    // bitmap を DC に選択。restore (SelectObject(old)) を必ず DeleteObject より先に行うため
    // 同一 scopeguard 内で順序を固定する。
    let old_obj = unsafe { SelectObject(mem_dc, HGDIOBJ(bitmap.0)) };
    let cleanup = scopeguard(move || unsafe {
        SelectObject(mem_dc, old_obj);
        let _ = DeleteObject(HGDIOBJ(bitmap.0));
        let _ = DeleteDC(mem_dc);
        ReleaseDC(Some(hwnd), src_dc);
    });

    let printed = unsafe { PrintWindow(hwnd, mem_dc, PW_FULLCONTENT_FLAGS) };
    if !printed.as_bool() {
        drop(cleanup);
        bail!("PrintWindow returned FALSE (window may be DWM-occluded or protected)");
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
    drop(cleanup);
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
