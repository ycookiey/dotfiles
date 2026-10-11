use crate::protocol::{ExecCommand, Message, MessageLevel, ShellAction};
use rayon::prelude::*;
use serde::Deserialize;
use std::env;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

// jsonl 1行を型付き deserialize するための最小スキーマ。
// serde_json::Value より速く、simd_json::serde::from_slice と組み合わせて使う。
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct LineMeta {
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(rename = "type", default)]
    typ: Option<String>,
    #[serde(default)]
    is_meta: Option<bool>,
    #[serde(default)]
    custom_title: Option<String>,
    #[serde(default)]
    ai_title: Option<String>,
    #[serde(default)]
    message: Option<LineMessage>,
}

#[derive(Deserialize, Default)]
struct LineMessage {
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    content: Option<LineContent>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum LineContent {
    Text(String),
    Array(Vec<ContentItem>),
}

#[derive(Deserialize, Default)]
struct ContentItem {
    #[serde(default)]
    text: Option<String>,
}

/// LineMessage から user メッセージ本文を抽出（既存 extract_user_content の typed版）
fn line_user_content(msg: &LineMessage) -> Option<String> {
    let content = msg.content.as_ref()?;
    let raw = match content {
        LineContent::Text(s) => s.clone(),
        LineContent::Array(items) => {
            let mut out = String::new();
            for item in items {
                if let Some(t) = &item.text {
                    if !out.is_empty() {
                        out.push(' ');
                    }
                    out.push_str(t);
                }
            }
            if out.is_empty() {
                return None;
            }
            out
        }
    };
    let cleaned = strip_json_blobs(&raw);
    if cleaned.trim().is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

/// Parses one jsonl line in place: simd-json overwrites `line` while parsing, so a line
/// must not be parsed twice.
fn parse_line(line: &mut [u8]) -> Option<LineMeta> {
    simd_json::serde::from_slice(line).ok()
}

/// Only the top-level `cwd` of a jsonl line, or of a scan-cache `info`. Kept apart from
/// LineMeta so a line whose other fields do not fit LineMeta still yields its cwd.
#[derive(Deserialize)]
struct CwdOnly {
    #[serde(default)]
    cwd: Option<String>,
}

pub(crate) const HIGHLIGHT_TOP_N: usize = 5;
pub(crate) const GREEN: &str = "\x1b[32m";
pub(crate) const DIM: &str = "\x1b[2m";
pub(crate) const RESET: &str = "\x1b[0m";

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub(crate) struct SessionInfo {
    pub(crate) session_id: String,
    pub(crate) cwd: String,
    pub(crate) timestamp: String,
    pub(crate) title: Option<String>,
    #[serde(default)]
    pub(crate) ai_title: Option<String>,
    pub(crate) latest_message: String,
    pub(crate) project: String,
    pub(crate) path: PathBuf,
}

struct ScanResult {
    sessions: Vec<SessionInfo>,
    no_last_ts: usize,
    unparsable: usize,
}

pub(crate) fn is_session_id(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.len() != 36 {
        return false;
    }
    for (i, b) in bytes.iter().enumerate() {
        match i {
            8 | 13 | 18 | 23 => {
                if *b != b'-' {
                    return false;
                }
            }
            _ => {
                if !b.is_ascii_hexdigit() {
                    return false;
                }
            }
        }
    }
    true
}

pub fn resume_by_id(session_id: &str, dangerously_skip_permissions: bool) -> ShellAction {
    let cwd = find_session_cwd(session_id);

    let mut action = ShellAction {
        exec: Some(ExecCommand {
            program: "claude".into(),
            args: {
                let mut a = vec!["--resume".into(), session_id.into()];
                if dangerously_skip_permissions {
                    a.push("--dangerously-skip-permissions".into());
                }
                a
            },
        }),
        ..Default::default()
    };
    match cwd {
        Some(c) => action.cd = Some(c),
        None => action.messages.push(Message {
            text: format!("Session {session_id} not found locally; resuming without cd"),
            level: MessageLevel::Info,
        }),
    }
    action
}

/// cwd of `<projects>/<dir>/<session_id>.jsonl`. The scan cache (written by the `c r`
/// picker) already holds the cwd of almost every session, and a session's cwd never
/// changes, so it is used first; otherwise the head of the session file is read.
fn find_session_cwd(session_id: &str) -> Option<String> {
    let projects_dir = dirs::home_dir()?.join(".claude").join("projects");
    let target = format!("{session_id}.jsonl");
    cached_session_cwd(&read_scan_cache_cwds(), &projects_dir, &target)
        .or_else(|| scan_session_cwd(&projects_dir, &target))
}

/// cwd of the cached session whose file is `<projects_dir>/<dir>/<file_name>`, if that
/// file still exists. The same session id can sit in several project dirs (a session
/// carried into a worktree); then None, so scan_session_cwd picks one in listing order
/// as before rather than whichever the cache lists first.
fn cached_session_cwd(
    entries: &[(PathBuf, String)],
    projects_dir: &Path,
    file_name: &str,
) -> Option<String> {
    let mut matches = entries.iter().filter(|(path, _)| {
        path.file_name().is_some_and(|n| n == file_name)
            && path.parent().and_then(Path::parent) == Some(projects_dir)
    });
    let (path, cwd) = matches.next()?;
    if matches.next().is_some() || !path.is_file() {
        return None;
    }
    Some(cwd.clone())
}

/// Looks for `<projects_dir>/<dir>/<file_name>` and returns the first top-level cwd in
/// its head. Found but without a cwd → None (as before: no other dir is tried).
fn scan_session_cwd(projects_dir: &Path, file_name: &str) -> Option<String> {
    for e in fs::read_dir(projects_dir).ok()?.flatten() {
        if !entry_is_dir(&e) {
            continue;
        }
        let jsonl = e.path().join(file_name);
        if !jsonl.is_file() {
            continue;
        }
        return head_cwd(&jsonl);
    }
    None
}

/// First top-level cwd in the head of a session file, of any line type: large sessions
/// can open with many file-history-snapshot lines (no cwd), so a fixed line count is
/// not enough. Tries HEAD_BASE_BYTES, then once HEAD_MAX_BYTES.
fn head_cwd(path: &Path) -> Option<String> {
    for window in [HEAD_BASE_BYTES, HEAD_MAX_BYTES] {
        let (head, _) = read_head_and_tail(path, window, 0).ok()?;
        let found = head
            .split(|&b| b == b'\n')
            .filter(|l| !l.is_empty())
            .find_map(|l| serde_json::from_slice::<CwdOnly>(l).ok()?.cwd);
        if found.is_some() {
            return found;
        }
    }
    None
}

pub fn select(query: &[String], dangerously_skip_permissions: bool) -> ShellAction {
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
        lines.push(format_line(i, s, proj_width, GREEN));
    }
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
    push_rest_with_groups(&mut lines, &rest, proj_width, now_epoch);

    let q = query.join(" ");
    let mut cmd = Command::new("fzf");
    cmd.args(["-d", "\t", "--with-nth", "2..", "--no-sort", "--ansi"]);
    let mut header_parts: Vec<String> = Vec::new();
    if scan.no_last_ts > 0 {
        header_parts.push(format!("{} no timestamp", scan.no_last_ts));
    }
    if scan.unparsable > 0 {
        header_parts.push(format!("{} unparsable", scan.unparsable));
    }
    if !header_parts.is_empty() {
        let header = format!("⚠ skipped: {}", header_parts.join(", "));
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
            args: {
                let mut a = vec!["--resume".into(), session_id.into()];
                if dangerously_skip_permissions {
                    a.push("--dangerously-skip-permissions".into());
                }
                a
            },
        }),
        ..Default::default()
    }
}

