use crate::protocol::{ExecCommand, Message, MessageLevel, ShellAction};
use rayon::prelude::*;
use serde_json::Value;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

struct SessionInfo {
    session_id: String,
    cwd: String,
    timestamp: String,
    title: Option<String>,
    first_message: String,
    project: String,
}

pub fn select(query: &[String]) -> ShellAction {
    let sessions = scan_sessions();
    if sessions.is_empty() {
        return ShellAction {
            messages: vec![Message {
                text: "No Claude Code sessions found under ~/.claude/projects".into(),
                level: MessageLevel::Error,
            }],
            exit_code: 1,
            ..Default::default()
        };
    }

    let proj_width = sessions.iter().map(|s| s.project.len()).max().unwrap_or(10);

    let mut lines: Vec<String> = Vec::with_capacity(sessions.len());
    for (i, s) in sessions.iter().enumerate() {
        let display = s
            .title
            .as_deref()
            .unwrap_or(&s.first_message)
            .replace('\t', " ")
            .replace('\n', " ");
        let ts = format_timestamp(&s.timestamp);
        let line = format!(
            "{i}\t{:<pw$}  {}  {}",
            s.project,
            ts,
            display,
            pw = proj_width,
        );
        lines.push(line);
    }

    let q = query.join(" ");
    let mut cmd = Command::new("fzf");
    cmd.args(["-d", "\t", "--with-nth", "2..", "--no-sort"]);
    if !q.is_empty() {
        cmd.args(["--query", &q]);
    }
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped());

    let mut fzf = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return ShellAction {
                messages: vec![Message {
                    text: format!("Failed to start fzf: {e}"),
                    level: MessageLevel::Error,
                }],
                exit_code: 1,
                ..Default::default()
            };
        }
    };

    if let Some(mut stdin) = fzf.stdin.take() {
        let _ = stdin.write_all(lines.join("\n").as_bytes());
    }

    let output = match fzf.wait_with_output() {
        Ok(o) => o,
        Err(e) => {
            return ShellAction {
                messages: vec![Message {
                    text: format!("fzf failed: {e}"),
                    level: MessageLevel::Error,
                }],
                exit_code: 1,
                ..Default::default()
            };
        }
    };

    if !output.status.success() {
        return ShellAction {
            exit_code: output.status.code().unwrap_or(1),
            ..Default::default()
        };
    }

    let selected = String::from_utf8_lossy(&output.stdout);
    let idx: usize = selected
        .trim()
        .split('\t')
        .next()
        .unwrap_or("")
        .parse()
        .unwrap_or(usize::MAX);

    let Some(session) = sessions.get(idx) else {
        return ShellAction {
            exit_code: 1,
            ..Default::default()
        };
    };

    let cwd = session.cwd.clone();
    let session_id = session.session_id.clone();

    ShellAction {
        cd: Some(cwd),
        exec: Some(ExecCommand {
            program: "claude".into(),
            args: vec!["--resume".into(), session_id.into()],
        }),
        ..Default::default()
    }
}

fn scan_sessions() -> Vec<SessionInfo> {
    let projects_dir = match dirs::home_dir() {
        Some(h) => h.join(".claude").join("projects"),
        None => return Vec::new(),
    };
    if !projects_dir.is_dir() {
        return Vec::new();
    }

    let paths: Vec<PathBuf> = collect_session_jsonl(&projects_dir);
    let mut sessions: Vec<SessionInfo> =
        paths.par_iter().filter_map(|p| parse_session(p)).collect();

    sessions.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    sessions
}

fn collect_session_jsonl(projects_dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(projects_dir) else {
        return out;
    };
    for e in entries.flatten() {
        let p = e.path();
        if !p.is_dir() {
            continue;
        }
        let Ok(files) = fs::read_dir(&p) else {
            continue;
        };
        for f in files.flatten() {
            let fp = f.path();
            if fp.extension().is_some_and(|ex| ex == "jsonl")
                && !fp.to_string_lossy().contains("subagents")
            {
                out.push(fp);
            }
        }
    }
    out
}

