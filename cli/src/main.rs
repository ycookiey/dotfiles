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
    /// Claude Bedrock mode
    ClaudeBedrock {
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
}

fn main() {
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
        Commands::ClaudeBedrock { args } => commands::cb::run(&args),
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
    }
}
