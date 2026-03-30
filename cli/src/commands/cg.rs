use crate::protocol::{ExecCommand, Message, MessageLevel, ShellAction};
use std::collections::HashMap;

pub fn run(args: &[String]) {
    let key_file = dirs::home_dir()
        .expect("home dir")
        .join(".claude")
        .join(".glm-api-key");

    let api_key = match std::fs::read_to_string(&key_file) {
        Ok(k) => k.trim().to_string(),
        Err(_) => {
            ShellAction {
                messages: vec![Message {
                    text: format!(
                        "GLM API key not found: {}\nRun install/bw-secrets.ps1 to fetch from Bitwarden",
                        key_file.display()
                    ),
                    level: MessageLevel::Error,
                }],
                exit_code: 1,
                ..Default::default()
            }
            .print();
            return;
        }
    };

    let mut env: HashMap<String, String> = HashMap::new();
    env.insert("ANTHROPIC_AUTH_TOKEN".into(), api_key);
    env.insert(
        "ANTHROPIC_BASE_URL".into(),
        "https://api.z.ai/api/anthropic".into(),
    );
    env.insert("API_TIMEOUT_MS".into(), "3000000".into());

    let mut claude_args: Vec<String> = Vec::new();
    let rest: Vec<String>;
    if args.first().map(|s| s.as_str()) == Some("r") {
        claude_args.push("/resume".into());
        rest = args[1..].to_vec();
    } else {
        rest = args.to_vec();
    }
    claude_args.extend(rest);

    let action = ShellAction {
        set_env: env,
        messages: vec![Message {
            text: "Claude GLM".into(),
            level: MessageLevel::Info,
        }],
        exec: Some(ExecCommand {
            program: "claude".into(),
            args: claude_args,
        }),
        ..Default::default()
    };
    action.print();
}
