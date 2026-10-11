use std::process::{Command, Stdio};

/// List entries of `fd_type` (`f` / `d`) under the cwd with fd, pick one with fzf,
/// and return it. `args` go to fd as is. `None` when fd/fzf fail to start, fzf is
/// cancelled, or nothing is selected; spawn errors are reported on stderr.
pub fn fd_fzf_pick(fd_type: &str, prompt: &str, args: &[String]) -> Option<String> {
    let fd = Command::new("fd")
        .args([
            "--type", fd_type,
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
            return None;
        }
    };

    let fd_stdout = match fd.stdout {
        Some(s) => s,
        None => {
            eprintln!("fd stdout unavailable");
            return None;
        }
    };

    let fzf = Command::new("fzf")
        .args(["--height", "40%", "--reverse", "--prompt", prompt])
        .stdin(fd_stdout)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn();

    let fzf = match fzf {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to spawn fzf: {e}");
            return None;
        }
    };

    let output = match fzf.wait_with_output() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("fzf wait failed: {e}");
            return None;
        }
    };

    if !output.status.success() {
        return None;
    }

    let selected = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if selected.is_empty() {
        return None;
    }
    Some(selected)
}