// ── Scan cache ──
// 全 jsonl の全文 parse は ~150ms かかる。だが 1 回の `c r` で更新される
// セッションは普段 1〜2 件。なので (path, mtime) を照合して、mtime 未変更分は
// cache から SessionInfo を流用、変更/新規分のみ再 parse する。

const SCAN_CACHE_VERSION: u32 = 2;

#[derive(serde::Serialize, serde::Deserialize)]
struct ScanCache {
    version: u32,
    entries: Vec<ScanCacheEntry>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct ScanCacheEntry {
    path: PathBuf,
    mtime_ns: u128,
    info: SessionInfo,
}

fn scan_cache_path() -> Option<PathBuf> {
    Some(
        dirs::home_dir()?
            .join(".cache")
            .join("dotcli")
            .join("sessions-scan.json"),
    )
}

/// (path, cwd) of every scan-cache entry, in cache order, reading nothing else of the
/// entries. Empty when the cache is missing, unreadable or of another version.
fn read_scan_cache_cwds() -> Vec<(PathBuf, String)> {
    #[derive(Deserialize)]
    struct Cache {
        version: u32,
        entries: Vec<Entry>,
    }
    #[derive(Deserialize)]
    struct Entry {
        path: PathBuf,
        info: CwdOnly,
    }
    let Some(p) = scan_cache_path() else { return Vec::new() };
    let Ok(data) = fs::read(&p) else { return Vec::new() };
    let Ok(cache) = serde_json::from_slice::<Cache>(&data) else { return Vec::new() };
    if cache.version != SCAN_CACHE_VERSION {
        return Vec::new();
    }
    cache
        .entries
        .into_iter()
        .filter_map(|e| Some((e.path, e.info.cwd?)))
        .collect()
}

fn read_scan_cache() -> std::collections::HashMap<PathBuf, (u128, SessionInfo)> {
    use std::collections::HashMap;
    let Some(p) = scan_cache_path() else { return HashMap::new(); };
    let Ok(data) = fs::read_to_string(&p) else { return HashMap::new(); };
    let Ok(cache): Result<ScanCache, _> = serde_json::from_str(&data) else {
        return HashMap::new();
    };
    if cache.version != SCAN_CACHE_VERSION {
        return HashMap::new();
    }
    cache
        .entries
        .into_iter()
        .map(|e| (e.path, (e.mtime_ns, e.info)))
        .collect()
}

fn write_scan_cache(
    sessions: &[SessionInfo],
    mtimes: &std::collections::HashMap<PathBuf, u128>,
) {
    let Some(p) = scan_cache_path() else { return };
    let Some(dir) = p.parent() else { return };
    let _ = fs::create_dir_all(dir);
    let entries: Vec<ScanCacheEntry> = sessions
        .iter()
        .filter_map(|s| {
            let mt = mtimes.get(&s.path)?;
            Some(ScanCacheEntry {
                path: s.path.clone(),
                mtime_ns: *mt,
                info: s.clone(),
            })
        })
        .collect();
    let cache = ScanCache {
        version: SCAN_CACHE_VERSION,
        entries,
    };
    let Ok(data) = serde_json::to_string(&cache) else { return };
    let tmp = p.with_extension("json.tmp");
    if fs::write(&tmp, &data).is_ok() {
        let _ = fs::rename(&tmp, &p);
    }
}

fn scan_sessions() -> ScanResult {
    use std::collections::HashMap;

    let projects_dir = match dirs::home_dir() {
        Some(h) => h.join(".claude").join("projects"),
        None => return ScanResult { sessions: Vec::new(), no_last_ts: 0, unparsable: 0 },
    };
    if !projects_dir.is_dir() {
        return ScanResult { sessions: Vec::new(), no_last_ts: 0, unparsable: 0 };
    }

    // 1. (path, mtime_ns) を一覧から集める (mtime も列挙結果から取り、追加の stat なし)
    let current: Vec<(PathBuf, u128)> = collect_session_jsonl(&projects_dir);

    // 2. cache を読む
    let mut cache = read_scan_cache();
    let cached_len = cache.len();

    // 3. hit / miss 振り分け (hit は cache から move)
    let mut hits: Vec<SessionInfo> = Vec::new();
    let mut misses: Vec<PathBuf> = Vec::new();
    for (path, mt) in &current {
        match cache.remove(path) {
            Some((cached_mt, info)) if cached_mt == *mt => hits.push(info),
            _ => misses.push(path.clone()),
        }
    }
    let current_map: HashMap<PathBuf, u128> = current.into_iter().collect();

    // 4. miss のみ parse
    let mut no_last_ts: usize = 0;
    let mut unparsable: usize = 0;
    let parsed: Vec<Result<SessionInfo, bool>> =
        misses.par_iter().map(|p| parse_session(p)).collect();
    for r in parsed {
        match r {
            Ok(s) => hits.push(s),
            Err(true) => no_last_ts += 1,
            Err(false) => unparsable += 1,
        }
    }

    let mut sessions = hits;
    sessions.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    // 5. dirty なら書き戻し（新規 parse があった、または cache のエントリ数が現状と違う）
    let dirty = !misses.is_empty() || cached_len != current_map.len();
    if dirty {
        write_scan_cache(&sessions, &current_map);
    }

    ScanResult { sessions, no_last_ts, unparsable }
}

/// (path, mtime_ns) of every `<projects_dir>/<dir>/*.jsonl`, in listing order. Files
/// whose mtime cannot be read are left out.
pub(crate) fn collect_session_jsonl(projects_dir: &Path) -> Vec<(PathBuf, u128)> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(projects_dir) else {
        return out;
    };
    for e in entries.flatten() {
        if !entry_is_dir(&e) {
            continue;
        }
        let Ok(files) = fs::read_dir(e.path()) else {
            continue;
        };
        for f in files.flatten() {
            let fp = f.path();
            if fp.extension().is_some_and(|ex| ex == "jsonl")
                && !fp.to_string_lossy().contains("subagents")
            {
                if let Some(mt) = entry_mtime_ns(&f) {
                    out.push((fp, mt));
                }
            }
        }
    }
    out
}

