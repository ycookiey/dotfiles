use crate::protocol::{CLAUDE_PROVIDER_ENV, ExecCommand, ShellAction};
use std::collections::HashMap;

pub fn run(args: &[String]) {
    let mut env: HashMap<String, String> = HashMap::new();
    env.insert("CLAUDE_CODE_USE_BEDROCK".into(), "1".into());
    env.insert("AWS_REGION".into(), "ap-northeast-1".into());
    env.insert("CLAUDE_CODE_MAX_OUTPUT_TOKENS".into(), "4096".into());
    env.insert(
        "ANTHROPIC_MODEL".into(),
        "global.anthropic.claude-opus-4-5-20251101-v1:0".into(),
    );

    let mut claude_args: Vec<String> = Vec::new();
    let rest: Vec<String>;
    if args.first().map(|s| s.as_str()) == Some("r") {
        claude_args.push("/resume".into());
        rest = args[1..].to_vec();
    } else {
        rest = args.to_vec();
    }
    claude_args.extend(rest);

    let unset_env = CLAUDE_PROVIDER_ENV.iter().map(|s| s.to_string()).collect();

    let action = ShellAction {
        set_env: env,
        unset_env,
        exec: Some(ExecCommand {
            program: "claude".into(),
            args: claude_args,
        }),
        ..Default::default()
    };
    action.print();
}
