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

fn run_inner(repo_path: &str) -> Result<String, ()> {
    // fetch; skip if offline
    let fetch_ok = git_success(repo_path, &["fetch", "-q"]);
    if !fetch_ok {
        return Ok(String::new());
    }

    // compare HEAD vs upstream: if no diff, nothing to do
    let head_sha = git_output(repo_path, &["rev-parse", "HEAD"])?;
    let upstream_sha = match git_output(repo_path, &["rev-parse", "@{u}"]) {
        Ok(s) => s,
        Err(_) => return Ok(String::new()), // no upstream configured
    };
    if head_sha == upstream_sha {
        return Ok(String::new());
    }

    // check dirty via gix; fallback to git if gix fails
    let dirty = is_dirty_gix(repo_path).unwrap_or_else(|_| {
        git(repo_path, &["status", "--porcelain"])
            .map(|o| !o.stdout.is_empty())
            .unwrap_or(false)
    });

    if dirty {
        return Ok("dirty".to_string());
    }

    let before = head_sha;
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
