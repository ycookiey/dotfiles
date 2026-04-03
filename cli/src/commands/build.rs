use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

struct Crate {
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

const CRATES: &[Crate] = &[
    Crate {
        path: "cli",
        bin_name: "dotcli",
        post: Some(post_dotcli),
    },
    Crate {
        path: "claude/statusline",
        bin_name: "claude-statusline",
        post: None,
    },
];

fn dotfiles_dir() -> Option<std::path::PathBuf> {
    // Try compile-time DOTFILES_DIR first
    let compiled = env!("DOTFILES_DIR");
    let dotfiles = Path::new(&compiled);
    if dotfiles.exists() {
        return Some(dotfiles.to_path_buf());
    }
    // Fallback: find dotfiles from current exe
    std::env::current_exe().ok().and_then(|exe| {
        exe.ancestors()
            .find(|dir| dir.join("definitions.toml").exists())
            .map(|p| p.to_path_buf())
    })
}

fn cargo_bin_dir() -> Option<PathBuf> {
    // Derive from current exe (dotcli itself is in cargo bin)
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
}

fn newest_mtime(dir: &Path) -> Option<SystemTime> {
    let mut newest: Option<SystemTime> = None;
    fn walk(dir: &Path, newest: &mut Option<SystemTime>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
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

struct OutdatedInfo {
    crate_path: &'static str,
    commits: Vec<(String, String)>, // (hash, msg)
    changed_files: Vec<String>,
}

fn get_outdated_info(
    crate_info: &Crate,
    dotfiles: &Path,
    bin_mtime: SystemTime,
) -> Option<OutdatedInfo> {
    let bin_timestamp = bin_mtime
        .duration_since(SystemTime::UNIX_EPOCH)
        .ok()?
        .as_secs();

    // Get commits after binary build time
    let after = format!("{}-01-01", 1970 + (bin_timestamp / 31536000));
    let log_output = match Command::new("git")
        .args([
            "log",
            "--after",
            &after,
            "--format=%H %s",
            "--",
            crate_info.path,
        ])
        .current_dir(&dotfiles)
        .output()
    {
        Ok(output) => String::from_utf8_lossy(&output.stdout).to_string(),
        _ => return None,
    };

    let mut commits = Vec::new();
    let mut changed_files = HashSet::new();
    let prefix = format!("{}/", crate_info.path);

    for line in log_output.lines() {
        if let Some(space_idx) = line.find(' ') {
            let hash = &line[..space_idx];
            let msg = &line[space_idx + 1..];
            // Check commit time
            let time_output = match Command::new("git")
                .args(["log", "-1", "--format=%ct", hash, "--", crate_info.path])
                .current_dir(&dotfiles)
                .output()
            {
                Ok(output) => String::from_utf8_lossy(&output.stdout).trim().to_string(),
                _ => continue,
            };
            if let Ok(commit_ts) = time_output.parse::<u64>() {
                if commit_ts > bin_timestamp {
                    commits.push((hash.to_string(), msg.to_string()));
                    // Get changed files for this commit
                    let diff_output = match Command::new("git")
                        .args([
                            "diff",
                            "--name-only",
                            &format!("{}^..{}", hash, hash),
                            "--",
                            crate_info.path,
                        ])
                        .current_dir(&dotfiles)
                        .output()
                    {
                        Ok(output) => String::from_utf8_lossy(&output.stdout).to_string(),
                        _ => continue,
                    };
                    for file in diff_output.lines() {
                        if !file.ends_with("Cargo.lock") && !file.starts_with("target/") {
                            if let Some(stripped) = file.strip_prefix(&prefix) {
                                changed_files.insert(stripped.to_string());
                            } else {
                                changed_files.insert(file.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    if commits.is_empty() || changed_files.is_empty() {
        return None;
    }

    Some(OutdatedInfo {
        crate_path: crate_info.path,
        commits,
        changed_files: changed_files.into_iter().collect(),
    })
}

pub fn check() {
    let Some(dotfiles) = dotfiles_dir() else {
        return;
    };
    let Some(cargo_bin) = cargo_bin_dir() else {
        return;
    };

    let mut outdated_infos = Vec::new();
    for crate_info in CRATES {
        let project_dir = dotfiles.join(crate_info.path);
        if !project_dir.join("Cargo.toml").exists() {
            continue;
        }
        let bin = cargo_bin.join(format!("{}.exe", crate_info.bin_name));
        let bin_mtime = bin.metadata().ok().and_then(|m| m.modified().ok());
        let src_mtime = newest_mtime(&project_dir);

        if let (Some(b), Some(s)) = (bin_mtime, src_mtime) {
            if s > b {
                if let Some(info) = get_outdated_info(crate_info, &dotfiles, b) {
                    outdated_infos.push(info);
                }
            }
        } else if bin_mtime.is_none() {
            // Binary missing
            let log_output = match Command::new("git")
                .args(["log", "-1", "--format=%h %s", "--", crate_info.path])
                .current_dir(&dotfiles)
                .output()
            {
                Ok(output) => String::from_utf8_lossy(&output.stdout).to_string(),
                _ => String::new(),
            };
            if let Some(space_idx) = log_output.find(' ') {
                let hash = &log_output[..space_idx];
                let msg = &log_output[space_idx + 1..];
                let status_output = match Command::new("git")
                    .args(["status", "--short", crate_info.path])
                    .current_dir(&dotfiles)
                    .output()
                {
                    Ok(output) => String::from_utf8_lossy(&output.stdout).to_string(),
                    _ => String::new(),
                };
                let prefix = format!("{}/", crate_info.path);
                let files: Vec<String> = status_output
                    .lines()
                    .filter(|f| !f.contains("Cargo.lock"))
                    .map(|f| {
                        f.strip_prefix(&prefix)
                            .unwrap_or(f)
                            .split_whitespace()
                            .next()
                            .unwrap_or(f)
                            .to_string()
                    })
                    .collect();
                if !files.is_empty() {
                    outdated_infos.push(OutdatedInfo {
                        crate_path: crate_info.path,
                        commits: vec![(format!("{} (not built)", hash), msg.to_string())],
                        changed_files: files,
                    });
                }
            }
        }
    }

    if !outdated_infos.is_empty() {
        eprintln!("dotfiles build outdated");
        for info in &outdated_infos {
            eprintln!("{}/", info.crate_path);
            eprintln!("└─ changed: {}", info.changed_files.join(", "));
            eprintln!();
            eprintln!("commits:");
            for (hash, msg) in &info.commits {
                eprintln!("    ✦ {} {}", hash, msg);
            }
            eprintln!();
        }
        eprintln!("run: dotb");
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
    let self_project = CRATES.iter().find(|c| c.path == "cli");
    let other_projects: Vec<_> = CRATES.iter().filter(|c| c.path != "cli").collect();

    let mut failed = Vec::new();

    // Build non-self projects normally
    for crate_info in &other_projects {
        if !build_project(&cargo, &dotfiles, crate_info) {
            failed.push(crate_info.path);
        }
    }

    // Build self: use `cargo build --release` + manual copy
    if let Some(crate_info) = self_project {
        let project_dir = dotfiles.join(crate_info.path);
        if project_dir.join("Cargo.toml").exists() {
            eprint!("  build {} ... ", crate_info.path);
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
                            if let Some(post) = crate_info.post {
                                post(&dotfiles);
                            }
                        }
                        Err(e) => {
                            // Restore old binary
                            let _ = std::fs::rename(&tmp, &dst);
                            eprintln!("FAILED (copy: {e})");
                            failed.push(crate_info.path);
                        }
                    }
                }
                _ => {
                    eprintln!("FAILED");
                    failed.push(crate_info.path);
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

fn build_project(cargo: &str, dotfiles: &Path, crate_info: &Crate) -> bool {
    let project_dir = dotfiles.join(crate_info.path);
    if !project_dir.join("Cargo.toml").exists() {
        eprintln!("  skip {} (not found)", crate_info.path);
        return true;
    }

    eprint!("  build {} ... ", crate_info.path);
    let status = Command::new(cargo)
        .args(["install", "--path"])
        .arg(&project_dir)
        .arg("--quiet")
        .status();

    match status {
        Ok(s) if s.success() => {
            eprintln!("ok");
            if let Some(post) = crate_info.post {
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
