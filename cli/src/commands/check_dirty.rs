use std::path::{Path, PathBuf};
use std::process::Command;

pub fn run(file_path: &str) {
    if let Some(msg) = check(file_path) {
        println!(
            r#"{{"hookSpecificOutput":{{"hookEventName":"PreToolUse","permissionDecision":"allow","additionalContext":{}}}}}"#,
            json_string(&msg)
        );
    }
}

fn check(file_path: &str) -> Option<String> {
    let file = Path::new(file_path);
    let parent = file.parent()?;
    let repo_dir = git_toplevel(parent)?;
    let rel = relative_path(&repo_dir, file)?;

    let unstaged = git_diff(&repo_dir, &["diff", "--stat", "--", &rel]);
    let staged = git_diff(&repo_dir, &["diff", "--cached", "--stat", "--", &rel]);

    if unstaged.is_empty() && staged.is_empty() {
        return None;
    }

    let mut msg = String::from("This file has uncommitted changes");
    if !staged.is_empty() {
        msg.push_str(" (includes staged)");
    }
    msg.push_str(" — editing may mix unrelated changes into a single commit. Consider committing first.");
    Some(msg)
}

fn git_toplevel(cwd: &Path) -> Option<PathBuf> {
    let out = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(cwd)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() { None } else { Some(PathBuf::from(s)) }
}

fn git_diff(repo_dir: &Path, args: &[&str]) -> String {
    Command::new("git")
        .args(args)
        .current_dir(repo_dir)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

fn relative_path(repo: &Path, file: &Path) -> Option<String> {
    let repo_s = normalize(&repo.to_string_lossy());
    let file_s = normalize(&file.to_string_lossy());
    let prefix = if repo_s.ends_with('/') {
        repo_s.clone()
    } else {
        format!("{}/", repo_s)
    };
    let repo_lower = prefix.to_lowercase();
    let file_lower = file_s.to_lowercase();
    if !file_lower.starts_with(&repo_lower) {
        return None;
    }
    Some(file_s[prefix.len()..].to_string())
}

fn normalize(p: &str) -> String {
    let s = p.replace('\\', "/");
    // Handle msys-style /c/... -> c:/...
    if s.len() >= 3 {
        let bytes = s.as_bytes();
        if bytes[0] == b'/' && bytes[1].is_ascii_alphabetic() && bytes[2] == b'/' {
            return format!("{}:/{}", bytes[1] as char, &s[3..]);
        }
    }
    s
}

fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
