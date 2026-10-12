use crate::paths::dotfiles_root;
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

fn post_pdfview(dotfiles_dir: &Path) {
    let Some(cargo_bin) = cargo_bin_dir() else {
        return;
    };
    let dst = cargo_bin.join("pdfium.dll");
    if dst.exists() {
        return;
    }
    let src = dotfiles_dir.join("cli/target/release/pdfium.dll");
    if !src.exists() {
        eprintln!("  pdfium.dll missing; run cli\\pdfview\\scripts\\setup-pdfium.ps1 -Profile release");
        return;
    }
    match std::fs::copy(&src, &dst) {
        Ok(_) => eprintln!("  copied pdfium.dll to cargo bin"),
        Err(e) => eprintln!("  pdfium.dll copy failed: {e}"),
    }
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
    Crate {
        path: "cli/pdfview",
        bin_name: "pdfview",
        post: Some(post_pdfview),
    },
];

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
            // DirEntry::file_type/metadata reuse the data the directory listing
            // already returned (FindNextFileW on Windows), so they cost no extra
            // syscall per entry. They do not follow links, so a symlink or
            // junction falls back to the path-based calls that follow it.
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            let path = entry.path();
            let (is_dir, meta) = if file_type.is_symlink() {
                (path.is_dir(), path.metadata())
            } else {
                (file_type.is_dir(), entry.metadata())
            };
            if is_dir {
                if path.file_name().map(|n| n == "target").unwrap_or(false) {
                    continue; // skip build artifacts
                }
                walk(&path, newest);
            } else if let Ok(mtime) = meta.and_then(|m| m.modified()) {
                *newest = Some(newest.map_or(mtime, |n: SystemTime| n.max(mtime)));
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

    // Get commits after binary build time. `--since=@<epoch>` takes the exact
    // timestamp (inclusive; the `commit_ts > bin_timestamp` check below keeps
    // the comparison strict).
    // TODO: this still spawns `git log -1` and `git diff` once per listed
    // commit. Collapse it to one git call per crate: either one
    // `git log --since=@<epoch> --format=... --name-only` parsed in place, or a
    // tree-hash stamp written by `dotcli build` and compared on check. The
    // choice is pending (misses commits pulled in with an older committer
    // date vs. a stamp file to keep in sync).
    let since = format!("--since=@{bin_timestamp}");
    let log_output = match Command::new("git")
        .args([
            "log",
            &since,
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

fn missing_binary_info(crate_info: &Crate, dotfiles: &Path) -> OutdatedInfo {
    let last_commit = Command::new("git")
        .args(["log", "-1", "--format=%h %s", "--", crate_info.path])
        .current_dir(dotfiles)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    let (hash, msg) = last_commit.split_once(' ').unwrap_or((&last_commit, ""));
    let label = if hash.is_empty() {
        "(not built)".to_string()
    } else {
        format!("{hash} (not built)")
    };

    let status = Command::new("git")
        .args(["status", "--porcelain", "-z", "--", crate_info.path])
        .current_dir(dotfiles)
        .output()
        .map(|o| o.stdout)
        .unwrap_or_default();
    let prefix = format!("{}/", crate_info.path);
    let mut files: Vec<String> = parse_porcelain_z(&String::from_utf8_lossy(&status))
        .into_iter()
        .filter(|f| !f.ends_with("Cargo.lock"))
        .map(|f| f.strip_prefix(&prefix).map(str::to_string).unwrap_or(f))
        .collect();
    if files.is_empty() {
        files.push("(clean)".to_string());
    }

    OutdatedInfo {
        crate_path: crate_info.path,
        commits: vec![(label, msg.to_string())],
        changed_files: files,
    }
}

/// Paths from `git status --porcelain -z` output. Each entry is `XY <path>`;
/// a rename or copy (`R`/`C` in X or Y) is followed by an extra NUL-terminated
/// field holding the original path, which is skipped. Paths are relative to
/// the repository root and never quoted under `-z`.
fn parse_porcelain_z(out: &str) -> Vec<String> {
    let mut files = Vec::new();
    let mut fields = out.split('\0');
    while let Some(entry) = fields.next() {
        if entry.len() < 4 {
            continue;
        }
        let (xy, path) = entry.split_at(3);
        if xy.contains(['R', 'C']) {
            fields.next();
        }
        files.push(path.to_string());
    }
    files
}

pub fn check() {
    let Some(dotfiles) = dotfiles_root() else {
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
            // Binary missing: always report it, whether or not the tree is clean.
            outdated_infos.push(missing_binary_info(crate_info, &dotfiles));
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
    let dotfiles = dotfiles_root().unwrap_or_else(|| {
        eprintln!("Error: dotfiles directory not found");
        std::process::exit(1);
    });
    let cargo_bin = cargo_bin_dir().unwrap_or_else(|| {
        eprintln!("Error: cargo bin directory not found");
        std::process::exit(1);
    });

    let cargo = which_cargo();

    let mut failed = Vec::new();
    for crate_info in CRATES {
        if !build_project(&cargo, &dotfiles, &cargo_bin, crate_info) {
            failed.push(crate_info.path);
        }
    }

    if failed.is_empty() {
        eprintln!("All projects built successfully");
    } else {
        eprintln!("Failed: {}", failed.join(", "));
        std::process::exit(1);
    }
}

// On Windows, a running exe can't be overwritten (cargo install's final move
// fails with os error 5), but it can be renamed. So: build in the project's
// target dir, rename the live exe out of the way, then copy the new one in
// (install_exe). Applies to all crates: dotcli is always running during dotb (and as
// long-lived MCP servers), and Claude Code starts claude-statusline afresh each time it
// redraws the status line, so a copy may be running while it is being replaced.
fn build_project(cargo: &Path, dotfiles: &Path, cargo_bin: &Path, crate_info: &Crate) -> bool {
    let project_dir = dotfiles.join(crate_info.path);
    if !project_dir.join("Cargo.toml").exists() {
        eprintln!("  skip {} (not found)", crate_info.path);
        return true;
    }

    eprint!("  build {} ... ", crate_info.path);
    let status = Command::new(cargo)
        .args(["build", "--release", "--quiet"])
        .current_dir(&project_dir)
        .status();
    if !matches!(status, Ok(s) if s.success()) {
        eprintln!("FAILED");
        return false;
    }

    // Workspace members (e.g. cli/pdfview) emit to the workspace root's
    // target dir, so search ancestors for the built exe.
    let Some(src) = project_dir
        .ancestors()
        .map(|d| d.join(format!("target/release/{}.exe", crate_info.bin_name)))
        .find(|p| p.exists())
    else {
        eprintln!("FAILED (built exe not found)");
        return false;
    };
    match install_exe(&src, cargo_bin, crate_info.bin_name) {
        Ok(()) => {
            eprintln!("ok");
            if let Some(post) = crate_info.post {
                post(dotfiles);
            }
            true
        }
        Err(e) => {
            eprintln!("FAILED (copy: {e})");
            false
        }
    }
}

/// Copies `src` to `<dir>/<bin_name>.exe`, first renaming a live exe there out of the way.
///
/// The rename target is unique per build (`<bin_name>.exe.<nanos>-<pid>.old`): a fixed
/// name would have to replace the previous build's `.old`, which fails while a
/// long-lived process (e.g. an MCP server started before that build) still runs it, and
/// the copy onto the still-present live exe then fails too. After a successful copy every
/// `<bin_name>.*.old` that can be deleted is removed; ones still running stay until a
/// later build.
fn install_exe(src: &Path, dir: &Path, bin_name: &str) -> std::io::Result<()> {
    let dst = dir.join(format!("{bin_name}.exe"));
    let old = dir.join(unique_old_name(bin_name));
    let renamed = dst.exists() && std::fs::rename(&dst, &old).is_ok();
    match std::fs::copy(src, &dst) {
        Ok(_) => {
            remove_old_exes(dir, bin_name);
            Ok(())
        }
        Err(e) => {
            if renamed {
                // dst was moved away, so anything there now is a partial copy.
                let _ = std::fs::remove_file(&dst);
                let _ = std::fs::rename(&old, &dst);
            }
            Err(e)
        }
    }
}

fn unique_old_name(bin_name: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{bin_name}.exe.{nanos}-{}.old", std::process::id())
}

/// `<bin_name>.old` (older builds), `<bin_name>.exe.old` and `<bin_name>.exe.<id>.old`.
fn is_old_exe_name(name: &str, bin_name: &str) -> bool {
    name.strip_prefix(bin_name)
        .and_then(|rest| rest.strip_prefix('.'))
        .is_some_and(|rest| rest == "old" || rest.ends_with(".old"))
}

fn remove_old_exes(dir: &Path, bin_name: &str) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let is_file = e.file_type().is_ok_and(|t| t.is_file());
        if is_file && is_old_exe_name(&e.file_name().to_string_lossy(), bin_name) {
            // Fails while a process still runs it; a later build retries.
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// Finds cargo by looking at files only (spawning `cargo --version` goes through the
/// rustup proxy and is slow): PATH first, then `$CARGO_HOME/bin`, then `~/.cargo/bin`.
fn which_cargo() -> PathBuf {
    let exe = format!("cargo{}", std::env::consts::EXE_SUFFIX);
    let path_dirs = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .unwrap_or_default();
    let fallbacks = [
        std::env::var_os("CARGO_HOME").map(|h| PathBuf::from(h).join("bin")),
        dirs::home_dir().map(|h| h.join(".cargo").join("bin")),
    ];
    path_dirs
        .into_iter()
        .filter(|d| !d.as_os_str().is_empty())
        .chain(fallbacks.into_iter().flatten())
        .map(|d| d.join(&exe))
        .find(|p| p.is_file())
        .unwrap_or_else(|| {
            eprintln!("Error: cargo not found");
            std::process::exit(1);
        })
}

#[cfg(test)]
mod tests {
    use super::{install_exe, parse_porcelain_z};
    use std::path::{Path, PathBuf};
    use std::process::{Child, Command, Stdio};

    struct TempDir(PathBuf);
    impl TempDir {
        fn new(name: &str) -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let p = std::env::temp_dir().join(format!(
                "dotcli-build-{name}-{}-{nanos}",
                std::process::id()
            ));
            std::fs::create_dir_all(&p).unwrap();
            TempDir(p)
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Keeps a copy of ping running from `exe`, like a long-lived dotcli holding its image.
    struct Running(Child);
    impl Running {
        fn start(exe: &Path) -> Self {
            let child = Command::new(exe)
                .args(["-n", "60", "127.0.0.1"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            Running(child)
        }
    }
    impl Drop for Running {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    fn ping_exe() -> PathBuf {
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        Path::new(&root).join("System32").join("PING.EXE")
    }

    #[cfg(windows)]
    #[test]
    fn install_exe_replaces_running_exe_while_an_old_copy_also_runs() {
        let tmp = TempDir::new("install");
        let dir = &tmp.0;
        let src = dir.join("new-build.bin");
        std::fs::write(&src, b"new build").unwrap();
        for name in ["tool.exe", "tool.old"] {
            std::fs::copy(ping_exe(), dir.join(name)).unwrap();
        }
        for name in [
            "tool.exe.old",
            "tool.exe.1-2.old",
            "toolbox.old",
            "other.exe.old",
            "tool.txt",
        ] {
            std::fs::write(dir.join(name), b"x").unwrap();
        }
        let _live = Running::start(&dir.join("tool.exe"));
        let _older = Running::start(&dir.join("tool.old"));
        std::thread::sleep(std::time::Duration::from_millis(300));

        install_exe(&src, dir, "tool").unwrap();

        assert_eq!(std::fs::read(dir.join("tool.exe")).unwrap(), b"new build");
        // Leftovers of this bin that nothing runs are removed; held ones stay.
        assert!(!dir.join("tool.exe.old").exists());
        assert!(!dir.join("tool.exe.1-2.old").exists());
        assert!(dir.join("tool.old").exists());
        let renamed: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with("tool.exe.") && n.ends_with(".old"))
            .collect();
        assert_eq!(renamed.len(), 1, "the running exe was renamed once: {renamed:?}");
        // Other names are left alone.
        for name in ["toolbox.old", "other.exe.old", "tool.txt"] {
            assert!(dir.join(name).exists(), "{name} must stay");
        }
    }

    #[test]
    fn old_exe_names_match_only_this_bin() {
        use super::{is_old_exe_name, unique_old_name};
        for name in ["dotcli.old", "dotcli.exe.old", "dotcli.exe.17-4.old"] {
            assert!(is_old_exe_name(name, "dotcli"), "{name}");
        }
        assert!(is_old_exe_name(&unique_old_name("dotcli"), "dotcli"));
        for name in [
            "dotcli.exe",
            "dotcli.exe.old.1",
            "dotcliold",
            "dotcli-x.old",
            "dotclix.exe.old",
            "claude-statusline.exe.old",
            "dotcli.oldx",
        ] {
            assert!(!is_old_exe_name(name, "dotcli"), "{name}");
        }
        assert_ne!(unique_old_name("dotcli"), unique_old_name("dotcli"));
    }

    #[test]
    fn install_exe_puts_exe_in_place_when_none_exists() {
        let tmp = TempDir::new("fresh");
        let src = tmp.0.join("b.bin");
        std::fs::write(&src, b"b").unwrap();
        install_exe(&src, &tmp.0, "tool").unwrap();
        assert_eq!(std::fs::read(tmp.0.join("tool.exe")).unwrap(), b"b");
    }

    #[test]
    fn porcelain_z_takes_paths_not_status_codes() {
        let out = " M cli/src/a.rs\0?? cli/new dir/\0A  cli/b.rs\0";
        assert_eq!(
            parse_porcelain_z(out),
            ["cli/src/a.rs", "cli/new dir/", "cli/b.rs"]
        );
    }

    #[test]
    fn porcelain_z_skips_rename_source() {
        let out = "R  cli/new.rs\0cli/old.rs\0 M cli/c.rs\0";
        assert_eq!(parse_porcelain_z(out), ["cli/new.rs", "cli/c.rs"]);
    }

    #[test]
    fn porcelain_z_empty() {
        assert!(parse_porcelain_z("").is_empty());
    }
}
