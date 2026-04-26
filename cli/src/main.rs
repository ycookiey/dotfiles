mod commands;
mod definitions;
mod generate;
mod protocol;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "dotcli", version, about = "Dotfiles CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate shell aliases from definitions.toml
    Generate {
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Proxy management (on/off/status/log)
    Proxy {
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
    /// Claude multi-account switcher
    ClaudeSwitch {
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
    /// Cursor CLI: ensure .cursor/rules + run `cursor agent`
    CursorAgent {
        #[arg(long)]
        force: bool,
        #[arg(long)]
        skip_mdc: bool,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Claude Bedrock mode
    ClaudeBedrock {
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
    /// Claude GLM mode
    ClaudeGlm {
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
    /// Yazi with cwd sync
    YaziCd {
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
    /// gh repo list + fzf
    Grf {
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
    /// Open grf result in browser
    Grfo {
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
    /// Clone grf result
    Grfc {
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
    /// Build all Cargo projects in dotfiles
    Build {
        /// Check if builds are outdated (exit 1 if outdated)
        #[arg(long)]
        check: bool,
    },
    /// Flutter emulator runner
    Frun,
    /// Check file locks
    Locked {
        #[arg(default_value = ".")]
        path: String,
    },
    /// AI title generation
    Titles {
        #[command(subcommand)]
        action: TitlesAction,
    },
    /// PostToolUse hook: log tool calls for token audit
    TokenAuditHook,
    /// Format token-audit JSON output with bar charts
    TokenAuditFormat,
    /// Token consumption audit (static/session/compare)
    TokenAudit {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Git prompt info: dirty/updated check for background job
    GitPrompt {
        /// Path to git repository
        path: String,
    },
    /// PreToolUse hook: warn if file has uncommitted git changes (reads JSON from stdin)
    CheckDirty,
    /// Inject a physical key chord via Win32 SendInput (e.g. ctrl+l)
    SendKey {
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
    /// PostToolUse/PostToolUseFailure hook: update CAS journal
    CasPost {
        #[arg(long)]
        failure: bool,
    },
    /// Sync dotfiles → ~/.claude*/, scoopfile, wingetfile, mcp servers
    Sync {
        /// Path to dotfiles repo (defaults to $DOTFILES_DIR)
        #[arg(long)]
        dot: Option<String>,
    },
    /// CAS journal admin
    Cas {
        #[command(subcommand)]
        action: CasAction,
    },
    /// Show a desktop notification popup (Win32)
    Notify {
        /// Notification title
        #[arg(short, long)]
        title: String,
        /// Notification message
        #[arg(short, long)]
        message: String,
        /// Display duration in milliseconds
        #[arg(short, long, default_value_t = 3000)]
        duration: u32,
        /// Vertical stack offset (0 = bottom, 1 = one above, ...)
        #[arg(short, long, default_value_t = 0)]
        offset: i32,
    },
}

#[derive(Subcommand)]
enum TitlesAction {
    /// Generate titles for all uncached sessions
    Build,
}

#[derive(Subcommand)]
enum CasAction {
    /// Delete old journal entries
    Gc {
        #[arg(long, default_value = "7")]
        days: u32,
    },
    /// Show journal entries as TSV
    Inspect {
        #[arg(long)]
        session: Option<String>,
        #[arg(long)]
        path: Option<String>,
    },
}

fn main() {
    // Set DOTFILES_DIR if not already set
    if std::env::var("DOTFILES_DIR").is_err() {
        let exe = std::env::current_exe().expect("Failed to get current exe");
        if let Some(dotfiles) = exe
            .ancestors()
            .find(|p| p.join("definitions.toml").exists())
        {
            unsafe {
                std::env::set_var("DOTFILES_DIR", dotfiles.display().to_string());
            }
        }
    }

    let cli = Cli::parse();

    match cli.command {
        Commands::Generate { output } => {
            let output_dir = output.unwrap_or_else(|| {
                let exe = std::env::current_exe().expect("Failed to get current exe path");
                exe.ancestors()
                    .find(|p| p.join("cli").join("definitions.toml").exists())
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| std::env::current_dir().expect("Failed to get cwd"))
            });
            if let Err(e) = generate::run(&output_dir) {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        }
        Commands::Proxy { args } => commands::proxy::run(&args),
        Commands::ClaudeSwitch { args } => commands::c::run(&args),
        Commands::CursorAgent {
            force,
            skip_mdc,
            args,
        } => commands::cursor::run(force, skip_mdc, &args),
        Commands::ClaudeBedrock { args } => commands::cb::run(&args),
        Commands::ClaudeGlm { args } => commands::cg::run(&args),
        Commands::YaziCd { args } => commands::y::run(&args),
        Commands::Grf { args } => commands::grf::run_print(&args),
        Commands::Grfo { args } => commands::grf::run_open(&args),
        Commands::Grfc { args } => commands::grf::run_clone(&args),
        Commands::Build { check } => {
            if check {
                commands::build::check();
            } else {
                commands::build::run();
            }
        }
        Commands::Frun => commands::frun::run(),
        Commands::Locked { path } => commands::locked::run(&path),
        Commands::Titles { action } => match action {
            TitlesAction::Build => commands::titles::build(),
        },
        Commands::TokenAuditHook => commands::token_audit_hook::run(),
        Commands::TokenAuditFormat => commands::token_audit_format::run(),
        Commands::TokenAudit { args } => commands::token_audit::run(&args),
        Commands::GitPrompt { path } => commands::git_prompt::run(&path),
        Commands::CheckDirty => commands::check_dirty::run(),
        Commands::SendKey { args } => commands::send_key::run(&args),
        Commands::Notify {
            title,
            message,
            duration,
            offset,
        } => commands::notify::run(&title, &message, duration, offset),
        Commands::CasPost { failure } => commands::cas_post::run(failure),
        Commands::Sync { dot } => commands::sync::run(dot),
        Commands::Cas { action } => match action {
            CasAction::Gc { days } => commands::cas_admin::gc(days),
            CasAction::Inspect { session, path } => commands::cas_admin::inspect(session.as_deref(), path.as_deref()),
        },
    }
}
