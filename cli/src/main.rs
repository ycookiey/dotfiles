mod commands;
mod definitions;
mod generate;
mod paths;
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
    /// zoxide interactive pick → cd → Claude (same args as ClaudeSwitch)
    ClaudeZi {
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
    /// fd + fzf → cd (zoxide hook records it)
    Zf {
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
    /// fd + fzf → open in nvim
    Vf {
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
    /// PostToolUse hook: log tool calls for token audit
    TokenAuditHook,
    /// Format token-audit JSON output with bar charts
    TokenAuditFormat,
    /// Token consumption audit (static/session/compare)
    TokenAudit {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Scan source files for duplicate token sequences (refactor candidates)
    DupScan(commands::dup_scan::DupScanArgs),
    /// Format dup-scan JSON output (stdin)
    DupScanFormat,
    /// Scan source files for duplicate string literals (Tailwind, SQL, prompts, etc.)
    StringDup(commands::string_dup::StringDupArgs),
    /// Format string-dup JSON output (stdin)
    StringDupFormat,
    /// Scan for duplicate array/object data literals (single-source constant candidates)
    LiteralDup(commands::literal_dup::LiteralDupArgs),
    /// Format literal-dup JSON output (stdin)
    LiteralDupFormat,
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
    /// (scoop/winget exports run at most once per 6h unless --force)
    Sync {
        /// Path to dotfiles repo (defaults to $DOTFILES_DIR)
        #[arg(long)]
        dot: Option<String>,
        /// Run the scoop/winget exports even if they ran within the throttle interval
        #[arg(long)]
        force: bool,
    },
    /// CAS journal admin
    Cas {
        #[command(subcommand)]
        action: CasAction,
    },
    /// MCP server (stdio) that captures Windows window screenshots by title
    McpScreenshot,
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
    /// Start a program detached from the calling shell, passing the arguments as given
    /// (no cmd.exe re-parsing). Used by the generated nushell launchers.
    // One trailing list with no help flag, so that -h, --help or -- right after the
    // program reach the program instead of clap.
    #[command(disable_help_flag = true)]
    Detach {
        /// Program path (or a name looked up on PATH), then its arguments
        #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },
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
    let cli = Cli::parse();

    match cli.command {
        Commands::Generate { output } => {
            let output_dir = output.or_else(paths::dotfiles_root).unwrap_or_else(|| {
                std::env::current_dir().expect("Failed to get cwd")
            });
            if let Err(e) = generate::run(&output_dir) {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        }
        Commands::Proxy { args } => commands::proxy::run(&args),
        Commands::ClaudeSwitch { args } => commands::c::run(&args),
        Commands::ClaudeZi { args } => commands::c::run_zi(&args),
        Commands::CursorAgent {
            force,
            skip_mdc,
            args,
        } => commands::cursor::run(force, skip_mdc, &args),
        Commands::ClaudeBedrock { args } => commands::cb::run(&args),
        Commands::ClaudeGlm { args } => commands::cg::run(&args),
        Commands::Zf { args } => commands::zf::run(&args),
        Commands::Vf { args } => commands::vf::run(&args),
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
        Commands::TokenAuditHook => commands::token_audit_hook::run(),
        Commands::TokenAuditFormat => commands::token_audit_format::run(),
        Commands::TokenAudit { args } => commands::token_audit::run(&args),
        Commands::DupScan(args) => commands::dup_scan::run(args),
        Commands::DupScanFormat => commands::dup_scan_format::run(),
        Commands::StringDup(args) => commands::string_dup::run(args),
        Commands::StringDupFormat => commands::string_dup_format::run(),
        Commands::LiteralDup(args) => commands::literal_dup::run(args),
        Commands::LiteralDupFormat => commands::literal_dup_format::run(),
        Commands::GitPrompt { path } => commands::git_prompt::run(&path),
        Commands::CheckDirty => commands::check_dirty::run(),
        Commands::SendKey { args } => commands::send_key::run(&args),
        Commands::McpScreenshot => commands::mcp_screenshot::run(),
        Commands::Notify {
            title,
            message,
            duration,
            offset,
        } => commands::notify::run(&title, &message, duration, offset),
        Commands::Detach { command } => {
            let (program, args) = command.split_first().expect("clap requires the program");
            std::process::exit(commands::detach::run(program, args))
        }
        Commands::CasPost { failure } => commands::cas_post::run(failure),
        Commands::Sync { dot, force } => commands::sync::run(dot, force),
        Commands::Cas { action } => match action {
            CasAction::Gc { days } => commands::cas_admin::gc(days),
            CasAction::Inspect { session, path } => commands::cas_admin::inspect(session.as_deref(), path.as_deref()),
        },
    }
}

#[cfg(test)]
mod scanner_args_tests {
    use super::{Cli, Commands};
    use clap::error::ErrorKind;
    use clap::{CommandFactory, Parser};
    use std::path::PathBuf;

    fn parse(args: &[&str]) -> Result<Commands, clap::Error> {
        Cli::try_parse_from(std::iter::once("dotcli").chain(args.iter().copied())).map(|c| c.command)
    }

    fn dup_scan(args: &[&str]) -> crate::commands::dup_scan::DupScanArgs {
        match parse(&[&["dup-scan"], args].concat()) {
            Ok(Commands::DupScan(a)) => a,
            Ok(_) => panic!("not dup-scan"),
            Err(e) => panic!("{args:?}: {e}"),
        }
    }

    fn string_dup(args: &[&str]) -> crate::commands::string_dup::StringDupArgs {
        match parse(&[&["string-dup"], args].concat()) {
            Ok(Commands::StringDup(a)) => a,
            Ok(_) => panic!("not string-dup"),
            Err(e) => panic!("{args:?}: {e}"),
        }
    }

    fn literal_dup(args: &[&str]) -> crate::commands::literal_dup::LiteralDupArgs {
        match parse(&[&["literal-dup"], args].concat()) {
            Ok(Commands::LiteralDup(a)) => a,
            Ok(_) => panic!("not literal-dup"),
            Err(e) => panic!("{args:?}: {e}"),
        }
    }

    fn error_kind(args: &[&str]) -> ErrorKind {
        match parse(args) {
            Ok(_) => panic!("{args:?} parsed"),
            Err(e) => e.kind(),
        }
    }

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn unknown_flags_and_bad_values_are_errors() {
        assert_eq!(error_kind(&["dup-scan", "--min-token", "50"]), ErrorKind::UnknownArgument);
        assert_eq!(error_kind(&["string-dup", "--top", "abc"]), ErrorKind::ValueValidation);
        assert_eq!(error_kind(&["string-dup", "--threshold", "high"]), ErrorKind::ValueValidation);
        assert_eq!(error_kind(&["string-dup", "--similarity", "cosine"]), ErrorKind::InvalidValue);
        assert_eq!(error_kind(&["literal-dup", "--min-elements", "x"]), ErrorKind::ValueValidation);
        assert_eq!(error_kind(&["literal-dup", "--gap", "3"]), ErrorKind::UnknownArgument);
        assert_eq!(error_kind(&["dup-scan", "stray"]), ErrorKind::UnknownArgument);
        assert!(parse(&["dup-scan", "--path"]).is_err());
        assert!(parse(&["dup-scan", "--top", "-1"]).is_err());
    }

    // The values the hand-written parsers used before the scanners moved to clap.
    #[test]
    fn defaults_are_unchanged() {
        let d = dup_scan(&[]);
        assert_eq!((d.min_tokens, d.min_lines, d.top, d.gap), (30, 3, 50, 0));
        let s = string_dup(&[]);
        assert_eq!((s.min_chars, s.min_words, s.min_occurrences, s.top), (30, 3, 2, 50));
        assert_eq!((s.similarity, s.threshold), (None, 0.7));
        let l = literal_dup(&[]);
        assert_eq!((l.min_occurrences, l.min_elements, l.min_chars, l.top), (3, 3, 10, 30));
        for scan in [&d.scan, &s.scan, &l.scan] {
            assert_eq!(scan.path, None);
            assert_eq!(scan.ext, None);
            assert!(scan.exclude.is_empty());
            assert!(!scan.no_gitignore);
            assert_eq!(scan.root(), std::env::current_dir().unwrap());
            assert_eq!(scan.exts(&["a", "b"]), ["a", "b"]);
        }
    }

    // The calls in skills/restructure/SKILL.md.
    #[test]
    fn restructure_skill_calls_parse() {
        assert_eq!(dup_scan(&["--top", "10"]).top, 10);
        let d = dup_scan(&["--gap", "3", "--top", "10"]);
        assert_eq!((d.gap, d.top), (3, 10));
        assert_eq!(string_dup(&["--top", "10"]).top, 10);
        let s = string_dup(&["--similarity", "jaccard", "--threshold", "0.7", "--top", "10"]);
        assert_eq!((s.similarity.as_deref(), s.threshold, s.top), (Some("jaccard"), 0.7, 10));
        assert_eq!(literal_dup(&["--top", "10"]).top, 10);
        let l = literal_dup(&["--min-occurrences", "2", "--min-elements", "3", "--top", "10"]);
        assert_eq!((l.min_occurrences, l.min_elements, l.top), (2, 3, 10));
    }

    #[test]
    fn shared_options_parse_like_the_old_parsers() {
        let d = dup_scan(&[
            "--path", "src", "--ext", "rs, ps1", "--exclude", "a/b", "--exclude", "c",
            "--no-gitignore", "--top", "5", "--top", "7",
        ]);
        assert_eq!(d.scan.root(), PathBuf::from("src"));
        assert_eq!(d.scan.exts(&["x"]), ["rs", "ps1"]);
        assert_eq!(d.scan.exclude, ["a/b", "c"]);
        assert!(d.scan.no_gitignore);
        // A repeated single-value option keeps its last value, as before.
        assert_eq!(d.top, 7);
        assert_eq!(string_dup(&["--ext", "md", "--ext", "ts"]).scan.exts(&[]), ["ts"]);
    }

    /// Everything after `detach` reaches the program, including -h, --help and --.
    #[test]
    fn detach_passes_every_argument_through() {
        for args in [
            &["app.exe", "--remote-debugging-port=9222", "--no-first-run", "x"][..],
            &["app.exe", "-h"],
            &["app.exe", "--help"],
            &["app.exe", "--", "b"],
            &["app.exe", "a", "--", "b"],
            &["app.exe", "-V"],
        ] {
            match parse(&[&["detach"], args].concat()) {
                Ok(Commands::Detach { command }) => assert_eq!(command, args),
                Ok(_) => panic!("{args:?}: parsed as another command"),
                Err(e) => panic!("{args:?}: {e}"),
            }
        }
        assert!(parse(&["detach"]).is_err(), "the program is required");
    }
}
