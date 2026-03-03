use crate::protocol::{Message, MessageLevel, ShellAction};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

const PROXY_DIR: &str = r"C:\Main\Project\en-hancer-proxy";
const PROXY_ADDR: &str = "http://127.0.0.1:18080";

fn pid_file() -> PathBuf {
    PathBuf::from(PROXY_DIR).join(".en-hancer-proxy.pid")
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
    let pf = pid_file();
    fs::read_to_string(&pf)
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
    if let Some(pid) = read_pid() {
        if is_process_running(pid) {
            ShellAction {
                messages: vec![Message {
                    text: format!("Already running (PID: {pid})"),
                    level: MessageLevel::Warn,
                }],
                ..Default::default()
            }
            .print();
            return;
        }
    }

    let dir = PathBuf::from(PROXY_DIR);
    let target_dir = std::env::temp_dir().join("en-hancer-proxy-target");

    let child = Command::new("cargo")
        .arg("run")
        .current_dir(&dir)
        .env("CARGO_TARGET_DIR", &target_dir)
        .env("RUST_LOG", "info")
        .stdout(fs::File::create(dir.join("proxy-run.log")).expect("log file"))
        .stderr(fs::File::create(dir.join("proxy-run.err.log")).expect("err log file"))
        .spawn();

    match child {
        Ok(child) => {
            let pid = child.id();
            fs::write(pid_file(), pid.to_string()).expect("write pid");

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
            .print();
        }
        Err(e) => {
            ShellAction {
                messages: vec![Message {
                    text: format!("Failed to start proxy: {e}"),
                    level: MessageLevel::Error,
                }],
                exit_code: 1,
                ..Default::default()
            }
            .print();
        }
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
