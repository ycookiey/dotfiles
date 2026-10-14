use crate::paths::dotfiles_root;
use std::collections::BTreeSet;
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

/// `cargo install` と同じ置き先 ($CARGO_HOME/bin、未設定なら ~/.cargo/bin)。
/// 実行中の exe の場所から導くと、target/release の dotcli から dotb したとき
/// コピー元と置き先が同じファイルになるので使わない。
fn cargo_bin_dir() -> Option<PathBuf> {
    let home = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".cargo")))?;
    Some(home.join("bin")).filter(|d| d.is_dir())
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

    // Commits touching the crate after the binary was built, with their changed
    // files, from one `git log` per crate. `--since=@<epoch>` takes the exact
    // timestamp (inclusive; the `committed > bin_timestamp` check below keeps
    // the comparison strict).
    // Known limit: `--since` and the check go by committer date, so a commit
    // whose committer date is older than the binary but that arrived later
    // (pulled or rebased in without rewriting the date) is missed. When every
    // newer change to the crate is such a commit, this returns None and
    // `check` reports nothing for the crate, though its sources are newer.
    let since = format!("--since=@{bin_timestamp}");
    let log_output = match Command::new("git")
        .args(["log", &since, LOG_FORMAT, "--name-only", "--", crate_info.path])
        .current_dir(dotfiles)
        .output()
    {
        Ok(output) => String::from_utf8_lossy(&output.stdout).to_string(),
        _ => return None,
    };

    let mut commits = Vec::new();
    let mut changed_files = BTreeSet::new();
    let prefix = format!("{}/", crate_info.path);

    for commit in parse_log(&log_output) {
        if !commit.committed.is_some_and(|ts| ts > bin_timestamp) {
            continue;
        }
        // Files changed against the first parent, as `git diff <hash>^..<hash>`.
        let files = match commit.parents.len() {
            // A root commit has no `<hash>^`; it lists no files.
            0 => Vec::new(),
            1 => commit.files,
            // `log --name-only` prints no files for a merge (and its
            // `--diff-merges=first-parent` also changes which commits the
            // pathspec keeps), so merges, which are rare, get their own diff.
            _ => first_parent_diff(dotfiles, &commit.hash, crate_info.path),
        };
        commits.push((commit.hash, commit.subject));
        for file in files {
            if !file.ends_with("Cargo.lock") && !file.starts_with("target/") {
                if let Some(stripped) = file.strip_prefix(&prefix) {
                    changed_files.insert(stripped.to_string());
                } else {
                    changed_files.insert(file);
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

/// `%x00` before every field: no field holds a NUL, so splitting the whole
/// output on NUL yields, per commit, the hash, the parents, the committer time
/// and `<subject>\n\n<file>\n...` (`--name-only` adds the file lines).
const LOG_FORMAT: &str = "--format=%x00%H%x00%P%x00%ct%x00%s";

/// One commit of `git log LOG_FORMAT --name-only`.
#[derive(Debug, PartialEq)]
struct LoggedCommit {
    hash: String,
    parents: Vec<String>,
    /// Committer time; `None` when it does not parse.
    committed: Option<u64>,
    subject: String,
    files: Vec<String>,
}

fn parse_log(out: &str) -> Vec<LoggedCommit> {
    let mut fields = out.split('\0');
    fields.next(); // text before the first record (empty)
    let mut commits = Vec::new();
    while let (Some(hash), Some(parents), Some(ct), Some(rest)) =
        (fields.next(), fields.next(), fields.next(), fields.next())
    {
        let mut lines = rest.lines();
        let subject = lines.next().unwrap_or_default().to_string();
        commits.push(LoggedCommit {
            hash: hash.to_string(),
            parents: parents.split_whitespace().map(str::to_string).collect(),
            committed: ct.trim().parse().ok(),
            subject,
            files: lines.filter(|l| !l.is_empty()).map(str::to_string).collect(),
        });
    }
    commits
}

fn first_parent_diff(dotfiles: &Path, hash: &str, path: &str) -> Vec<String> {
    Command::new("git")
        .args(["diff", "--name-only", &format!("{hash}^..{hash}"), "--", path])
        .current_dir(dotfiles)
        .output()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
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
// target dir, then swap the new exe in with install_exe. Applies to all crates:
// dotcli is always running during dotb (and as long-lived MCP servers), and Claude
// Code starts claude-statusline afresh each time it redraws the status line, so a
// copy may be running while it is being replaced.
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
            eprintln!("FAILED ({e})");
            false
        }
    }
}

/// `<bin_dir>/<bin_name>.exe` を `src` で置き換える。
/// 1. 新 exe を `.exe.new` へコピー（ここが遅い部分。稼働中の exe には触れない）
/// 2. 稼働中の exe を毎回異なる名前 `<bin_name>.<ms>.old` へ退避（実行中でも rename は可能。
///    固定名だと、前回の退避先を別プロセスがまだ実行中のとき置き換え不可で失敗する）
/// 3. `.exe.new` を本来の名前へ rename（exe が無い時間を rename 1 回分に縮める）
///
/// 退避ファイルは毎回削除を試み、まだ実行中で消せないものだけ次回へ残す。
fn install_exe(src: &Path, bin_dir: &Path, bin_name: &str) -> Result<(), String> {
    let dst = bin_dir.join(format!("{bin_name}.exe"));
    if let (Ok(a), Ok(b)) = (src.canonicalize(), dst.canonicalize()) {
        if a == b {
            return Err(format!("source and destination are the same file: {}", a.display()));
        }
    }

    let staged = bin_dir.join(format!("{bin_name}.exe.new"));
    let _ = std::fs::remove_file(&staged);
    std::fs::copy(src, &staged).map_err(|e| format!("copy: {e}"))?;

    let aside = if dst.exists() {
        let ms = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let aside = bin_dir.join(format!("{bin_name}.{ms}.old"));
        if let Err(e) = std::fs::rename(&dst, &aside) {
            let _ = std::fs::remove_file(&staged);
            return Err(format!("move aside: {e}"));
        }
        Some(aside)
    } else {
        None
    };

    if let Err(e) = std::fs::rename(&staged, &dst) {
        if let Some(aside) = &aside {
            let _ = std::fs::rename(aside, &dst);
        }
        let _ = std::fs::remove_file(&staged);
        return Err(format!("swap in: {e}"));
    }

    remove_old_exes(bin_dir, bin_name);
    Ok(())
}

/// Names `install_exe` and earlier builds moved a live exe to: `<bin_name>.<ms>.old`,
/// `<bin_name>.exe.<id>.old`, and the fixed `<bin_name>.old` / `<bin_name>.exe.old`.
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
    use super::{install_exe, parse_log, parse_porcelain_z, LoggedCommit};
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
            "tool.1.old",
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
        assert!(!dir.join("tool.1.old").exists());
        assert!(!dir.join("tool.exe.new").exists());
        assert!(dir.join("tool.old").exists());
        // The live exe went to `tool.<ms>.old` and stays there while it runs.
        let renamed: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| {
                n.strip_prefix("tool.")
                    .and_then(|r| r.strip_suffix(".old"))
                    .is_some_and(|ms| !ms.is_empty() && ms.bytes().all(|b| b.is_ascii_digit()))
            })
            .collect();
        assert_eq!(renamed.len(), 1, "the running exe was renamed once: {renamed:?}");
        // Other names are left alone.
        for name in ["toolbox.old", "other.exe.old", "tool.txt"] {
            assert!(dir.join(name).exists(), "{name} must stay");
        }
    }

    #[test]
    fn old_exe_names_match_only_this_bin() {
        use super::is_old_exe_name;
        for name in [
            "dotcli.old",
            "dotcli.exe.old",
            "dotcli.exe.17-4.old",
            "dotcli.1760000000000.old",
        ] {
            assert!(is_old_exe_name(name, "dotcli"), "{name}");
        }
        for name in [
            "dotcli.exe",
            "dotcli.exe.new",
            "dotcli.exe.old.1",
            "dotcliold",
            "dotcli-x.old",
            "dotclix.exe.old",
            "claude-statusline.exe.old",
            "dotcli.oldx",
        ] {
            assert!(!is_old_exe_name(name, "dotcli"), "{name}");
        }
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
    fn install_exe_refuses_to_copy_a_file_onto_itself() {
        let tmp = TempDir::new("same");
        let dst = tmp.0.join("tool.exe");
        std::fs::write(&dst, b"live").unwrap();
        assert!(install_exe(&dst, &tmp.0, "tool").is_err());
        assert_eq!(std::fs::read(&dst).unwrap(), b"live");
        assert!(!tmp.0.join("tool.exe.new").exists());
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

    fn logged(hash: &str, parents: &[&str], ts: Option<u64>, subject: &str, files: &[&str]) -> LoggedCommit {
        LoggedCommit {
            hash: hash.to_string(),
            parents: parents.iter().map(|s| s.to_string()).collect(),
            committed: ts,
            subject: subject.to_string(),
            files: files.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn log_parsing() {
        // Shape of `git log LOG_FORMAT --name-only`: a merge prints no files, and the
        // last record's file list ends with a newline.
        let out = "\0h3\0p2 p9\01700000300\0Merge branch 'x'\n\
                   \0h2\0p1\01700000200\0fix(cli): 日本語の件名: a b\n\ncli/src/a.rs\ncli/Cargo.lock\n\
                   \0h1\0\01700000100\0\n\ncli/b.rs\n";
        assert_eq!(
            parse_log(out),
            vec![
                logged("h3", &["p2", "p9"], Some(1700000300), "Merge branch 'x'", &[]),
                logged(
                    "h2",
                    &["p1"],
                    Some(1700000200),
                    "fix(cli): 日本語の件名: a b",
                    &["cli/src/a.rs", "cli/Cargo.lock"],
                ),
                logged("h1", &[], Some(1700000100), "", &["cli/b.rs"]),
            ]
        );
    }

    #[test]
    fn log_parsing_edge_cases() {
        assert!(parse_log("").is_empty());
        // A commit without files and one whose time does not parse.
        assert_eq!(
            parse_log("\0h1\0p0\0x\0no files\n"),
            vec![logged("h1", &["p0"], None, "no files", &[])]
        );
        // A truncated record is dropped.
        assert!(parse_log("\0h1\0p0").is_empty());
    }

    /// Runs `get_outdated_info` against a temp repo: only commits strictly after the
    /// binary time count, merges list their first-parent diff, Cargo.lock is dropped,
    /// and paths are made relative to the crate.
    #[test]
    fn outdated_info_from_temp_repo() {
        let tmp = TempDir::new("outdated");
        let repo = &tmp.0;
        let git = |args: &[&str], date: &str| {
            let out = Command::new("git")
                .args([
                    "-c", "user.name=t", "-c", "user.email=t@example.com",
                    "-c", "commit.gpgsign=false", "-c", "core.hooksPath=",
                    "-c", "init.defaultBranch=main",
                ])
                .args(args)
                .env("GIT_AUTHOR_DATE", date)
                .env("GIT_COMMITTER_DATE", date)
                .current_dir(repo)
                .output()
                .unwrap();
            assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        };
        let write = |rel: &str, s: &str| {
            let p = repo.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, s).unwrap();
        };
        let commit = |msg: &str, date: &str| {
            git(&["add", "-A"], date);
            git(&["commit", "-q", "-m", msg], date);
        };
        git(&["init", "-q"], "@1000000000 +0000");
        write("cr/old.rs", "1");
        commit("old", "@1000000000 +0000");
        // Binary built at 1000000500. A commit at exactly that time does not count.
        write("cr/same.rs", "1");
        commit("same second", "@1000000500 +0000");
        write("cr/src/a.rs", "1");
        write("cr/Cargo.lock", "1");
        write("other/x.rs", "1");
        commit("new: a", "@1000000600 +0000");
        git(&["checkout", "-q", "-b", "side", "HEAD~1"], "@1000000700 +0000");
        write("cr/side.rs", "1");
        commit("side", "@1000000400 +0000");
        git(&["checkout", "-q", "main"], "@1000000700 +0000");
        git(&["merge", "-q", "--no-ff", "-m", "merge side", "side"], "@1000000800 +0000");

        let krate = super::Crate { path: "cr", bin_name: "x", post: None };
        let bin = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1000000500);
        let info = super::get_outdated_info(&krate, repo, bin).unwrap();
        let subjects: Vec<&str> = info.commits.iter().map(|(_, m)| m.as_str()).collect();
        assert_eq!(subjects, ["merge side", "new: a"]);
        assert_eq!(info.changed_files, ["side.rs", "src/a.rs"]);
    }
}