/// Whether a directory entry is a directory, following links like `Path::is_dir`.
/// On Windows the type comes from the listing itself; only links cost a stat.
fn entry_is_dir(e: &fs::DirEntry) -> bool {
    match e.file_type() {
        Ok(t) if !t.is_symlink() => t.is_dir(),
        _ => e.path().is_dir(),
    }
}

/// mtime (ns since the epoch) of the file an entry names, following links like
/// `fs::metadata`. On Windows `DirEntry::metadata` comes from the listing (no open), but
/// describes a link itself, so links fall back to `fs::metadata`.
fn entry_mtime_ns(e: &fs::DirEntry) -> Option<u128> {
    let meta = match e.file_type() {
        Ok(t) if !t.is_symlink() => e.metadata().ok()?,
        _ => fs::metadata(e.path()).ok()?,
    };
    meta.modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_nanos())
}

/// セッションファイルは大きい場合 80MB に達するので、先頭/末尾だけ部分読みする。
/// HEAD: session_id, cwd 抽出用。TAIL: title/timestamp/最新メッセージ抽出用。
/// BASE は高速パス、MAX は fallback (file-history-snapshot が大量に詰まっているなど、
/// BASE で session_id/cwd/timestamp が取れなかった時に一度だけ拡張して再試行する)。
const HEAD_BASE_BYTES: u64 = 128 * 1024;
const TAIL_BASE_BYTES: u64 = 512 * 1024;
const HEAD_MAX_BYTES: u64 = 4 * 1024 * 1024;
const TAIL_MAX_BYTES: u64 = 4 * 1024 * 1024;

