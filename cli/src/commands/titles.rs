use crate::commands::resume;
use serde_json::{json, Value};
use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

// ── Cache ──

fn cache_dir() -> PathBuf {
    dirs::home_dir()
        .expect("no home dir")
        .join(".cache")
        .join("dotcli")
        .join("session-titles")
}

fn cache_path(session_id: &str) -> PathBuf {
    cache_dir().join(format!("{session_id}.txt"))
}

pub(crate) fn read_cached_title(session_id: &str) -> Option<String> {
    fs::read_to_string(cache_path(session_id))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn write_cached_title(session_id: &str, title: &str) {
    let dir = cache_dir();
    let _ = fs::create_dir_all(&dir);
    let _ = fs::write(cache_path(session_id), title.trim());
}

pub(crate) fn allocate_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("failed to bind ephemeral port")
        .local_addr()
        .expect("no local addr")
        .port()
}

// ── LLM Server ──

struct MistralServer {
    child: std::process::Child,
    port: u16,
}

fn mistralrs_bin() -> Option<PathBuf> {
    let p = dirs::home_dir()?.join(".local").join("bin").join("mistralrs.exe");
    p.exists().then_some(p)
}

impl MistralServer {
    fn start() -> anyhow::Result<Self> {
        let bin = mistralrs_bin()
            .ok_or_else(|| anyhow::anyhow!("mistralrs not found at ~/.local/bin/mistralrs.exe"))?;
        let port = allocate_port();
        let child = Command::new(bin)
            .args([
                "serve",
                "--isq", "4",
                "-m", "Qwen/Qwen2.5-1.5B-Instruct",
                "-p", &port.to_string(),
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        Ok(Self { child, port })
    }

    fn wait_ready(&self) -> bool {
        let url = format!("http://127.0.0.1:{}/v1/models", self.port);
        for _ in 0..120 {
            std::thread::sleep(std::time::Duration::from_millis(500));
            if ureq::get(&url).call().is_ok() {
                return true;
            }
        }
        false
    }

    fn kill(&mut self) {
        let pid = self.child.id();
        let _ = Command::new("taskkill")
            .args(["/F", "/T", "/PID", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

impl Drop for MistralServer {
    fn drop(&mut self) {
        self.kill();
    }
}

// ── Lock ──

fn lock_path() -> PathBuf {
    cache_dir().join(".generating.lock")
}

fn try_acquire_lock() -> bool {
    let lp = lock_path();
    if let Ok(content) = fs::read_to_string(&lp) {
        if let Ok(pid) = content.trim().parse::<u32>() {
            if is_process_running(pid) {
                return false;
            }
        }
    }
    let _ = fs::create_dir_all(cache_dir());
    fs::write(&lp, std::process::id().to_string()).is_ok()
}

fn release_lock() {
    let _ = fs::remove_file(lock_path());
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

// ── Inference ──

fn generate_title(server: &MistralServer, input: &resume::TitleInput) -> Option<String> {
    let url = format!("http://127.0.0.1:{}/v1/chat/completions", server.port);
    let payload = json!({
        "model": "default",
        "messages": [
            {
                "role": "system",
                "content": "セッションの目的を15-25字の体言止めタイトルにする。タイトルだけ出力。"
            },
            {
                "role": "user",
                "content": "開始: Neovimの補完が遅い\nAI: nvim-cmpの設定を確認。ソース優先度の調整を提案。\n最新: luasnipを削除してnative snippetに移行\nAI: 移行完了。"
            },
            {
                "role": "assistant",
                "content": "Neovim補完の最適化と移行"
            },
            {
                "role": "user",
                "content": format!(
                    "開始: {}\nAI: {}\n最新: {}\nAI: {}",
                    input.first_message, input.first_assistant,
                    input.latest_message, input.latest_assistant,
                )
            }
        ],
        "max_tokens": 25,
        "temperature": 0.1,
    });

    let resp: Value = ureq::post(&url).send_json(&payload).ok()?.body_mut().read_json().ok()?;
    let title = resp
        .get("choices")?
        .get(0)?
        .get("message")?
        .get("content")?
        .as_str()?
        .trim()
        .to_string();
    if title.is_empty() { None } else { Some(title) }
}

// ── Build command ──

pub fn build() {
    if mistralrs_bin().is_none() {
        eprintln!("Error: mistralrs not found at ~/.local/bin/mistralrs.exe");
        std::process::exit(1);
    }

    let home = dirs::home_dir().expect("no home dir");
    let projects_dir = home.join(".claude").join("projects");
    if !projects_dir.is_dir() {
        eprintln!("No sessions directory found");
        return;
    }

    let paths = resume::collect_session_jsonl(&projects_dir);
    let needs_gen: Vec<(PathBuf, String)> = paths
        .into_iter()
        .filter_map(|p| {
            let sid = session_id_from_path(&p)?;
            if read_cached_title(&sid).is_some() {
                return None;
            }
            Some((p, sid))
        })
        .collect();

    if needs_gen.is_empty() {
        println!("All sessions have cached titles");
        return;
    }

    if !try_acquire_lock() {
        eprintln!("Generation already in progress");
        std::process::exit(1);
    }

    eprint!("Starting mistralrs server...");
    let server = match MistralServer::start() {
        Ok(s) => s,
        Err(e) => {
            release_lock();
            eprintln!("\nFailed to start server: {e}");
            std::process::exit(1);
        }
    };
    if !server.wait_ready() {
        release_lock();
        eprintln!("\nServer failed to become ready");
        std::process::exit(1);
    }
    eprintln!(" ready");

    let total = needs_gen.len();
    for (i, (path, sid)) in needs_gen.iter().enumerate() {
        let Some(input) = resume::extract_title_input(path) else {
            eprintln!("[{}/{}] skip (no input): {}", i + 1, total, sid);
            continue;
        };
        match generate_title(&server, &input) {
            Some(title) => {
                write_cached_title(sid, &title);
                eprintln!("[{}/{}] {}", i + 1, total, title);
            }
            None => {
                eprintln!("[{}/{}] failed: {}", i + 1, total, sid);
            }
        }
    }

    release_lock();
}

fn session_id_from_path(path: &Path) -> Option<String> {
    let data = fs::read_to_string(path).ok()?;
    for line in data.lines().take(10) {
        if let Ok(v) = serde_json::from_str::<Value>(line) {
            if let Some(sid) = v.get("sessionId").and_then(|x| x.as_str()) {
                return Some(sid.to_string());
            }
        }
    }
    None
}

// ── Background generation (for `c r`) ──

pub(crate) struct BgGenParams {
    pub needs_gen: Vec<(PathBuf, String)>,     // (jsonl_path, session_id)
    pub sessions: Vec<resume::SessionInfo>,
    pub fzf_port: u16,
    pub tmp_path: PathBuf,
    pub proj_width: usize,
    pub cwd_now: String,
}

pub(crate) fn background_generate(params: BgGenParams) {
    if mistralrs_bin().is_none() {
        return;
    }
    if params.needs_gen.is_empty() {
        return;
    }
    if !try_acquire_lock() {
        return;
    }

    let server = match MistralServer::start() {
        Ok(s) => s,
        Err(_) => {
            release_lock();
            return;
        }
    };
    if !server.wait_ready() {
        release_lock();
        return;
    }

    for (idx, (path, sid)) in params.needs_gen.iter().enumerate() {
        // Notify fzf that this session is now generating
        reload_fzf(&params, Some(sid));
        let Some(input) = resume::extract_title_input(path) else {
            continue;
        };
        if let Some(title) = generate_title(&server, &input) {
            write_cached_title(sid, &title);
            // Show completed title; mark next as generating if any
            let next_sid = params.needs_gen.get(idx + 1).map(|(_, s)| s.as_str());
            reload_fzf(&params, next_sid);
        }
    }

    release_lock();
}

fn reload_fzf(params: &BgGenParams, generating_sid: Option<&str>) {
    // Rebuild all lines with current cache state
    let mut titles: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for s in &params.sessions {
        if let Some(t) = read_cached_title(&s.session_id) {
            titles.insert(s.session_id.clone(), t);
        }
    }

    let mut pwd_group: Vec<(usize, &resume::SessionInfo)> = Vec::new();
    let mut rest: Vec<(usize, &resume::SessionInfo)> = Vec::new();
    for (i, s) in params.sessions.iter().enumerate() {
        if pwd_group.len() < resume::HIGHLIGHT_TOP_N
            && resume::paths_equal(&s.cwd, &params.cwd_now)
        {
            pwd_group.push((i, s));
        } else {
            rest.push((i, s));
        }
    }

    let mut lines: Vec<String> = Vec::with_capacity(params.sessions.len());
    for &(i, s) in &pwd_group {
        let generating = generating_sid == Some(s.session_id.as_str())
            && s.title.is_none()
            && !titles.contains_key(&s.session_id);
        lines.push(resume::format_line(
            i,
            s,
            params.proj_width,
            resume::GREEN,
            titles.get(&s.session_id).map(|s| s.as_str()),
            generating,
        ));
    }
    let now_epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    if !pwd_group.is_empty() && !rest.is_empty() {
        let label = resume::project_label(&params.cwd_now);
        let sep = format!(
            "{}\t{}── {} recent ({}) ──{}",
            usize::MAX,
            resume::DIM,
            pwd_group.len(),
            label,
            resume::RESET,
        );
        lines.push(sep);
    }
    resume::push_rest_with_groups(&mut lines, &rest, params.proj_width, &titles, now_epoch, generating_sid);

    // Write to tmp file and send reload to fzf
    let content = lines.join("\n");
    if fs::write(&params.tmp_path, &content).is_err() {
        return;
    }

    let tmp_str = params.tmp_path.to_string_lossy().replace('\\', "\\\\");
    let body = format!("reload(type {tmp_str})");
    let url = format!("http://127.0.0.1:{}", params.fzf_port);
    let _ = ureq::post(&url)
        .header("Content-Type", "text/plain")
        .send(body.as_bytes());
}
