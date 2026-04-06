use crate::protocol::{ExecCommand, Message, MessageLevel, ShellAction};
use rayon::prelude::*;
use serde_json::Value;
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub(crate) const HIGHLIGHT_TOP_N: usize = 5;
pub(crate) const GREEN: &str = "\x1b[32m";
pub(crate) const DIM: &str = "\x1b[2m";
pub(crate) const RESET: &str = "\x1b[0m";

pub(crate) struct SessionInfo {
    pub(crate) session_id: String,
    pub(crate) cwd: String,
    pub(crate) timestamp: String,
    pub(crate) title: Option<String>,
    pub(crate) latest_message: String,
    pub(crate) project: String,
}

struct ScanResult {
    sessions: Vec<SessionInfo>,
    no_last_ts: usize,
}

pub fn select(query: &[String]) -> ShellAction {
    use crate::commands::titles;
    use std::collections::HashMap;

    let scan = scan_sessions();
    let sessions = scan.sessions;
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

    let cwd_now = env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    // Load cached AI titles
    let cached_titles: HashMap<String, String> = sessions
        .iter()
        .filter_map(|s| {
            titles::read_cached_title(&s.session_id).map(|t| (s.session_id.clone(), t))
        })
        .collect();

    // Partition: pwd-match (up to N) first, then the rest
    let mut pwd_group: Vec<(usize, &SessionInfo)> = Vec::new();
    let mut rest: Vec<(usize, &SessionInfo)> = Vec::new();
    for (i, s) in sessions.iter().enumerate() {
        if pwd_group.len() < HIGHLIGHT_TOP_N && paths_equal(&s.cwd, &cwd_now) {
            pwd_group.push((i, s));
        } else {
            rest.push((i, s));
        }
    }

    let proj_width = sessions
        .iter()
        .map(|s| s.project.len())
        .max()
        .unwrap_or(10)
        .min(MAX_PROJECT_WIDTH);

    let mut lines: Vec<String> = Vec::with_capacity(sessions.len());
    for &(i, s) in &pwd_group {
        lines.push(format_line(
            i,
            s,
            proj_width,
            GREEN,
            cached_titles.get(&s.session_id).map(|t| t.as_str()),
        ));
    }
    // 現在時刻（epoch秒）
    let now_epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    if !pwd_group.is_empty() && !rest.is_empty() {
        let label = project_label(&cwd_now);
        let sep = format!(
            "{}\t{}── {} recent ({}) ──{}",
            usize::MAX, DIM, pwd_group.len(), label, RESET
        );
        lines.push(sep);
    }
    push_rest_with_groups(&mut lines, &rest, proj_width, &cached_titles, now_epoch);

    // Collect uncached sessions for background generation
    let home = dirs::home_dir().expect("no home dir");
    let projects_dir = home.join(".claude").join("projects");
    let all_jsonl = collect_session_jsonl(&projects_dir);
    let needs_gen: Vec<(PathBuf, String)> = sessions
        .iter()
        .filter(|s| s.title.is_none() && !cached_titles.contains_key(&s.session_id))
        .filter_map(|s| {
            all_jsonl
                .iter()
                .find(|p| {
                    p.file_stem()
                        .map(|f| f.to_string_lossy().contains(&s.session_id))
                        .unwrap_or(false)
                })
                .map(|p| (p.clone(), s.session_id.clone()))
        })
        .collect();

    // Allocate fzf listen port and tmp file for reload
    let fzf_port = titles::allocate_port();
    let tmp_path = env::temp_dir().join(format!("dotcli-fzf-{}.tmp", std::process::id()));

    let q = query.join(" ");
    let mut cmd = Command::new("fzf");
    cmd.args([
        "-d",
        "\t",
        "--with-nth",
        "2..",
        "--no-sort",
        "--ansi",
        "--listen-unsafe",
        &fzf_port.to_string(),
    ]);
    if scan.no_last_ts > 0 {
        let header = format!("⚠ {} sessions skipped (no last timestamp)", scan.no_last_ts);
        cmd.args(["--header", &header]);
    }
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

    // Spawn background title generation thread
    let bg_tmp = tmp_path.clone();
    let bg_sessions: Vec<SessionInfo> = sessions
        .iter()
        .map(|s| SessionInfo {
            session_id: s.session_id.clone(),
            cwd: s.cwd.clone(),
            timestamp: s.timestamp.clone(),
            title: s.title.clone(),
            latest_message: s.latest_message.clone(),
            project: s.project.clone(),
        })
        .collect();
    let bg_cwd = cwd_now.clone();
    std::thread::spawn(move || {
        titles::background_generate(titles::BgGenParams {
            needs_gen,
            sessions: bg_sessions,
            fzf_port,
            tmp_path: bg_tmp,
            proj_width,
            cwd_now: bg_cwd,
        });
    });

    let output = match fzf.wait_with_output() {
        Ok(o) => o,
        Err(e) => {
            let _ = fs::remove_file(&tmp_path);
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

    let _ = fs::remove_file(&tmp_path);

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

fn scan_sessions() -> ScanResult {
    let projects_dir = match dirs::home_dir() {
        Some(h) => h.join(".claude").join("projects"),
        None => return ScanResult { sessions: Vec::new(), no_last_ts: 0 },
    };
    if !projects_dir.is_dir() {
        return ScanResult { sessions: Vec::new(), no_last_ts: 0 };
    }

    let paths: Vec<PathBuf> = collect_session_jsonl(&projects_dir);
    let results: Vec<Result<SessionInfo, bool>> =
        paths.par_iter().map(|p| parse_session(p)).collect();
    let mut no_last_ts: usize = 0;
    let mut sessions: Vec<SessionInfo> = Vec::with_capacity(results.len());
    for r in results {
        match r {
            Ok(s) => sessions.push(s),
            Err(true) => no_last_ts += 1,
            Err(false) => {}
        }
    }

    sessions.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    ScanResult { sessions, no_last_ts }
}

pub(crate) fn collect_session_jsonl(projects_dir: &Path) -> Vec<PathBuf> {
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

/// Returns Ok(session) on success, Err(true) if only last_timestamp was missing,
/// Err(false) for other parse failures.
fn parse_session(path: &Path) -> Result<SessionInfo, bool> {
    let data = fs::read_to_string(path).map_err(|_| false)?;
    let lines: Vec<&str> = data.lines().collect();
    if lines.is_empty() {
        return Err(false);
    }

    let mut session_id: Option<String> = None;
    let mut cwd: Option<String> = None;

    for line in lines.iter().take(10) {
        let v: Value = serde_json::from_str(line).map_err(|_| false)?;
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
            if let Some(sid) = v.get("sessionId").and_then(|x| x.as_str()) {
                session_id = Some(sid.to_string());
            }
        }
        if cwd.is_some() && session_id.is_some() {
            break;
        }
    }

    let session_id = session_id.ok_or(false)?;
    let cwd = cwd.ok_or(false)?;

    let mut title: Option<String> = None;
    let mut last_timestamp: Option<String> = None;
    let mut latest_message: Option<String> = None;
    for line in lines.iter().rev() {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if last_timestamp.is_none() {
            last_timestamp = v
                .get("timestamp")
                .and_then(|t| t.as_str())
                .map(|s| s.to_string());
        }
        if title.is_none()
            && v.get("type").and_then(|t| t.as_str()) == Some("custom-title")
        {
            title = v
                .get("customTitle")
                .and_then(|t| t.as_str())
                .map(|s| s.to_string());
        }
        if latest_message.is_none()
            && v.get("type").and_then(|t| t.as_str()) == Some("user")
            && v.get("isMeta").and_then(|m| m.as_bool()) != Some(true)
        {
            if let Some(msg) = v.get("message") {
                if msg.get("role").and_then(|r| r.as_str()) == Some("user") {
                    if let Some(text) = extract_user_content(msg) {
                        let cleaned = strip_xml_tags(&text);
                        if !cleaned.is_empty() {
                            latest_message = Some(truncate_message(cleaned, 120));
                        }
                    }
                }
            }
        }
        if title.is_some() && last_timestamp.is_some() && latest_message.is_some() {
            break;
        }
    }

    let latest_message = latest_message.unwrap_or_else(|| "(no user message)".into());
    let timestamp = last_timestamp.ok_or(true)?;

    let project = project_label(&cwd);

    Ok(SessionInfo {
        session_id,
        cwd,
        timestamp,
        title,
        latest_message,
        project,
    })
}

pub(crate) fn format_line(
    i: usize,
    s: &SessionInfo,
    proj_width: usize,
    color: &str,
    ai_title: Option<&str>,
) -> String {
    let display = s
        .title
        .as_deref()
        .or(ai_title)
        .unwrap_or(&s.latest_message)
        .replace('\t', " ")
        .replace('\n', " ");
    let ts = format_timestamp(&s.timestamp);
    if color.is_empty() {
        format!(
            "{i}\t{:<pw$}  {}  {}",
            s.project,
            ts,
            display,
            pw = proj_width,
        )
    } else {
        format!(
            "{i}\t{color}{:<pw$}  {}  {}{RESET}",
            s.project,
            ts,
            display,
            pw = proj_width,
        )
    }
}

/// Compare paths ignoring slash direction (Windows vs Unix)
pub(crate) fn paths_equal(a: &str, b: &str) -> bool {
    let normalize = |s: &str| s.replace('\\', "/").to_lowercase();
    normalize(a) == normalize(b)
}

pub(crate) const MAX_PROJECT_WIDTH: usize = 15;

pub(crate) fn project_label(cwd: &str) -> String {
    let name = Path::new(cwd.trim())
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| cwd.to_string());
    truncate_str(&name, MAX_PROJECT_WIDTH)
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max - 1).collect();
        out.push('…');
        out
    }
}

