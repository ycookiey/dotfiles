use super::claude_launch::{SAFE_ARG, SKIP_PERMISSIONS};
use super::resume;
use crate::protocol::{CLAUDE_PROVIDER_ENV, ExecCommand, Message, MessageLevel, ShellAction};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
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

    // Determine account number; `recommended` is read only when auto-detecting,
    // the only case that uses it (account pick and the "(recommended)" label).
    let mut recommended: Option<Recommended> = None;
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
        recommended = read_recommended(&d);
        let picked = pick_account(recommended.as_ref(), &last_file);
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
            let is_recommended = recommended
                .as_ref()
                .is_some_and(|r| r.fresh && r.account == *n);
            let text = if is_recommended {
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

    // s → safe mode (no --dangerously-skip-permissions)
    let safe_mode = claude_args.first().map(|s| s.as_str()) == Some(SAFE_ARG);
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
            resume::select(&query, skip_perms)
        };
        action.set_env.extend(env);
        action.unset_env = unset_env;
        action.messages.extend(messages);
        let code = action.exit_code;
        action.print();
        std::process::exit(code);
    }

    // Default: add --dangerously-skip-permissions unless safe mode
    if !safe_mode {
        claude_args.insert(0, SKIP_PERMISSIONS.into());
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

/// How long a `.recommended` entry stays valid, in seconds.
const RECOMMENDED_TTL_SECS: u64 = 300;

/// A parsed `<claude_dir>/.recommended` entry (`<account>\t<unix secs>`).
struct Recommended {
    account: String,
    /// Written within `RECOMMENDED_TTL_SECS` (or with a timestamp in the future).
    fresh: bool,
}

fn read_recommended(d: &Path) -> Option<Recommended> {
    let content = fs::read_to_string(d.join(".recommended")).ok()?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    parse_recommended(&content, now)
}

fn parse_recommended(content: &str, now: u64) -> Option<Recommended> {
    let mut parts = content.split('\t');
    let account = parts.next()?.trim().to_string();
    let ts = parts.next()?.trim().parse::<u64>().ok()?;
    Some(Recommended {
        account,
        fresh: now.saturating_sub(ts) < RECOMMENDED_TTL_SECS,
    })
}

fn pick_account(recommended: Option<&Recommended>, last_file: &PathBuf) -> Option<String> {
    // Recommended account (within 5 minutes)
    if let Some(r) = recommended.filter(|r| r.fresh) {
        return Some(r.account.clone());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recommended_entry_parsing_and_freshness() {
        let r = parse_recommended("2\t1000\n", 1299).unwrap();
        assert_eq!(r.account, "2");
        assert!(r.fresh);
        assert!(!parse_recommended("2\t1000", 1300).unwrap().fresh);
        // A timestamp ahead of the clock counts as fresh instead of wrapping around.
        assert!(parse_recommended("3\t2000", 1000).unwrap().fresh);
        assert!(parse_recommended("2", 1000).is_none());
        assert!(parse_recommended("2\tnope", 1000).is_none());
    }

    #[test]
    fn pick_prefers_fresh_recommended_then_last_account() {
        let dir = std::env::temp_dir().join(format!("dotcli-c-pick-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let last = dir.join(".last_account");
        fs::write(&last, "5\n").unwrap();
        let fresh = Recommended {
            account: "2".into(),
            fresh: true,
        };
        let stale = Recommended {
            account: "2".into(),
            fresh: false,
        };
        assert_eq!(pick_account(Some(&fresh), &last).as_deref(), Some("2"));
        assert_eq!(pick_account(Some(&stale), &last).as_deref(), Some("5"));
        assert_eq!(pick_account(None, &last).as_deref(), Some("5"));
        fs::remove_dir_all(&dir).unwrap();
    }
}