fn parse_session(path: &Path) -> Option<SessionInfo> {
    let data = fs::read_to_string(path).ok()?;
    let lines: Vec<&str> = data.lines().collect();
    if lines.is_empty() {
        return None;
    }

    let mut session_id: Option<String> = None;
    let mut cwd: Option<String> = None;
    let mut timestamp: Option<String> = None;
    let mut first_message: Option<String> = None;

    for line in lines.iter().take(5) {
        let v: Value = serde_json::from_str(line).ok()?;
        if session_id.is_none()
            && let Some(sid) = v.get("sessionId").and_then(|x| x.as_str())
        {
            session_id = Some(sid.to_string());
        }
        if v.get("type").and_then(|t| t.as_str()) != Some("user") {
            continue;
        }
        if v.get("isMeta").and_then(|m| m.as_bool()) == Some(true) {
            continue;
        }
        let msg = match v.get("message") {
            Some(m) => m,
            None => continue,
        };
        if msg.get("role").and_then(|r| r.as_str()) != Some("user") {
            continue;
        }
        if cwd.is_none() {
            cwd = v.get("cwd").and_then(|c| c.as_str()).map(|s| s.to_string());
            timestamp = v
                .get("timestamp")
                .and_then(|t| t.as_str())
                .map(|s| s.to_string());
            if let Some(sid) = v.get("sessionId").and_then(|x| x.as_str()) {
                session_id = Some(sid.to_string());
            }
        }
        if let Some(fc) = extract_user_content(msg) {
            first_message = Some(truncate_message(fc, 80));
            break;
        }
    }

    let session_id = session_id?;
    let cwd = cwd?;
    let timestamp = timestamp.unwrap_or_default();

    let title = lines.iter().rev().find_map(|line| {
        let v: Value = serde_json::from_str(line).ok()?;
        if v.get("type").and_then(|t| t.as_str()) == Some("custom-title") {
            return v
                .get("customTitle")
                .and_then(|t| t.as_str())
                .map(|s| s.to_string());
        }
        None
    });

    let first_message = first_message.unwrap_or_else(|| "(no user message)".into());

    let project = project_label(&cwd);

    Some(SessionInfo {
        session_id,
        cwd,
        timestamp,
        title,
        first_message,
        project,
    })
}

fn project_label(cwd: &str) -> String {
    let path = Path::new(cwd.trim());
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| cwd.to_string())
}

fn extract_user_content(msg: &Value) -> Option<String> {
    let content = msg.get("content")?;
    if let Some(s) = content.as_str() {
        return Some(s.to_string());
    }
    if let Some(arr) = content.as_array() {
        let mut out = String::new();
        for item in arr {
            if let Some(t) = item.get("text").and_then(|t| t.as_str()) {
                if !out.is_empty() {
                    out.push(' ');
                }
                out.push_str(t);
            }
        }
        if !out.is_empty() {
            return Some(out);
        }
    }
    None
}

/// "2026-04-03T11:05:19.155Z" → " 4/ 3 20:05" (UTC+9)
fn format_timestamp(ts: &str) -> String {
    if ts.len() >= 16 {
        let mm: u32 = ts[5..7].parse().unwrap_or(0);
        let dd: u32 = ts[8..10].parse().unwrap_or(0);
        let hh: u32 = ts[11..13].parse().unwrap_or(0);
        let mn = &ts[14..16];

        let (hh, dd, mm) = {
            let h = hh + 9;
            if h >= 24 {
                let d = dd + 1;
                let days_in_month = match mm {
                    2 => 28,
                    4 | 6 | 9 | 11 => 30,
                    _ => 31,
                };
                if d > days_in_month {
                    (h - 24, 1, mm + 1)
                } else {
                    (h - 24, d, mm)
                }
            } else {
                (h, dd, mm)
            }
        };

        return format!("{mm:2}/{dd:2} {hh:2}:{mn}");
    }
    ts.to_string()
}

fn truncate_message(s: String, max: usize) -> String {
    let mut it = s.chars();
    let mut out: String = it.by_ref().take(max).collect();
    if it.next().is_some() {
        out.push('…');
    }
    out
}
