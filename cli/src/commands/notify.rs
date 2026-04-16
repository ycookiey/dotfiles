#[cfg(target_os = "windows")]
mod win {
    use windows::{
        core::{w, PCWSTR},
        Win32::{
            Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM},
            Graphics::Gdi::{
                BeginPaint, CreateFontW, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint,
                FillRect, GetDeviceCaps, GetStockObject, SelectObject, SetBkMode, SetTextColor,
                UpdateWindow, ANSI_CHARSET, CLIP_DEFAULT_PRECIS, DEFAULT_PITCH, DEFAULT_QUALITY,
                DT_LEFT, DT_SINGLELINE, DT_VCENTER, DT_WORDBREAK, FF_DONTCARE, FW_BOLD, FW_NORMAL,
                HBRUSH, LOGPIXELSY, NULL_BRUSH, OUT_DEFAULT_PRECIS, PAINTSTRUCT, TRANSPARENT,
            },
            System::{
                Console::GetConsoleWindow,
                LibraryLoader::GetModuleHandleW,
                WindowsProgramming::MulDiv,
            },
            UI::WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect,
                GetMessageW, GetWindowLongPtrW, KillTimer, PostQuitMessage, RegisterClassExW,
                SetTimer, SetWindowLongPtrW, ShowWindow, SystemParametersInfoW, TranslateMessage,
                CREATESTRUCTW, GWLP_USERDATA, MSG, SPI_GETWORKAREA, SW_HIDE, SW_SHOWNOACTIVATE,
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, WNDCLASSEXW, WM_CREATE, WM_DESTROY,
                WM_ERASEBKGND, WM_LBUTTONDOWN, WM_NCCREATE, WM_PAINT, WM_TIMER,
                WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
            },
        },
    };

    const TIMER_ID: usize = 1;
    const WIN_W: i32 = 350;
    const WIN_H: i32 = 100;
    const MARGIN: i32 = 20;
    const STACK_GAP: i32 = 10;

    struct PopupParams {
        title: String,
        message: String,
        duration_ms: u32,
    }

    const fn rgb(r: u8, g: u8, b: u8) -> u32 {
        r as u32 | ((g as u32) << 8) | ((b as u32) << 16)
    }

    pub fn run(title: &str, message: &str, duration_ms: u32, offset: i32) {
        unsafe {
            let console = GetConsoleWindow();
            if !console.is_invalid() {
                let _ = ShowWindow(console, SW_HIDE);
            }

            let hinstance: HINSTANCE = GetModuleHandleW(None).unwrap().into();

            let class_name = w!("DotcliNotify");

            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(wnd_proc),
                hInstance: hinstance,
                lpszClassName: class_name,
                hbrBackground: HBRUSH(GetStockObject(NULL_BRUSH).0),
                ..Default::default()
            };
            RegisterClassExW(&wc);

            let mut work_area = RECT::default();
            SystemParametersInfoW(
                SPI_GETWORKAREA,
                0,
                Some(&mut work_area as *mut RECT as *mut _),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            )
            .ok();

            let x = work_area.right - WIN_W - MARGIN;
            let y = work_area.bottom - WIN_H - MARGIN - (offset * (WIN_H + STACK_GAP));

            let params = Box::new(PopupParams {
                title: title.to_string(),
                message: message.to_string(),
                duration_ms,
            });

            let hwnd = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                class_name,
                PCWSTR(std::ptr::null()),
                WS_POPUP,
                x,
                y,
                WIN_W,
                WIN_H,
                None,
                None,
                Some(hinstance),
                Some(Box::into_raw(params) as *mut _),
            )
            .unwrap();

            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            let _ = UpdateWindow(hwnd).ok();

            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }

    unsafe extern "system" fn wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        unsafe {
            match msg {
                WM_NCCREATE => {
                    let cs = &*(lparam.0 as *const CREATESTRUCTW);
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
                    DefWindowProcW(hwnd, msg, wparam, lparam)
                }
                WM_CREATE => {
                    let params =
                        &*(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const PopupParams);
                    SetTimer(Some(hwnd), TIMER_ID, params.duration_ms, None);
                    LRESULT(0)
                }
                WM_TIMER => {
                    if wparam.0 == TIMER_ID {
                        let _ = KillTimer(Some(hwnd), TIMER_ID);
                        DestroyWindow(hwnd).ok();
                    }
                    LRESULT(0)
                }
                WM_ERASEBKGND => LRESULT(1),
                WM_PAINT => {
                    let params =
                        &*(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const PopupParams);

                    let mut ps = PAINTSTRUCT::default();
                    let hdc = BeginPaint(hwnd, &mut ps);

                    let bg_brush = CreateSolidBrush(COLORREF(rgb(45, 45, 48)));
                    let mut rect = RECT::default();
                    GetClientRect(hwnd, &mut rect).ok();
                    FillRect(hdc, &rect, bg_brush);
                    let _ = DeleteObject(bg_brush.into());

                    SetBkMode(hdc, TRANSPARENT);

                    let logy = GetDeviceCaps(Some(hdc), LOGPIXELSY);

                    let title_font = CreateFontW(
                        -MulDiv(11, logy, 72),
                        0,
                        0,
                        0,
                        FW_BOLD.0 as i32,
                        0,
                        0,
                        0,
                        ANSI_CHARSET,
                        OUT_DEFAULT_PRECIS,
                        CLIP_DEFAULT_PRECIS,
                        DEFAULT_QUALITY,
                        (DEFAULT_PITCH.0 | (FF_DONTCARE.0 << 4)) as u32,
                        w!("Segoe UI"),
                    );
                    let old_font = SelectObject(hdc, title_font.into());
                    SetTextColor(hdc, COLORREF(0x00FFFFFF));
                    let mut title_rect = RECT {
                        left: 10,
                        top: 10,
                        right: 340,
                        bottom: 40,
                    };
                    let mut title_buf: Vec<u16> = params.title.encode_utf16().collect();
                    DrawTextW(
                        hdc,
                        &mut title_buf,
                        &mut title_rect,
                        DT_LEFT | DT_VCENTER | DT_SINGLELINE,
                    );

                    let msg_font = CreateFontW(
                        -MulDiv(9, logy, 72),
                        0,
                        0,
                        0,
                        FW_NORMAL.0 as i32,
                        0,
                        0,
                        0,
                        ANSI_CHARSET,
                        OUT_DEFAULT_PRECIS,
                        CLIP_DEFAULT_PRECIS,
                        DEFAULT_QUALITY,
                        (DEFAULT_PITCH.0 | (FF_DONTCARE.0 << 4)) as u32,
                        w!("Segoe UI"),
                    );
                    SelectObject(hdc, msg_font.into());
                    SetTextColor(hdc, COLORREF(rgb(211, 211, 211)));
                    let mut msg_rect = RECT {
                        left: 10,
                        top: 40,
                        right: 340,
                        bottom: 90,
                    };
                    let mut msg_buf: Vec<u16> = params.message.encode_utf16().collect();
                    DrawTextW(hdc, &mut msg_buf, &mut msg_rect, DT_LEFT | DT_WORDBREAK);

                    SelectObject(hdc, old_font);
                    let _ = DeleteObject(title_font.into());
                    let _ = DeleteObject(msg_font.into());

                    let _ = EndPaint(hwnd, &ps);
                    LRESULT(0)
                }
                WM_LBUTTONDOWN => {
                    DestroyWindow(hwnd).ok();
                    LRESULT(0)
                }
                WM_DESTROY => {
                    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut PopupParams;
                    if !ptr.is_null() {
                        drop(Box::from_raw(ptr));
                    }
                    PostQuitMessage(0);
                    LRESULT(0)
                }
                _ => DefWindowProcW(hwnd, msg, wparam, lparam),
            }
        }
    }
}

#[cfg(target_os = "windows")]
pub use win::run;

#[cfg(not(target_os = "windows"))]
pub fn run(_title: &str, _message: &str, _duration_ms: u32, _offset: i32) {
    eprintln!("notify is only supported on Windows");
    std::process::exit(1);
}
