use crate::protocol::{Message, MessageLevel, ShellAction};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const PROXY_DIR: &str = r"C:\Main\Project\en-hancer-proxy";
const PROXY_ADDR: &str = "http://127.0.0.1:18080";

fn pid_file() -> PathBuf {
    pid_file_in(Path::new(PROXY_DIR))
}

fn pid_file_in(dir: &Path) -> PathBuf {
    dir.join(".en-hancer-proxy.pid")
}

fn is_process_running(pid: u32) -> bool {
    Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH"])
        .output()
        .map(|o| {
            let out = String::from_utf8_lossy(&o.stdout);
            out.contains(&pid.to_string())
        })
        .unwrap_or(false)
}

fn read_pid() -> Option<u32> {
    read_pid_at(&pid_file())
}

fn read_pid_at(pf: &Path) -> Option<u32> {
    fs::read_to_string(pf)
        .ok()
        .and_then(|s| s.trim().parse().ok())
}

pub fn run(args: &[String]) {
    let sub = args.first().map(|s| s.as_str()).unwrap_or("");
    match sub {
        "on" => on(),
        "off" => off(),
        "log" => log(),
        _ => status(),
    }
}

fn on() {
    start_in(Path::new(PROXY_DIR)).print_and_exit();
}

/// Starts the proxy from its checkout at `dir`, unless the PID file there names a
/// running process.
fn start_in(dir: &Path) -> ShellAction {
    let pid_path = pid_file_in(dir);
    if let Some(pid) = read_pid_at(&pid_path) {
        if is_process_running(pid) {
            return ShellAction {
                messages: vec![Message {
                    text: format!("Already running (PID: {pid})"),
                    level: MessageLevel::Warn,
                }],
                ..Default::default()
            };
        }
    }

    if !dir.is_dir() {
        return ShellAction::error(format!("Proxy checkout not found: {}", dir.display()));
    }
    let create_log = |name: &str| {
        let path = dir.join(name);
        fs::File::create(&path).map_err(|e| format!("Cannot create {}: {e}", path.display()))
    };
    let (log, err_log) = match (create_log("proxy-run.log"), create_log("proxy-run.err.log")) {
        (Ok(log), Ok(err_log)) => (log, err_log),
        (Err(e), _) | (_, Err(e)) => return ShellAction::error(e),
    };
    let target_dir = std::env::temp_dir().join("en-hancer-proxy-target");

    let child = Command::new("cargo")
        .arg("run")
        .current_dir(dir)
        .env("CARGO_TARGET_DIR", &target_dir)
        .env("RUST_LOG", "info")
        .stdout(log)
        .stderr(err_log)
        .spawn();
    let mut child = match child {
        Ok(child) => child,
        Err(e) => return ShellAction::error(format!("Failed to start proxy: {e}")),
    };

    let pid = child.id();
    if let Err(e) = fs::write(&pid_path, pid.to_string()) {
        // Without the PID file `proxy off` could not stop it.
        let _ = child.kill();
        return ShellAction::error(format!("Cannot write {}: {e}", pid_path.display()));
    }

    let mut env = HashMap::new();
    env.insert("HTTP_PROXY".into(), PROXY_ADDR.into());
    env.insert("HTTPS_PROXY".into(), PROXY_ADDR.into());
    env.insert("ALL_PROXY".into(), PROXY_ADDR.into());
    env.insert("NO_PROXY".into(), "localhost,127.0.0.1".into());

    ShellAction {
        set_env: env,
        messages: vec![Message {
            text: format!("Proxy started (PID: {pid})"),
            level: MessageLevel::Info,
        }],
        ..Default::default()
    }
}

fn off() {
    let pf = pid_file();
    let Some(pid) = read_pid() else {
        ShellAction {
            messages: vec![Message {
                text: "Not running".into(),
                level: MessageLevel::Warn,
            }],
            ..Default::default()
        }
        .print();
        return;
    };

    // Kill process
    let _ = Command::new("taskkill")
        .args(["/F", "/PID", &pid.to_string()])
        .output();
    let _ = fs::remove_file(&pf);

    ShellAction {
        unset_env: vec![
            "HTTP_PROXY".into(),
            "HTTPS_PROXY".into(),
            "ALL_PROXY".into(),
            "NO_PROXY".into(),
        ],
        messages: vec![Message {
            text: "Proxy stopped".into(),
            level: MessageLevel::Info,
        }],
        ..Default::default()
    }
    .print();
}

fn status() {
    let Some(pid) = read_pid() else {
        ShellAction {
            messages: vec![Message {
                text: "Not running".into(),
                level: MessageLevel::Warn,
            }],
            ..Default::default()
        }
        .print();
        return;
    };

    if is_process_running(pid) {
        ShellAction {
            messages: vec![Message {
                text: format!("Running (PID: {pid})"),
                level: MessageLevel::Info,
            }],
            ..Default::default()
        }
        .print();
    } else {
        let _ = fs::remove_file(pid_file());
        ShellAction {
            messages: vec![Message {
                text: "Stale PID file, cleaning up".into(),
                level: MessageLevel::Warn,
            }],
            ..Default::default()
        }
        .print();
    }
}

fn log() {
    // log は直接表示（JSON プロトコル経由でなく stdout 直接）
    let log_path = PathBuf::from(PROXY_DIR).join("proxy-run.log");
    if let Ok(content) = fs::read_to_string(&log_path) {
        let lines: Vec<&str> = content.lines().collect();
        let start = lines.len().saturating_sub(30);
        for line in &lines[start..] {
            println!("{line}");
        }
    } else {
        eprintln!("No log file found");
    }
}

#[cfg(test)]
mod tests {
    use super::start_in;

    /// A missing checkout is an error with exit code 1 that starts nothing.
    #[test]
    fn missing_checkout_is_an_error() {
        let dir = std::env::temp_dir().join("dotcli-proxy-test-no-such-checkout");
        assert!(!dir.exists());
        let a = start_in(&dir);
        assert_eq!(a.exit_code, 1);
        assert!(a.set_env.is_empty());
        let json = serde_json::to_value(&a).unwrap();
        assert_eq!(json["messages"][0]["level"], "error");
        assert_eq!(
            json["messages"][0]["text"],
            format!("Proxy checkout not found: {}", dir.display())
        );
        assert!(!dir.exists());
    }
}
