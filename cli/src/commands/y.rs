use crate::protocol::ShellAction;
use std::process::{Command, Stdio};

pub fn run(args: &[String]) {
    let tmp = std::env::temp_dir().join(format!("yazi-cwd-{}.tmp", std::process::id()));

    // yazi needs direct terminal access for TUI — inherit stdio
    // stdout must NOT be piped (shell pipes dotcli stdout to _dotcli_apply)
    // so we redirect dotcli's stdout to stderr temporarily, and use stderr for yazi's TUI
    let status = Command::new("yazi")
        .args(args)
        .arg(format!("--cwd-file={}", tmp.display()))
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status();

    if let Ok(s) = status {
        if s.success() || s.code() == Some(0) {
            if let Ok(cwd) = std::fs::read_to_string(&tmp) {
                let cwd = cwd.trim();
                let pwd = std::env::current_dir()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default();
                if !cwd.is_empty() && cwd != pwd {
                    let action = ShellAction {
                        cd: Some(cwd.to_string()),
                        ..Default::default()
                    };
                    action.print();
                }
            }
        }
    }

    let _ = std::fs::remove_file(&tmp);
}
