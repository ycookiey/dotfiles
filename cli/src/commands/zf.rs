use crate::protocol::ShellAction;
use std::process::{Command, Stdio};

/// fd でカレント配下のディレクトリを列挙 → fzf で選択 → cd
/// zoxide hook (pre_prompt) が cd 後に自動で履歴へ add する
pub fn run(args: &[String]) {
    let fd = Command::new("fd")
        .args([
            "--type", "d",
            "--hidden",
            "--exclude", ".git",
            "--exclude", "node_modules",
            "--exclude", "target",
        ])
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn();

    let fd = match fd {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to spawn fd: {e}");
            return;
        }
    };

    let fd_stdout = match fd.stdout {
        Some(s) => s,
        None => {
            eprintln!("fd stdout unavailable");
            return;
        }
    };

    let fzf = Command::new("fzf")
        .args(["--height", "40%", "--reverse", "--prompt", "zf> "])
        .stdin(fd_stdout)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn();

    let fzf = match fzf {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to spawn fzf: {e}");
            return;
        }
    };

    let output = match fzf.wait_with_output() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("fzf wait failed: {e}");
            return;
        }
    };

    if !output.status.success() {
        return;
    }

    let selected = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if selected.is_empty() {
        return;
    }

    let action = ShellAction {
        cd: Some(selected),
        ..Default::default()
    };
    action.print();
}
