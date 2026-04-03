use std::io::Write;
use std::process::{Command, Stdio};

/// gh repo list + fzf → URL を返す
pub fn run(args: &[String]) -> Option<String> {
    let gh = Command::new("gh")
        .args(["repo", "list"])
        .args(args)
        .args([
            "-L",
            "1000",
            "--json",
            "nameWithOwner,description,url",
            "-q",
            ".[]|[.nameWithOwner,.description,.url]|@tsv",
        ])
        .output();

    let gh_output = match gh {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        Ok(o) => {
            eprint!("{}", String::from_utf8_lossy(&o.stderr));
            return None;
        }
        Err(e) => {
            eprintln!("Failed to run gh: {e}");
            return None;
        }
    };

    if gh_output.trim().is_empty() {
        return None;
    }

    let mut fzf = Command::new("fzf")
        .args(["-d", "\t", "--with-nth", "1,2"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("Failed to start fzf");

    if let Some(mut stdin) = fzf.stdin.take() {
        let _ = stdin.write_all(gh_output.as_bytes());
    }

    let output = fzf.wait_with_output().expect("fzf wait");
    if !output.status.success() {
        return None;
    }

    let selected = String::from_utf8_lossy(&output.stdout);
    let url = selected.trim().split('\t').last().unwrap_or("").to_string();
    if url.is_empty() { None } else { Some(url) }
}

/// grf 結果を stdout に出力
pub fn run_print(args: &[String]) {
    if let Some(url) = run(args) {
        println!("{url}");
    }
}

/// grf 結果をブラウザで開く
pub fn run_open(args: &[String]) {
    if let Some(url) = run(args) {
        #[cfg(target_os = "windows")]
        let _ = Command::new("cmd").args(["/C", "start", &url]).status();
        #[cfg(target_os = "macos")]
        let _ = Command::new("open").arg(&url).status();
    }
}

/// grf 選択をクローン
pub fn run_clone(args: &[String]) {
    if let Some(url) = run(args) {
        let _ = Command::new("gh").args(["repo", "clone", &url]).status();
    }
}
