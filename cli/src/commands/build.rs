use std::path::Path;
use std::process::Command;

struct Project {
    path: &'static str,
    post: Option<fn(&Path)>,
}

fn post_dotcli(dotfiles_dir: &Path) {
    let exe = std::env::current_exe().unwrap_or_default();
    let _ = Command::new(exe)
        .args(["generate", "-o"])
        .arg(dotfiles_dir)
        .status();
}

const PROJECTS: &[Project] = &[
    Project {
        path: "cli",
        post: Some(post_dotcli),
    },
    Project {
        path: "claude/statusline",
        post: None,
    },
];

fn dotfiles_dir() -> Option<std::path::PathBuf> {
    let compiled = env!("DOTFILES_DIR");
    let p = Path::new(compiled);
    if p.exists() {
        return Some(p.to_path_buf());
    }
    None
}

pub fn run() {
    let dotfiles = dotfiles_dir().unwrap_or_else(|| {
        eprintln!("Error: dotfiles directory not found");
        std::process::exit(1);
    });

    let cargo = which_cargo();

    // On Windows, a running exe can't be overwritten.
    // Build self (cli) last via cargo build, then copy over the locked binary.
    let self_project = PROJECTS.iter().find(|p| p.path == "cli");
    let other_projects: Vec<_> = PROJECTS.iter().filter(|p| p.path != "cli").collect();

    let mut failed = Vec::new();

    // Build non-self projects normally
    for project in &other_projects {
        if !build_project(&cargo, &dotfiles, project) {
            failed.push(project.path);
        }
    }

    // Build self: use `cargo build --release` + manual copy
    if let Some(project) = self_project {
        let project_dir = dotfiles.join(project.path);
        if project_dir.join("Cargo.toml").exists() {
            eprint!("  build {} ... ", project.path);
            let status = Command::new(&cargo)
                .args(["build", "--release"])
                .current_dir(&project_dir)
                .status();

            match status {
                Ok(s) if s.success() => {
                    // Copy built binary over the running one
                    let src = project_dir.join("target/release/dotcli.exe");
                    let dst = std::env::current_exe().unwrap();
                    // Rename current exe out of the way, then copy new one
                    let tmp = dst.with_extension("old");
                    let _ = std::fs::rename(&dst, &tmp);
                    match std::fs::copy(&src, &dst) {
                        Ok(_) => {
                            let _ = std::fs::remove_file(&tmp);
                            eprintln!("ok");
                            if let Some(post) = project.post {
                                post(&dotfiles);
                            }
                        }
                        Err(e) => {
                            // Restore old binary
                            let _ = std::fs::rename(&tmp, &dst);
                            eprintln!("FAILED (copy: {e})");
                            failed.push(project.path);
                        }
                    }
                }
                _ => {
                    eprintln!("FAILED");
                    failed.push(project.path);
                }
            }
        }
    }

    if failed.is_empty() {
        eprintln!("All projects built successfully");
    }
    if !failed.is_empty() {
        eprintln!("Failed: {}", failed.join(", "));
        std::process::exit(1);
    }
}

fn build_project(cargo: &str, dotfiles: &Path, project: &Project) -> bool {
    let project_dir = dotfiles.join(project.path);
    if !project_dir.join("Cargo.toml").exists() {
        eprintln!("  skip {} (not found)", project.path);
        return true;
    }

    eprint!("  build {} ... ", project.path);
    let status = Command::new(cargo)
        .args(["install", "--path"])
        .arg(&project_dir)
        .arg("--quiet")
        .status();

    match status {
        Ok(s) if s.success() => {
            eprintln!("ok");
            if let Some(post) = project.post {
                post(dotfiles);
            }
            true
        }
        _ => {
            eprintln!("FAILED");
            false
        }
    }
}

fn which_cargo() -> String {
    // Try cargo in PATH
    if Command::new("cargo")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return "cargo".into();
    }
    // Fallback: ~/.cargo/bin/cargo
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    let fallback = format!("{home}/.cargo/bin/cargo");
    if Path::new(&fallback).exists() {
        return fallback;
    }
    eprintln!("Error: cargo not found");
    std::process::exit(1);
}
