use std::path::Path;
use std::process::Command;

pub fn run(repo_path: &str) {
    match run_inner(repo_path) {
        Ok(output) => {
            if !output.is_empty() {
                println!("{}", output);
            }
        }
        Err(_) => {
            print!("error");
        }
    }
}

fn git(repo_path: &str, args: &[&str]) -> Result<std::process::Output, ()> {
    Command::new("git")
        .args(args)
        .current_dir(repo_path)
        .output()
        .map_err(|_| ())
}

fn git_success(repo_path: &str, args: &[&str]) -> bool {
    git(repo_path, args)
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn git_output(repo_path: &str, args: &[&str]) -> Result<String, ()> {
    let out = git(repo_path, args)?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(())
    }
}

fn is_dirty_gix(repo_path: &str) -> Result<bool, ()> {
    let repo = gix::open(Path::new(repo_path)).map_err(|_| ())?;
    repo.is_dirty().map_err(|_| ())
}

/// Parses `git rev-list --left-right --count HEAD...@{u}` output (`<ahead>\t<behind>`):
/// commits only on HEAD, then commits only on the upstream.
fn parse_ahead_behind(s: &str) -> Option<(u64, u64)> {
    let mut it = s.split_whitespace();
    let ahead = it.next()?.parse().ok()?;
    let behind = it.next()?.parse().ok()?;
    if it.next().is_some() {
        return None;
    }
    Some((ahead, behind))
}

/// Token for a dirty tree that differs from its upstream. The shells (pwsh/profile.ps1,
/// nushell/config.nu) still accept the bare `dirty` that older dotcli builds print.
fn dirty_token(ahead: u64, behind: u64) -> String {
    format!("dirty:{ahead}:{behind}")
}

/// Machine token for the shells' prompt hooks: `updated:<n>:<areas>` after a pull that
/// moved HEAD, `dirty:<ahead>:<behind>` when the tree is dirty and differs from the
/// upstream (no pull), `error` (printed by `run`) when git fails, or empty for nothing
/// to report (offline, no upstream, up to date, or only ahead of the upstream).
fn run_inner(repo_path: &str) -> Result<String, ()> {
    // fetch; skip if offline
    let fetch_ok = git_success(repo_path, &["fetch", "-q"]);
    if !fetch_ok {
        return Ok(String::new());
    }

    // compare HEAD vs upstream: if no diff, nothing to do. Fails without an upstream.
    let counts = match git_output(repo_path, &["rev-list", "--left-right", "--count", "HEAD...@{u}"]) {
        Ok(s) => s,
        Err(_) => return Ok(String::new()), // no upstream configured
    };
    let Some((ahead, behind)) = parse_ahead_behind(&counts) else {
        return Ok(String::new());
    };
    if ahead == 0 && behind == 0 {
        return Ok(String::new());
    }

    // check dirty via gix; fallback to git if gix fails
    let dirty = is_dirty_gix(repo_path).unwrap_or_else(|_| {
        git(repo_path, &["status", "--porcelain"])
            .map(|o| !o.stdout.is_empty())
            .unwrap_or(false)
    });

    if dirty {
        return Ok(dirty_token(ahead, behind));
    }

    // Only ahead: `pull -r` would have nothing to bring in, so skip it.
    if behind == 0 {
        return Ok(String::new());
    }

    let before = git_output(repo_path, &["rev-parse", "HEAD"])?;
    git_success(repo_path, &["pull", "-q", "-r"])
        .then_some(())
        .ok_or(())?;

    let after = git_output(repo_path, &["rev-parse", "HEAD"])?;
    if before == after {
        return Ok(String::new());
    }

    let n = git_output(repo_path, &["rev-list", "--count", &format!("{}..{}", before, after)])?;

    let diff_out = git_output(repo_path, &["diff", "--name-only", &before, &after])?;
    let areas: Vec<String> = {
        let mut v: Vec<String> = diff_out
            .lines()
            .map(|l| l.split('/').next().unwrap_or(l).to_string())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        v.sort();
        v
    };
    let areas_str = areas.join(", ");

    Ok(format!("updated:{}:{}", n, areas_str))
}

