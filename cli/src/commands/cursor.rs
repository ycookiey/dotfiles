use crate::protocol::{ExecCommand, Message, MessageLevel, ShellAction};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn fmt_path(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

const MDC_NAME: &str = "claude-from-home.mdc";

const MDC_BODY: &str = r#"---
description: ~/.claude/CLAUDE.md をエージェントに常時適用（dotcli cursor-agent が生成）
alwaysApply: true
---

次をルールとして読み、従う。

"#;

pub fn run(force: bool, skip_mdc: bool, agent_args: &[String]) {
    let mut messages: Vec<Message> = Vec::new();

    if !skip_mdc {
        let git_root = match git_toplevel() {
            Ok(p) => p,
            Err(e) => {
                ShellAction {
                    messages: vec![Message {
                        text: format!("git rev-parse --show-toplevel に失敗: {e}"),
                        level: MessageLevel::Error,
                    }],
                    exit_code: 1,
                    ..Default::default()
                }
                .print();
                return;
            }
        };

        let home_claude = dirs::home_dir().map(|h| h.join(".claude").join("CLAUDE.md"));
        let Some(home_claude) = home_claude.filter(|p| p.is_file()) else {
            ShellAction {
                messages: vec![Message {
                    text: "$HOME\\.claude\\CLAUDE.md が無い。setup.ps1 または --skip-mdc".into(),
                    level: MessageLevel::Error,
                }],
                exit_code: 1,
                ..Default::default()
            }
            .print();
            return;
        };

        let rules_dir = git_root.join(".cursor").join("rules");
        let mdc_path = rules_dir.join(MDC_NAME);

        if let Err(e) = fs::create_dir_all(&rules_dir) {
            ShellAction {
                messages: vec![Message {
                    text: format!("{} を作成できない: {e}", fmt_path(&rules_dir)),
                    level: MessageLevel::Error,
                }],
                exit_code: 1,
                ..Default::default()
            }
            .print();
            return;
        }

        let content = format!("{}@{}\n", MDC_BODY, home_claude.to_string_lossy());

        if mdc_path.is_file() && !force {
            messages.push(Message {
                text: format!(
                    "{} は既存のためスキップ（上書き: a --force）",
                    fmt_path(&mdc_path)
                ),
                level: MessageLevel::Info,
            });
        } else if let Err(e) = fs::write(&mdc_path, content) {
            ShellAction {
                messages: vec![Message {
                    text: format!("{} に書けない: {e}", fmt_path(&mdc_path)),
                    level: MessageLevel::Error,
                }],
                exit_code: 1,
                ..Default::default()
            }
            .print();
            return;
        }
    }

    let exec_args: Vec<String> = agent_args.to_vec();

    ShellAction {
        messages,
        exec: Some(ExecCommand {
            program: "agent".into(),
            args: exec_args,
        }),
        ..Default::default()
    }
    .print();
}

fn git_toplevel() -> Result<PathBuf, String> {
    let out = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    let s = String::from_utf8_lossy(&out.stdout);
    let line = s.lines().next().unwrap_or("").trim();
    if line.is_empty() {
        return Err("empty git root".into());
    }
    Ok(PathBuf::from(line))
}
