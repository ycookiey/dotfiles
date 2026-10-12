use super::resume;
use crate::protocol::{CLAUDE_PROVIDER_ENV, ExecCommand, Message, MessageLevel, ShellAction};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

fn claude_dir() -> PathBuf {
    dirs::home_dir().expect("home dir").join(".claude")
}

pub fn run(args: &[String]) {
    run_in(args, None);
}

/// zoxide の対話選択でディレクトリを選び、そこで `c` と同じ引数解釈で起動する。
/// `r` / `rs` の再開ピッカーは選んだディレクトリのセッションに絞る。
pub fn run_zi(args: &[String]) {
    let output = Command::new("zoxide")
        .args(["query", "--interactive"])
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .output();
    let output = match output {
        Ok(o) => o,
        Err(e) => {
            ShellAction {
                messages: vec![Message {
                    text: format!("Failed to start zoxide: {e}"),
                    level: MessageLevel::Error,
                }],
                exit_code: 1,
                ..Default::default()
            }
            .print();
            return;
        }
    };
    let dir = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !output.status.success() || dir.is_empty() {
        // fzf キャンセル
        ShellAction {
            exit_code: output.status.code().unwrap_or(1),
            ..Default::default()
        }
        .print();
        return;
    }
    run_in(args, Some(dir));
}

/// `dir` 指定時は起動前にそこへ cd する（`cz` 用）。
fn run_in(args: &[String], dir: Option<String>) {
    let d = claude_dir();
    let last_file = d.join(".last_account");

    // "save" subcommand: accept both "c save 2" and "c 2 save"
    if let Some(name) = parse_save_args(args) {
        save(&name, &d);
        return;
    }

    let mut claude_args: Vec<String> = args.to_vec();

    // Determine account number
    let (account, shifted) = if args
        .first()
        .map(|s| s.chars().all(|c| c.is_ascii_digit()))
        .unwrap_or(false)
    {
        // Explicit account number
        let n = args[0].clone();
        claude_args = args[1..].to_vec();
        (Some(n), true)
    } else {
        // Auto-detect: recommended (within 5 min) or last used
        let picked = pick_account(&d, &last_file);
        (picked, false)
    };

    let mut env: HashMap<String, String> = HashMap::new();

    if let Some(ref n) = account {
        let acc_dir = dirs::home_dir()
            .expect("home dir")
            .join(format!(".claude-{n}"));
        if !acc_dir.exists() {
            ShellAction {
                messages: vec![Message {
                    text: "Not found. Run setup.ps1".into(),
                    level: MessageLevel::Error,
                }],
                exit_code: 1,
                ..Default::default()
            }
            .print();
            return;
        }
        env.insert(
            "CLAUDE_CONFIG_DIR".into(),
            acc_dir.to_string_lossy().to_string(),
        );
        let _ = fs::write(&last_file, n);
    }

    let mut messages: Vec<Message> = Vec::new();
    if let Some(ref n) = account {
        if !shifted {
            let recommended = is_recommended(&d, n);
            let text = if recommended {
                format!("Claude Acc {n} (recommended)")
            } else {
                format!("Claude Acc {n}")
            };
            messages.push(Message {
                text,
                level: MessageLevel::Info,
            });
        }
    }

    // Proxy CA cert
    let ca = PathBuf::from(r"C:\Main\Project\en-hancer-proxy\certs\ca.crt");
    if ca.exists() {
        env.insert(
            "NODE_EXTRA_CA_CERTS".into(),
            ca.to_string_lossy().to_string(),
        );
    }

    // Fullscreen renderer: Claude Code turns it off by itself after a few
    // sessions end within 10 s of starting (e.g. panes closed right after a
    // session restore), whatever the `tui` setting says. The variable keeps
    // it on and leaves such sessions uncounted. A value already set (`0` for
    // the classic renderer) wins; `/tui default` still switches one session.
    if std::env::var_os("CLAUDE_CODE_NO_FLICKER").is_none() {
        env.insert("CLAUDE_CODE_NO_FLICKER".into(), "1".into());
    }

    let unset_env: Vec<String> = CLAUDE_PROVIDER_ENV.iter().map(|s| s.to_string()).collect();

    // s → safe mode (no --dangerously-skip-permissions)
    let safe_mode = claude_args.first().map(|s| s.as_str()) == Some("s");
    if safe_mode {
        claude_args.remove(0);
    }

    // r / rs → fzf session picker + cd + claude --resume
    // r <session-id> → direct resume by UUID (skip fzf)
    if matches!(claude_args.first().map(|s| s.as_str()), Some("r" | "rs")) {
        let skip_perms = claude_args[0] != "rs";
        let query: Vec<String> = claude_args[1..].to_vec();
        let mut action = if query.len() == 1 && resume::is_session_id(&query[0]) {
            resume::resume_by_id(&query[0], skip_perms)
        } else {
            resume::select(&query, skip_perms, dir.as_deref())
        };
        action.set_env.extend(env);
        action.unset_env = unset_env;
        action.messages.extend(messages);
        let code = action.exit_code;
        action.print();
        std::process::exit(code);
    }

    // n <name> → 名前付きで新規作成。同名セッションが既にあれば作らず c r を案内
    if claude_args.first().map(|s| s.as_str()) == Some("n") {
        let Some(name) = claude_args.get(1).cloned() else {
            ShellAction {
                messages: vec![Message {
                    text: "Usage: c n <name>".into(),
                    level: MessageLevel::Error,
                }],
                exit_code: 1,
                ..Default::default()
            }
            .print();
            std::process::exit(1);
        };
        if let Some(s) = resume::find_by_title(&name) {
            ShellAction {
                messages: vec![Message {
                    text: format!(
                        "Session \"{name}\" already exists ({}, {}). Resume: c r {name}",
                        s.project, s.cwd
                    ),
                    level: MessageLevel::Error,
                }],
                exit_code: 1,
                ..Default::default()
            }
            .print();
            std::process::exit(1);
        }
        claude_args.splice(0..2, ["--name".to_string(), name]);
    }

    // Default: add --dangerously-skip-permissions unless safe mode
    if !safe_mode {
        claude_args.insert(0, "--dangerously-skip-permissions".into());
    }

    let action = ShellAction {
        set_env: env,
        unset_env,
        cd: dir,
        messages,
        exec: Some(ExecCommand {
            program: "claude".into(),
            args: claude_args,
        }),
        ..Default::default()
    };
    action.print();
}