/// 部分読みで (head, tail) のバイト列を返す。ファイルが (head + tail) 以下なら全文を head に入れて tail は空。
/// 部分境界の改行で切る（中途半端なバイトを捨てる）。
fn read_head_and_tail(path: &Path, head_bytes: u64, tail_bytes: u64) -> Result<(Vec<u8>, Vec<u8>), bool> {
    let mut f = File::open(path).map_err(|_| false)?;
    let len = f.metadata().map_err(|_| false)?.len();

    if len <= head_bytes + tail_bytes {
        let mut data = Vec::with_capacity(len as usize);
        f.read_to_end(&mut data).map_err(|_| false)?;
        return Ok((data, Vec::new()));
    }

    let mut head = vec![0u8; head_bytes as usize];
    f.read_exact(&mut head).map_err(|_| false)?;
    let last_nl = head.iter().rposition(|&b| b == b'\n').unwrap_or(0);
    head.truncate(last_nl);

    let tail_off = len - tail_bytes;
    f.seek(SeekFrom::Start(tail_off)).map_err(|_| false)?;
    let mut tail = vec![0u8; tail_bytes as usize];
    f.read_exact(&mut tail).map_err(|_| false)?;
    let first_nl = tail.iter().position(|&b| b == b'\n').unwrap_or(tail.len());
    let start = (first_nl + 1).min(tail.len());
    tail.drain(..start);

    Ok((head, tail))
}

/// Returns Ok(session) on success, Err(true) if only last_timestamp was missing,
/// Err(false) for other parse failures.
///
/// BASE ウィンドウでまず試し、cwd/session_id/timestamp のいずれかが取れなかったら
/// 一度だけ MAX ウィンドウで再試行する。BASE で通る分はキャッシュされるので、
/// MAX の I/O は「メタレコードが大量に詰まった大型セッション」等のエッジのみ発生する。
fn parse_session(path: &Path) -> Result<SessionInfo, bool> {
    match parse_session_window(path, HEAD_BASE_BYTES, TAIL_BASE_BYTES) {
        Ok(s) => Ok(s),
        Err(_) => parse_session_window(path, HEAD_MAX_BYTES, TAIL_MAX_BYTES),
    }
}

