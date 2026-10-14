//! `dotcli detach <program> [args...]`: start a program detached from the calling shell.
//!
//! The generated nushell launchers (`_dotcli_launch`) use it instead of `cmd /c start`,
//! which made cmd.exe parse the arguments again (`& | < > ^` split or escaped them,
//! `%NAME%` expanded), and instead of `job spawn`, whose processes nushell kills when it
//! exits. The arguments go to CreateProcess quoted by Rust's std (the MSVCRT rules that
//! CommandLineToArgvW and the C runtime parse back), so the program sees them as given.
//! The child gets no console and its own process group, so closing the shell or its
//! console does not end it, and dotcli exits as soon as it has started.

use std::io;
use std::process::{Command, Stdio};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const DETACHED_PROCESS: u32 = 0x0000_0008;
#[cfg(windows)]
const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
#[cfg(windows)]
const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
#[cfg(windows)]
const ERROR_ACCESS_DENIED: i32 = 5;

/// Starts `program` with `args` and returns the exit code for dotcli (0 when started).
pub fn run(program: &str, args: &[String]) -> i32 {
    match spawn(program, args) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("dotcli detach: {program}: {e}");
            1
        }
    }
}

fn spawn(program: &str, args: &[String]) -> io::Result<()> {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        // Also leave a job object the shell may run in (a kill-on-close job would end
        // the child with it). A job without JOB_OBJECT_LIMIT_BREAKAWAY_OK refuses the
        // flag with ERROR_ACCESS_DENIED; start the child inside the job then.
        cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_BREAKAWAY_FROM_JOB);
        match cmd.spawn() {
            Ok(_) => return Ok(()),
            Err(e) if e.raw_os_error() == Some(ERROR_ACCESS_DENIED) => {
                cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
            }
            Err(e) => return Err(e),
        }
    }
    // The child is not waited for; dropping the handle leaves it running.
    cmd.spawn().map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::run;
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    struct TempDir(PathBuf);
    impl TempDir {
        fn new(name: &str) -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let p = std::env::temp_dir().join(format!(
                "dotcli-detach-{name}-{}-{nanos}",
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

    /// The detached program receives each argument unchanged, including the characters
    /// cmd.exe would re-parse. `git config --add` stores every value it is given in a file,
    /// so it serves as a program that records its arguments.
    #[test]
    fn arguments_arrive_unchanged() {
        let tmp = TempDir::new("args");
        let file = tmp.0.join("args.cfg");
        let values = [
            "https://example.com/?a=1&b=2|c<d>e^f",
            "%PATH% and %OS%",
            "with space (paren) \"quote\"",
            r"C:\dir with space\",
        ];
        let recorded = || -> Vec<String> {
            std::process::Command::new("git")
                .args(["config", "-f"])
                .arg(&file)
                .args(["--get-all", "t.v"])
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).lines().map(str::to_string).collect())
                .unwrap_or_default()
        };
        // `git config --add` takes one value per call. The detached call returns before
        // git runs, so wait for each value before starting the next to keep the order.
        for (i, v) in values.iter().enumerate() {
            let args: Vec<String> = vec![
                "config".into(),
                "-f".into(),
                file.display().to_string(),
                "--add".into(),
                "t.v".into(),
                v.to_string(),
            ];
            assert_eq!(run("git", &args), 0);
            let deadline = Instant::now() + Duration::from_secs(20);
            while recorded().len() <= i {
                assert!(Instant::now() < deadline, "git config did not record {v:?}");
                std::thread::sleep(Duration::from_millis(50));
            }
        }
        assert_eq!(recorded(), values);
    }

    #[test]
    fn missing_program_is_an_error() {
        assert_eq!(run("dotcli-detach-no-such-program-xyz", &[]), 1);
    }
}