#[cfg(test)]
mod tests {
    use super::{dirty_token, parse_ahead_behind, run_inner};
    use std::path::{Path, PathBuf};
    use std::process::Command;

    struct TempDir(PathBuf);
    impl TempDir {
        fn new(name: &str) -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let p = std::env::temp_dir().join(format!(
                "dotcli-git-prompt-{name}-{}-{nanos}",
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

    /// Runs git for fixture setup, independent of the user's identity and signing config.
    fn git(dir: &Path, args: &[&str]) {
        let out = Command::new("git")
            .args([
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@example.com",
                "-c",
                "commit.gpgsign=false",
                "-c",
                "core.hooksPath=",
                "-c",
                "init.defaultBranch=main",
            ])
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn commit(dir: &Path, file: &str, content: &str) {
        std::fs::write(dir.join(file), content).unwrap();
        git(dir, &["add", file]);
        git(dir, &["commit", "-q", "-m", file]);
    }

    /// A bare upstream with one commit and two clones of it tracking `main`.
    fn fixture(t: &TempDir) -> (PathBuf, PathBuf) {
        let seed = t.0.join("seed");
        std::fs::create_dir_all(&seed).unwrap();
        git(&seed, &["init", "-q"]);
        commit(&seed, "a.txt", "a\n");
        git(&t.0, &["clone", "-q", "--bare", "seed", "up.git"]);
        git(&t.0, &["clone", "-q", "up.git", "one"]);
        git(&t.0, &["clone", "-q", "up.git", "two"]);
        (t.0.join("one"), t.0.join("two"))
    }

    fn head(dir: &Path) -> String {
        let out = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(dir)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    #[test]
    fn ahead_behind_parsing() {
        assert_eq!(parse_ahead_behind("2\t5\n"), Some((2, 5)));
        assert_eq!(parse_ahead_behind("0\t0"), Some((0, 0)));
        assert_eq!(parse_ahead_behind("3 1"), Some((3, 1)));
        assert_eq!(parse_ahead_behind(""), None);
        assert_eq!(parse_ahead_behind("1"), None);
        assert_eq!(parse_ahead_behind("1\tx"), None);
        assert_eq!(parse_ahead_behind("-1\t2"), None);
        assert_eq!(parse_ahead_behind("1\t2\t3"), None);
    }

    #[test]
    fn dirty_token_format() {
        assert_eq!(dirty_token(0, 3), "dirty:0:3");
        assert_eq!(dirty_token(2, 0), "dirty:2:0");
        assert_eq!(dirty_token(1, 4), "dirty:1:4");
    }

    #[test]
    fn only_ahead_and_clean_skips_pull_and_reports_nothing() {
        let t = TempDir::new("ahead");
        let (one, _) = fixture(&t);
        commit(&one, "b.txt", "b\n");
        let before = head(&one);
        assert_eq!(run_inner(one.to_str().unwrap()), Ok(String::new()));
        assert_eq!(head(&one), before);
    }

    #[test]
    fn dirty_and_diverged_reports_both_counts() {
        let t = TempDir::new("diverged");
        let (one, two) = fixture(&t);
        commit(&two, "c.txt", "c\n");
        git(&two, &["push", "-q", "origin", "main"]);
        commit(&one, "b.txt", "b\n");
        // A tracked file edited in place, so the result does not depend on whether
        // untracked files count as dirty.
        std::fs::write(one.join("a.txt"), "changed\n").unwrap();
        let before = head(&one);
        assert_eq!(run_inner(one.to_str().unwrap()), Ok("dirty:1:1".to_string()));
        assert_eq!(head(&one), before);
    }

    #[test]
    fn no_upstream_reports_nothing() {
        let t = TempDir::new("noup");
        let dir = t.0.join("solo");
        std::fs::create_dir_all(&dir).unwrap();
        git(&dir, &["init", "-q"]);
        commit(&dir, "a.txt", "a\n");
        // `fetch` with no remote succeeds and does nothing; `@{u}` then fails.
        assert_eq!(run_inner(dir.to_str().unwrap()), Ok(String::new()));
    }
}
