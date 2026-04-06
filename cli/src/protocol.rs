use serde::Serialize;
use std::collections::HashMap;

/// Rust サブコマンドが stdout に返す JSON。
/// シェル側の `_dotcli_apply` がパースして shell state に適用する。
#[derive(Serialize, Default)]
pub struct ShellAction {
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub set_env: HashMap<String, String>,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub unset_env: Vec<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub cd: Option<String>,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub messages: Vec<Message>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub exec: Option<ExecCommand>,

    #[serde(skip_serializing_if = "is_zero")]
    pub exit_code: i32,
}

#[derive(Serialize)]
pub struct ExecCommand {
    pub program: String,
    pub args: Vec<String>,
}

#[derive(Serialize)]
pub struct Message {
    pub text: String,
    pub level: MessageLevel,
}

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageLevel {
    Info,
    Warn,
    Error,
}

fn is_zero(v: &i32) -> bool {
    *v == 0
}

impl ShellAction {
    pub fn print(&self) {
        use std::io::Write;
        println!(
            "{}",
            serde_json::to_string(self).expect("Failed to serialize ShellAction")
        );
        let _ = std::io::stdout().flush();
    }
}

/// Env vars set by Claude provider commands (c, cb, cg).
/// Each command unsets all of these, then set_env overrides its own.
pub const CLAUDE_PROVIDER_ENV: &[&str] = &[
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_BASE_URL",
    "API_TIMEOUT_MS",
    "CLAUDE_CODE_USE_BEDROCK",
    "AWS_REGION",
    "CLAUDE_CODE_MAX_OUTPUT_TOKENS",
    "ANTHROPIC_MODEL",
    "ANTHROPIC_DEFAULT_OPUS_MODEL",
    "ANTHROPIC_DEFAULT_SONNET_MODEL",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL",
];
