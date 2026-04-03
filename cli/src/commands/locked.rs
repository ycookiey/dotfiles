use std::path::Path;
use std::process::Command;

pub fn run(path: &str) {
    let resolved = if path == "." {
        std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| ".".into())
    } else {
        Path::new(path)
            .canonicalize()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| path.into())
    };

    // Trim trailing backslash
    let resolved = resolved.trim_end_matches('\\');

    let _ = Command::new("sudo").args(["handle", resolved]).status();
}