pub(crate) fn extract_user_content(msg: &Value) -> Option<String> {
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

/// "2026-04-03T11:05:19.155Z" → epoch秒 (UTC)
fn parse_epoch(ts: &str) -> Option<i64> {
    if ts.len() < 19 {
        return None;
    }
    let y: i64 = ts[0..4].parse().ok()?;
    let mo: i64 = ts[5..7].parse().ok()?;
    let d: i64 = ts[8..10].parse().ok()?;
    let h: i64 = ts[11..13].parse().ok()?;
    let mi: i64 = ts[14..16].parse().ok()?;
    let s: i64 = ts[17..19].parse().ok()?;

    // days from epoch (1970-01-01) — simplified, no leap second
    let mut days: i64 = 0;
    for yr in 1970..y {
        days += if is_leap(yr) { 366 } else { 365 };
    }
    let month_days = [
        31,
        if is_leap(y) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    for m in 0..(mo - 1) as usize {
        days += month_days[m] as i64;
    }
    days += d - 1;

    Some(days * 86400 + h * 3600 + mi * 60 + s)
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

/// セッションのタイムスタンプからグループラベルを返す
/// now_epoch: 現在のUTC epoch秒
pub(crate) fn time_group_label(ts: &str, now_epoch: i64) -> &'static str {
    let Some(ts_epoch) = parse_epoch(ts) else {
        return "それ以前";
    };
    let diff = now_epoch - ts_epoch;

    // 時間差ベースの細かい分類（6h未満）
    if diff < 3600 {
        return "1時間以内";
    }
    if diff < 3600 * 3 {
        return "3時間以内";
    }
    if diff < 3600 * 6 {
        return "6時間以内";
    }

    // 日付ベースの分類（JST = UTC+9）
    let jst_offset: i64 = 9 * 3600;
    let now_jst = now_epoch + jst_offset;
    let ts_jst = ts_epoch + jst_offset;

    let now_day = now_jst / 86400; // JST日数
    let ts_day = ts_jst / 86400;
    let day_diff = now_day - ts_day;

    if day_diff == 0 {
        return "今日";
    }
    if day_diff == 1 {
        return "昨日";
    }
    if day_diff <= 3 {
        return "2-3日前";
    }

    // 曜日: 1970-01-01 = 木曜 → (day + 3) % 7: 0=月
    let now_weekday = (now_day + 3) % 7; // 0=Mon
    let this_monday = now_day - now_weekday;
    let last_monday = this_monday - 7;

    if ts_day >= this_monday {
        return "今週";
    }
    if ts_day >= last_monday {
        return "先週";
    }

    // 月ベース
    let now_month = now_jst_month(now_jst);
    let ts_month = now_jst_month(ts_jst);

    if now_month == ts_month {
        return "今月";
    }
    if now_month == ts_month + 1 || (now_month == 1 && ts_month == 12) {
        return "先月";
    }
    "それ以前"
}

/// JST epoch秒 → year*12+month (比較用)
fn now_jst_month(jst_epoch: i64) -> i32 {
    // 簡易: epoch日数から年月を逆算
    let days = jst_epoch / 86400;
    let mut y = 1970i32;
    let mut rem = days;
    loop {
        let yd = if is_leap(y as i64) { 366 } else { 365 };
        if rem < yd {
            break;
        }
        rem -= yd;
        y += 1;
    }
    let month_days = [
        31,
        if is_leap(y as i64) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut m = 0i32;
    for md in &month_days {
        if rem < *md as i64 {
            break;
        }
        rem -= *md as i64;
        m += 1;
    }
    y * 12 + m
}

/// restリストにタイムグループセパレータを挿入してlines に追加
pub(crate) fn push_rest_with_groups(
    lines: &mut Vec<String>,
    rest: &[(usize, &SessionInfo)],
    proj_width: usize,
    cached_titles: &std::collections::HashMap<String, String>,
    now_epoch: i64,
) {
    let mut current_group = "";
    for &(i, s) in rest {
        let group = time_group_label(&s.timestamp, now_epoch);
        if group != current_group {
            if !current_group.is_empty() {
                lines.push(format!("{}\t{}{}── {} ──{}", usize::MAX, GREEN, DIM, current_group, RESET));
            }
            current_group = group;
        }
        lines.push(format_line(
            i,
            s,
            proj_width,
            "",
            cached_titles.get(&s.session_id).map(|t| t.as_str()),
        ));
    }
    if !current_group.is_empty() {
        lines.push(format!("{}\t{}{}── {} ──{}", usize::MAX, GREEN, DIM, current_group, RESET));
    }
}

/// Strip XML/HTML tags and collapse whitespace
pub(crate) fn strip_xml_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut inside = false;
    for c in s.chars() {
        match c {
            '<' => inside = true,
            '>' => inside = false,
            _ if !inside => out.push(c),
            _ => {}
        }
    }
    // collapse whitespace
    let trimmed = out.split_whitespace().collect::<Vec<_>>().join(" ");
    trimmed
}

pub(crate) fn truncate_message(s: String, max: usize) -> String {
    let mut it = s.chars();
    let mut out: String = it.by_ref().take(max).collect();
    if it.next().is_some() {
        out.push('…');
    }
    out
}

// ── Title input extraction ──

#[allow(dead_code)]
pub(crate) struct TitleInput {
    pub session_id: String,
    pub first_message: String,
    pub first_assistant: String,
    pub latest_message: String,
    pub latest_assistant: String,
}

pub(crate) fn extract_title_input(path: &Path) -> Option<TitleInput> {
    let data = fs::read_to_string(path).ok()?;
    let lines: Vec<&str> = data.lines().collect();

    let mut session_id: Option<String> = None;
    let mut first_message: Option<String> = None;
    let mut first_assistant: Option<String> = None;

    // Forward pass: first 50 lines
    for line in lines.iter().take(50) {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if session_id.is_none() {
            if let Some(sid) = v.get("sessionId").and_then(|x| x.as_str()) {
                session_id = Some(sid.to_string());
            }
        }
        let typ = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
        if first_message.is_none() && typ == "user" {
            if v.get("isMeta").and_then(|m| m.as_bool()) == Some(true) {
                continue;
            }
            if let Some(msg) = v.get("message") {
                if msg.get("role").and_then(|r| r.as_str()) == Some("user") {
                    if let Some(text) = extract_user_content(msg) {
                        let cleaned = strip_xml_tags(&text);
                        if !cleaned.is_empty() {
                            first_message = Some(truncate_message(cleaned, 200));
                        }
                    }
                }
            }
        } else if first_message.is_some() && first_assistant.is_none() && typ == "assistant" {
            if let Some(text) = extract_assistant_text(&v) {
                first_assistant = Some(text);
            }
        }
        if first_assistant.is_some() {
            break;
        }
    }

    let session_id = session_id?;
    let first_message = first_message?;
    let first_assistant = first_assistant.unwrap_or_else(|| "(なし)".into());

    // Reverse pass: latest messages
    let mut latest_message: Option<String> = None;
    let mut latest_assistant: Option<String> = None;

    for line in lines.iter().rev() {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let typ = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
        if latest_message.is_none() && typ == "user" {
            if v.get("isMeta").and_then(|m| m.as_bool()) == Some(true) {
                continue;
            }
            if let Some(msg) = v.get("message") {
                if msg.get("role").and_then(|r| r.as_str()) == Some("user") {
                    if let Some(text) = extract_user_content(msg) {
                        let cleaned = strip_xml_tags(&text);
                        if !cleaned.is_empty() {
                            latest_message = Some(truncate_message(cleaned, 200));
                        }
                    }
                }
            }
        }
        if latest_assistant.is_none() && typ == "assistant" {
            if let Some(text) = extract_assistant_text(&v) {
                latest_assistant = Some(text);
            }
        }
        if latest_message.is_some() && latest_assistant.is_some() {
            break;
        }
    }

    Some(TitleInput {
        session_id,
        first_message,
        first_assistant,
        latest_message: latest_message.unwrap_or_else(|| "(なし)".into()),
        latest_assistant: latest_assistant.unwrap_or_else(|| "(なし)".into()),
    })
}

fn extract_assistant_text(v: &Value) -> Option<String> {
    let content = v.get("message")?.get("content")?;
    let raw = if let Some(s) = content.as_str() {
        s.to_string()
    } else if let Some(arr) = content.as_array() {
        let mut out = String::new();
        for item in arr {
            if item.get("type").and_then(|t| t.as_str()) == Some("text") {
                if let Some(t) = item.get("text").and_then(|t| t.as_str()) {
                    if !out.is_empty() {
                        out.push(' ');
                    }
                    out.push_str(t);
                }
            }
        }
        if out.is_empty() {
            return None;
        }
        out
    } else {
        return None;
    };
    let cleaned = strip_xml_tags(&raw);
    if cleaned.is_empty() {
        return None;
    }
    Some(truncate_message(cleaned, 200))
}
