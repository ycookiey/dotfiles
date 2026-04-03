use super::resume;
use crate::protocol::{CLAUDE_PROVIDER_ENV, ExecCommand, Message, MessageLevel, ShellAction};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn claude_dir() -> PathBuf {
    dirs::home_dir().expect("home dir").join(".claude")
}

pub fn run(args: &[String]) {
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

    let unset_env: Vec<String> = CLAUDE_PROVIDER_ENV.iter().map(|s| s.to_string()).collect();

    // r → fzf session picker + cd + claude --resume
    if claude_args.first().map(|s| s.as_str()) == Some("r") {
        let query: Vec<String> = claude_args[1..].to_vec();
        let mut action = resume::select(&query);
        action.set_env.extend(env);
        action.unset_env = unset_env;
        action.messages.extend(messages);
        action.print();
        return;
    }

    let action = ShellAction {
        set_env: env,
        unset_env,
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