/// Parse "save <name>" or "<name> save" from args
fn parse_save_args(args: &[String]) -> Option<String> {
    let (a, b) = (
        args.first().map(|s| s.as_str()),
        args.get(1).map(|s| s.as_str()),
    );
    match (a, b) {
        (Some("save"), Some(name)) => Some(name.to_string()),
        (Some(name), Some("save")) if name.chars().all(|c| c.is_ascii_digit()) => {
            Some(name.to_string())
        }
        _ => None,
    }
}

fn save(name: &str, d: &PathBuf) {
    let home = dirs::home_dir().expect("home dir");
    let target = home.join(format!(".claude-{name}"));
    let msg = if !target.exists() {
        let _ = fs::create_dir_all(&target);
        format!("Account {name} saved. Run setup.ps1 to link shared config")
    } else {
        format!("Account {name} saved")
    };
    let creds = d.join(".credentials.json");
    if creds.exists() {
        let _ = fs::copy(&creds, target.join(".credentials.json"));
    }
    ShellAction {
        messages: vec![Message {
            text: msg,
            level: MessageLevel::Info,
        }],
        ..Default::default()
    }
    .print();
}

fn pick_account(d: &PathBuf, last_file: &PathBuf) -> Option<String> {
    // Check recommended file (within 5 minutes)
    let rf = d.join(".recommended");
    if let Ok(content) = fs::read_to_string(&rf) {
        let parts: Vec<&str> = content.split('\t').collect();
        if parts.len() >= 2 {
            if let Ok(ts) = parts[1].trim().parse::<u64>() {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                if now - ts < 300 {
                    return Some(parts[0].trim().to_string());
                }
            }
        }
    }

    // Fallback: last used account
    if last_file.exists() {
        fs::read_to_string(last_file)
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    } else {
        None
    }
}

fn is_recommended(d: &PathBuf, n: &str) -> bool {
    let rf = d.join(".recommended");
    if let Ok(content) = fs::read_to_string(&rf) {
        let parts: Vec<&str> = content.split('\t').collect();
        if parts.len() >= 2 {
            if let Ok(ts) = parts[1].trim().parse::<u64>() {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                if now - ts < 300 && parts[0].trim() == n {
                    return true;
                }
            }
        }
    }
    false
}