fn parse_session_window(path: &Path, head_bytes: u64, tail_bytes: u64) -> Result<SessionInfo, bool> {
    let (mut head_buf, tail_buf) = read_head_and_tail(path, head_bytes, tail_bytes)?;
    // parse_line overwrites each line it parses. A small file comes back whole in head
    // with an empty tail, and both loops below walk it, so the reverse scan gets a copy.
    let mut tail_buf = if tail_buf.is_empty() { head_buf.clone() } else { tail_buf };
    let mut head_lines: Vec<&mut [u8]> =
        head_buf.split_mut(|&b| b == b'\n').filter(|s| !s.is_empty()).collect();
    if head_lines.is_empty() {
        return Err(false);
    }

    let mut session_id: Option<String> = None;
    let mut cwd: Option<String> = None;
    let mut ai_title: Option<String> = None;

    // cwd はセッションを通じて不変なので、type を問わず最初に top-level cwd を
    // 持つ行から採る。type=user 限定にすると file-history-snapshot が HEAD を
    // 埋め尽くす大型セッションで cwd を取り逃す。
    for line in head_lines.iter_mut() {
        let Some(meta) = parse_line(line) else { continue };
        if session_id.is_none() {
            if let Some(sid) = &meta.session_id {
                session_id = Some(sid.clone());
            }
        }
        if ai_title.is_none() && meta.typ.as_deref() == Some("ai-title") {
            ai_title = meta.ai_title.clone();
        }
        if cwd.is_none() {
            if let Some(c) = &meta.cwd {
                cwd = Some(c.clone());
            }
        }
        if cwd.is_some() && session_id.is_some() && ai_title.is_some() {
            break;
        }
    }

    let session_id = session_id.ok_or(false)?;
    let cwd = cwd.ok_or(false)?;

    // 小ファイル時は tail_buf が head の写し → head 全体を末尾走査の対象にする
    let mut lines: Vec<&mut [u8]> =
        tail_buf.split_mut(|&b| b == b'\n').filter(|s| !s.is_empty()).collect();

    let mut title: Option<String> = None;
    let mut last_timestamp: Option<String> = None;
    let mut message_parts: Vec<String> = Vec::new();
    let mut accumulated_len: usize = 0;
    const MIN_MSG_LEN: usize = 40;
    const MAX_MSG_LEN: usize = 120;
    const MSG_SEP: &str = " / ";
    // custom-title はセッション末尾近くにしか付かないので探索は末尾 N 行のみ。
    // これがないと、custom-title 無しセッション(全体の~91%)で全行 JSON parse が走る。
    const TITLE_SCAN_LINES: usize = 100;
    for (idx, line) in lines.iter_mut().rev().enumerate() {
        let Some(meta) = parse_line(line) else { continue };
        if last_timestamp.is_none() {
            last_timestamp = meta.timestamp;
        }
        if idx < TITLE_SCAN_LINES
            && title.is_none()
            && meta.typ.as_deref() == Some("custom-title")
        {
            title = meta.custom_title;
        }
        if ai_title.is_none() && meta.typ.as_deref() == Some("ai-title") {
            ai_title = meta.ai_title;
        }
        if accumulated_len < MIN_MSG_LEN
            && meta.typ.as_deref() == Some("user")
            && meta.is_meta != Some(true)
        {
            if let Some(msg) = &meta.message {
                if msg.role.as_deref() == Some("user") {
                    if let Some(text) = line_user_content(msg) {
                        if !is_noise_user_content(&text) {
                            let cleaned = strip_xml_tags(&text);
                            if !cleaned.is_empty() {
                                if !message_parts.is_empty() {
                                    accumulated_len += MSG_SEP.chars().count();
                                }
                                accumulated_len += cleaned.chars().count();
                                message_parts.push(cleaned);
                            }
                        }
                    }
                }
            }
        }
        if last_timestamp.is_some()
            && accumulated_len >= MIN_MSG_LEN
            && idx >= TITLE_SCAN_LINES
        {
            break;
        }
    }

    let latest_message = if message_parts.is_empty() {
        "(no user message)".into()
    } else {
        truncate_message(message_parts.join(MSG_SEP), MAX_MSG_LEN)
    };
    let timestamp = last_timestamp.ok_or(true)?;

    let project = project_label(&cwd);

    Ok(SessionInfo {
        session_id,
        cwd,
        timestamp,
        title,
        ai_title,
        latest_message,
        project,
        path: path.to_path_buf(),
    })
}

