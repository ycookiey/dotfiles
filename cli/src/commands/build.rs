use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

struct Project {
    path: &'static str,
    bin_name: &'static str,
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
        bin_name: "dotcli",
        post: Some(post_dotcli),
    },
    Project {
        path: "claude/statusline",
        bin_name: "claude-statusline",
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

fn cargo_bin_dir() -> Option<PathBuf> {
    // Derive from current exe (dotcli itself is in cargo bin)
    std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf()))
}

fn newest_mtime(dir: &Path) -> Option<SystemTime> {
    let mut newest: Option<SystemTime> = None;
    fn walk(dir: &Path, newest: &mut Option<SystemTime>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().map(|n| n == "target").unwrap_or(false) {
                    continue; // skip build artifacts
                }
                walk(&path, newest);
            } else {
                if let Ok(meta) = path.metadata() {
                    if let Ok(mtime) = meta.modified() {
                        *newest = Some(newest.map_or(mtime, |n: SystemTime| n.max(mtime)));
                    }
                }
            }
        }
    }
    walk(dir, &mut newest);
    newest
}

pub fn check() {
    let Some(dotfiles) = dotfiles_dir() else {
        return;
    };
    let Some(cargo_bin) = cargo_bin_dir() else {
        return;
    };

    let mut outdated = Vec::new();
    for project in PROJECTS {
        let project_dir = dotfiles.join(project.path);
        if !project_dir.join("Cargo.toml").exists() {
            continue;
        }
        let bin = cargo_bin.join(format!("{}.exe", project.bin_name));
        let bin_mtime = bin
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok());
        let src_mtime = newest_mtime(&project_dir);

        match (bin_mtime, src_mtime) {
            (None, _) => outdated.push(project.bin_name), // binary missing
            (Some(b), Some(s)) if s > b => outdated.push(project.bin_name),
            _ => {}
        }
    }

    if !outdated.is_empty() {
        eprintln!("dotfiles build outdated ({}), run: dotb", outdated.join(", "));
        std::process::exit(1);
    }
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
