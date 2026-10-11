use super::claude_launch::claude_args as launch_args;
use crate::protocol::{CLAUDE_PROVIDER_ENV, ExecCommand, Message, MessageLevel, ShellAction};
use std::collections::HashMap;
use std::path::Path;

pub fn run(args: &[String]) {
    let key_file = dirs::home_dir()
        .expect("home dir")
        .join(".claude")
        .join(".glm-api-key");
    action(&key_file, args).print_and_exit();
}

/// The action for `args`, with the API key read from `key_file`.
fn action(key_file: &Path, args: &[String]) -> ShellAction {
    let api_key = match std::fs::read_to_string(key_file) {
        Ok(k) => k.trim().to_string(),
        Err(_) => {
            return ShellAction::error(format!(
                "GLM API key not found: {}\nRun install/bw-secrets.ps1 to fetch from Bitwarden",
                key_file.display()
            ));
        }
    };

    let mut env: HashMap<String, String> = HashMap::new();
    env.insert("ANTHROPIC_AUTH_TOKEN".into(), api_key);
    env.insert(
        "ANTHROPIC_BASE_URL".into(),
        "https://api.z.ai/api/anthropic".into(),
    );
    env.insert("API_TIMEOUT_MS".into(), "3000000".into());
    env.insert("ANTHROPIC_DEFAULT_OPUS_MODEL".into(), "glm-5.1".into());
    env.insert("ANTHROPIC_DEFAULT_SONNET_MODEL".into(), "glm-4.7".into());
    env.insert("ANTHROPIC_DEFAULT_HAIKU_MODEL".into(), "glm-4.5-air".into());

    let mut claude_args: Vec<String> = vec!["--model".into(), "claude-opus-4-6".into()];
    claude_args.extend(launch_args(args));

    let unset_env = CLAUDE_PROVIDER_ENV.iter().map(|s| s.to_string()).collect();

    ShellAction {
        set_env: env,
        unset_env,
        messages: vec![Message {
            text: "Claude GLM".into(),
            level: MessageLevel::Info,
        }],
        exec: Some(ExecCommand {
            program: "claude".into(),
            args: claude_args,
        }),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::action;

    #[test]
    fn missing_key_is_an_error_with_exit_code_1() {
        let missing = std::env::temp_dir().join("dotcli-cg-test-no-such-dir/.glm-api-key");
        let a = action(&missing, &[]);
        assert_eq!(a.exit_code, 1);
        assert!(a.exec.is_none() && a.set_env.is_empty());
        let json = serde_json::to_value(&a).unwrap();
        assert_eq!(json["messages"][0]["level"], "error");
        assert!(
            json["messages"][0]["text"]
                .as_str()
                .unwrap()
                .starts_with("GLM API key not found: ")
        );
    }
}
