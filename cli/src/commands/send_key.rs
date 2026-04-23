// 物理キーイベント注入 (Win32 SendInput)。
// 例: `dotcli send-key ctrl+l` → Ctrl down, L down, L up, Ctrl up
// claude code の Ctrl+L など、PTY 経由 (\x0c) では発火しない処理を起動するために使う。

use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP,
    MAPVK_VK_TO_VSC, MapVirtualKeyW, SendInput, VIRTUAL_KEY, VK_CONTROL, VK_LWIN, VK_MENU,
    VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{GetClassNameW, GetForegroundWindow};

pub fn run(args: &[String]) {
    // --only-when-class <CLASSNAME> でフォアグラウンドウィンドウのクラス名一致時のみ送る
    let mut chord: Option<&str> = None;
    let mut required_class: Option<&str> = None;
    let mut iter = args.iter();
    while let Some(a) = iter.next() {
        if a == "--only-when-class" {
            required_class = iter.next().map(|s| s.as_str());
        } else if chord.is_none() {
            chord = Some(a.as_str());
        }
    }

    let spec = chord.unwrap_or("");
    if spec.is_empty() {
        eprintln!(
            "Usage: dotcli send-key <chord> [--only-when-class <NAME>]  (e.g. ctrl+l, ctrl+shift+a)"
        );
        std::process::exit(1);
    }

    if let Some(cls) = required_class {
        match foreground_class() {
            Some(actual) if actual == cls => {}
            other => {
                eprintln!(
                    "send-key: skip (foreground class={:?}, required={cls})",
                    other.as_deref().unwrap_or("<none>")
                );
                return;
            }
        }
    }

    let vks: Vec<u16> = match spec
        .split('+')
        .map(parse_key)
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(v) => v,
        Err(msg) => {
            eprintln!("{msg}");
            std::process::exit(1);
        }
    };

    // 物理キー入力に近づけるためキーイベントごとに小さな間隔を入れる。
    // 一括 SendInput だと modifier の押下時間がゼロに近く、GetAsyncKeyState などで
    // 物理状態を確認するアプリ (claude code 等) が Ctrl+L として認識しないケースあり。
    let sz = std::mem::size_of::<INPUT>() as i32;
    let mut events: Vec<INPUT> = Vec::with_capacity(vks.len() * 2);
    for vk in &vks {
        events.push(make_input(*vk, false));
    }
    for vk in vks.iter().rev() {
        events.push(make_input(*vk, true));
    }
    for ev in &events {
        let sent = unsafe { SendInput(std::slice::from_ref(ev), sz) };
        if sent != 1 {
            eprintln!("SendInput failed");
            std::process::exit(1);
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

fn parse_key(part: &str) -> Result<u16, String> {
    let lower = part.to_ascii_lowercase();
    let vk = match lower.as_str() {
        "ctrl" | "control" => VK_CONTROL,
        "shift" => VK_SHIFT,
        "alt" | "menu" => VK_MENU,
        "win" | "lwin" | "super" => VK_LWIN,
        s if s.len() == 1 => {
            let c = s.chars().next().unwrap();
            if c.is_ascii_alphanumeric() {
                VIRTUAL_KEY(c.to_ascii_uppercase() as u16)
            } else {
                return Err(format!("Unsupported single char: {c}"));
            }
        }
        other => return Err(format!("Unknown key: {other}")),
    };
    Ok(vk.0)
}

fn foreground_class() -> Option<String> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_invalid() {
            return None;
        }
        let mut buf = [0u16; 256];
        let len = GetClassNameW(hwnd, &mut buf);
        if len <= 0 {
            return None;
        }
        Some(String::from_utf16_lossy(&buf[..len as usize]))
    }
}

fn make_input(vk: u16, key_up: bool) -> INPUT {
    // 物理キー入力では VK と ScanCode の両方が立つ。一部アプリは scan code を
    // 区別するため、MapVirtualKey で動的に補完して物理入力に近づける。
    let scan = unsafe { MapVirtualKeyW(vk as u32, MAPVK_VK_TO_VSC) } as u16;
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: scan,
                dwFlags: if key_up {
                    KEYEVENTF_KEYUP
                } else {
                    KEYBD_EVENT_FLAGS(0)
                },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}