pub(crate) fn format_line(
    i: usize,
    s: &SessionInfo,
    proj_width: usize,
    color: &str,
) -> String {
    let title = s.title.as_deref().or(s.ai_title.as_deref());
    let msg = s.latest_message.replace('\t', " ").replace('\n', " ");
    let ts = format_timestamp(&s.timestamp);

    const TITLE_WIDTH: usize = 25;
    let display = if let Some(t) = title {
        let t = t.replace('\t', " ").replace('\n', " ");
        let t = truncate_str(&t, TITLE_WIDTH);
        let pad = TITLE_WIDTH.saturating_sub(t.chars().count());
        format!("{t}{:pad$}  {DIM}{msg}{RESET}", "")
    } else {
        msg
    };

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

/// True if the user content should be skipped from resume preview / title input.
/// Covers Claude Code internal command tags, plain-text slash commands, and raw JSON
/// tool outputs that occasionally land in the user role.
pub(crate) fn is_noise_user_content(text: &str) -> bool {
    let t = text.trim_start();
    if t.starts_with("<command-") || t.starts_with("<local-command-") {
        return true;
    }
    // Plain-text slash command (e.g. "/commit", "/lgtm") with no surrounding prose.
    if t.starts_with('/') && t.len() > 1 && !t.contains(|c: char| c.is_whitespace()) {
        return true;
    }
    // Raw JSON tool output (e.g. {"type":"teammate_terminated",...}).
    if t.starts_with("{\"") {
        return true;
    }
    false
}

/// Strip raw JSON blobs (e.g. agent-team idle/shutdown notifications) that occasionally
/// get appended to user-role messages. Drops any line whose first non-space char starts
/// `{"`, and truncates lines at a trailing ` {"` so prose followed by a JSON dump survives.
pub(crate) fn strip_json_blobs(s: &str) -> String {
    let mut parts: Vec<String> = Vec::new();
    for line in s.lines() {
        if line.trim_start().starts_with("{\"") {
            continue;
        }
        let cut = line.find(" {\"").unwrap_or(line.len());
        let kept = &line[..cut];
        if !kept.trim().is_empty() {
            parts.push(kept.to_string());
        }
    }
    parts.join("\n")
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
        lines.push(format_line(i, s, proj_width, ""));
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufRead;

    struct TempDir(PathBuf);
    impl TempDir {
        fn new(name: &str) -> Self {
            let p = env::temp_dir().join(format!(
                "dotcli-resume-test-{name}-{}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&p);
            fs::create_dir_all(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    const SID: &str = "0123abcd-0000-4000-8000-00000000beef";

    fn snapshot_line(i: usize) -> String {
        format!(
            r#"{{"type":"file-history-snapshot","messageId":"m{i}","snapshot":{{"pad":"{}"}}}}"#,
            "x".repeat(1000)
        )
    }

    fn user_line(cwd: &str, ts: &str, text: &str) -> String {
        format!(
            r#"{{"type":"user","sessionId":"{SID}","cwd":"{cwd}","timestamp":"{ts}","message":{{"role":"user","content":"{text}"}}}}"#
        )
    }

    /// `<tmp>/projects/<dir>/<SID>.jsonl` holding `lines`.
    fn write_session(projects: &Path, dir: &str, lines: &[String]) -> PathBuf {
        let d = projects.join(dir);
        fs::create_dir_all(&d).unwrap();
        let p = d.join(format!("{SID}.jsonl"));
        fs::write(&p, lines.join("\n") + "\n").unwrap();
        p
    }

    /// The pre-change lookup: first `cwd` string among the first 20 lines.
    fn first_20_lines_cwd(path: &Path) -> Option<String> {
        let f = File::open(path).ok()?;
        for line in std::io::BufReader::new(f).lines().take(20) {
            let Ok(v) = serde_json::from_str::<serde_json::Value>(&line.ok()?) else {
                continue;
            };
            if let Some(c) = v.get("cwd").and_then(|x| x.as_str()) {
                return Some(c.to_string());
            }
        }
        None
    }

    fn target() -> String {
        format!("{SID}.jsonl")
    }

    #[test]
    fn cwd_found_after_many_snapshot_lines() {
        let t = TempDir::new("snap");
        let projects = t.0.join("projects");
        // ~200KB of snapshots: past the first 20 lines and past HEAD_BASE_BYTES.
        let mut lines: Vec<String> = (0..200).map(snapshot_line).collect();
        lines.push(user_line("C:/work/big", "2026-01-01T00:00:00.000Z", "hi"));
        let path = write_session(&projects, "p", &lines);
        assert!(fs::metadata(&path).unwrap().len() > HEAD_BASE_BYTES);
        assert_eq!(first_20_lines_cwd(&path), None, "the old lookup missed it");
        assert_eq!(scan_session_cwd(&projects, &target()).as_deref(), Some("C:/work/big"));
    }

    #[test]
    fn cwd_of_a_normal_session_matches_the_old_lookup() {
        let t = TempDir::new("normal");
        let projects = t.0.join("projects");
        // A project dir without this session is skipped.
        fs::create_dir_all(projects.join("a-other")).unwrap();
        let lines = vec![
            snapshot_line(0),
            r#"{"type":"summary","cwd":null}"#.to_string(),
            user_line("C:/work/one", "2026-01-01T00:00:00.000Z", "hi"),
            user_line("C:/work/two", "2026-01-01T00:00:01.000Z", "again"),
        ];
        let path = write_session(&projects, "b", &lines);
        assert_eq!(first_20_lines_cwd(&path).as_deref(), Some("C:/work/one"));
        assert_eq!(scan_session_cwd(&projects, &target()).as_deref(), Some("C:/work/one"));
    }

    #[test]
    fn session_without_cwd_or_missing_gives_none() {
        let t = TempDir::new("none");
        let projects = t.0.join("projects");
        fs::create_dir_all(&projects).unwrap();
        assert_eq!(scan_session_cwd(&projects, &target()), None);
        write_session(&projects, "p", &[snapshot_line(0)]);
        assert_eq!(scan_session_cwd(&projects, &target()), None);
    }

    #[test]
    fn cached_cwd_is_used_only_for_an_existing_file_under_projects() {
        let t = TempDir::new("cache");
        let projects = t.0.join("projects");
        let path = write_session(&projects, "p", &[user_line("C:/from/file", "t", "x")]);
        let other_root = (t.0.join("elsewhere").join("p").join(target()), "C:/wrong".to_string());
        let cached = (path.clone(), "C:/from/cache".to_string());
        let gone = (projects.join("gone").join(target()), "C:/deleted".to_string());
        let find = |entries: &[(PathBuf, String)]| cached_session_cwd(entries, &projects, &target());
        assert_eq!(find(&[other_root.clone(), cached.clone()]).as_deref(), Some("C:/from/cache"));
        assert_eq!(find(&[other_root, gone.clone()]), None, "the cached file is gone");
        assert_eq!(find(&[gone, cached]), None, "an id in two project dirs is left to the scan");
    }

    fn check_parsed(path: &Path, latest_message: &str) {
        let Ok(s) = parse_session(path) else { panic!("parse failed") };
        assert_eq!(s.session_id, SID);
        assert_eq!(s.cwd, "C:/work/p");
        assert_eq!(s.timestamp, "2026-01-02T00:00:00.000Z");
        assert_eq!(s.title.as_deref(), Some("my title"));
        assert_eq!(s.ai_title.as_deref(), Some("ai named"));
        assert_eq!(s.latest_message, latest_message);
        assert_eq!(s.project, "p");
    }

    fn session_lines(padding: usize) -> Vec<String> {
        let mut lines = vec![
            // The escape matters: simd-json rewrites a line in place only where it has to
            // unescape, so a line parsed twice fails only when it holds one.
            user_line("C:/work/p", "2026-01-01T00:00:00.000Z", r#"first \"question\" about the parser"#),
            r#"{"type":"ai-title","aiTitle":"ai named"}"#.to_string(),
        ];
        lines.extend((0..padding).map(snapshot_line));
        lines.push(user_line("C:/work/p", "2026-01-02T00:00:00.000Z", "last question about it"));
        lines.push(r#"{"type":"custom-title","customTitle":"my title"}"#.to_string());
        lines
    }

    #[test]
    fn small_session_parses_head_and_tail_from_one_buffer() {
        let t = TempDir::new("small");
        let path = write_session(&t.0.join("projects"), "p", &session_lines(3));
        assert!(fs::metadata(&path).unwrap().len() <= HEAD_BASE_BYTES + TAIL_BASE_BYTES);
        // The reverse scan reaches the first message too: the head lines it walks again
        // were not overwritten by the head scan.
        check_parsed(&path, r#"last question about it / first "question" about the parser"#);
    }

    #[test]
    fn large_session_parses_separate_head_and_tail() {
        let t = TempDir::new("large");
        let path = write_session(&t.0.join("projects"), "p", &session_lines(800));
        assert!(fs::metadata(&path).unwrap().len() > HEAD_BASE_BYTES + TAIL_BASE_BYTES);
        check_parsed(&path, "last question about it");
    }

    /// The pre-change listing: `Path::is_dir` per project dir, `fs::metadata` per file.
    fn listed_then_stat(projects: &Path) -> Vec<(PathBuf, u128)> {
        let mut out = Vec::new();
        for e in fs::read_dir(projects).unwrap().flatten() {
            let p = e.path();
            if !p.is_dir() {
                continue;
            }
            for f in fs::read_dir(&p).unwrap().flatten() {
                let fp = f.path();
                if fp.extension().is_some_and(|ex| ex == "jsonl")
                    && !fp.to_string_lossy().contains("subagents")
                {
                    let Ok(m) = fs::metadata(&fp) else { continue };
                    let mt = m.modified().unwrap().duration_since(std::time::UNIX_EPOCH);
                    out.push((fp, mt.unwrap().as_nanos()));
                }
            }
        }
        out
    }

    #[test]
    fn listing_mtimes_match_stat() {
        let t = TempDir::new("list");
        let projects = t.0.join("projects");
        write_session(&projects, "a", &[snapshot_line(0)]);
        write_session(&projects, "b", &[snapshot_line(1)]);
        fs::write(projects.join("a").join("notes.txt"), "x").unwrap();
        fs::write(projects.join("top.jsonl"), "x").unwrap();
        #[cfg(windows)]
        {
            let ok = Command::new("cmd")
                .args(["/d", "/c", "mklink", "/J"])
                .arg(projects.join("c-junction"))
                .arg(projects.join("b"))
                .stdout(Stdio::null())
                .status()
                .unwrap()
                .success();
            assert!(ok, "mklink /J failed");
        }
        let listed = collect_session_jsonl(&projects);
        assert_eq!(listed, listed_then_stat(&projects));
        #[cfg(windows)]
        assert_eq!(listed.len(), 3, "the junctioned dir is listed too");
    }

    /// Read-only comparison against the real `~/.claude/projects` and scan cache:
    /// `cargo test --release -- --ignored real_sessions --nocapture`.
    #[test]
    #[ignore]
    fn real_sessions_match_the_old_lookups() {
        let projects = dirs::home_dir().unwrap().join(".claude").join("projects");
        let before = listed_then_stat(&projects);
        let listed = collect_session_jsonl(&projects);
        let after = listed_then_stat(&projects);
        let paths = |v: &[(PathBuf, u128)]| v.iter().map(|(p, _)| p.clone()).collect::<Vec<_>>();
        assert_eq!(paths(&listed), paths(&before));
        for (i, (p, mt)) in listed.iter().enumerate() {
            // A session being written may move between the listings.
            assert!(*mt == before[i].1 || *mt == after[i].1, "{}", p.display());
        }

        // Per file: the head lookup and the cache entry agree with the old 20-line lookup.
        let cache = read_scan_cache_cwds();
        let (mut same, mut found_only_now, mut cache_same) = (0, 0, 0);
        for (path, _) in &listed {
            let old = first_20_lines_cwd(path);
            let new = head_cwd(path);
            match &old {
                Some(o) => {
                    assert_eq!(new.as_ref(), Some(o), "{}", path.display());
                    same += 1;
                }
                None if new.is_some() => found_only_now += 1,
                None => {}
            }
            if let (Some(o), Some((_, c))) = (&old, cache.iter().find(|(p, _)| p == path)) {
                assert_eq!(c, o, "cache: {}", path.display());
                cache_same += 1;
            }
        }

        // Per session id: the whole lookup (cache, then scan) matches the old scan, which
        // took the first project dir in listing order holding the file.
        let old_find = |name: &str| -> Option<String> {
            for e in fs::read_dir(&projects).ok()?.flatten() {
                let p = e.path();
                if p.is_dir() && p.join(name).is_file() {
                    return first_20_lines_cwd(&p.join(name));
                }
            }
            None
        };
        let mut names: Vec<String> = listed
            .iter()
            .map(|(p, _)| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names.dedup();
        let (mut ids_same, mut ids_from_cache) = (0, 0);
        for name in &names {
            let cached = cached_session_cwd(&cache, &projects, name);
            if cached.is_some() {
                ids_from_cache += 1;
            }
            let new = cached.or_else(|| scan_session_cwd(&projects, name));
            if let Some(o) = old_find(name) {
                assert_eq!(new.as_ref(), Some(&o), "{name}");
                ids_same += 1;
            }
        }
        eprintln!(
            "files={} same_cwd={same} found_only_now={found_only_now} cache_same={cache_same} \
             cache_entries={} ids={} ids_same={ids_same} ids_from_cache={ids_from_cache}",
            listed.len(),
            cache.len(),
            names.len()
        );
    }
}

