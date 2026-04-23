use std::path::Path;
use std::process::Command;

use gix::hash::Kind;

pub fn compute_blob_hash(content: &[u8]) -> String {
    let header = format!("blob {}\0", content.len());
    let mut hasher = gix::hash::hasher(Kind::Sha1);
    hasher.update(header.as_bytes());
    hasher.update(content);
    // collision detection error is extremely unlikely; treat as infallible
    let oid = hasher.try_finalize().expect("sha1 collision detected");
    oid.to_hex().to_string()
}

/// Returns the blob hash of `file_path` at HEAD using `git rev-parse HEAD:<relpath>`.
pub fn head_blob_hash_from_git(file_path: &Path) -> Option<String> {
    let parent = file_path.parent()?;
    let output = Command::new("git")
        .args(["-C", parent.to_str()?, "rev-parse", &format!("HEAD:{}", rel_path_from_git_root(file_path)?)])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let hash = String::from_utf8(output.stdout).ok()?.trim().to_string();
    if hash.len() == 40 {
        Some(hash)
    } else {
        None
    }
}

/// Returns the path of `file_path` relative to the git root.
fn rel_path_from_git_root(file_path: &Path) -> Option<String> {
    let parent = file_path.parent()?;
    let output = Command::new("git")
        .args(["-C", parent.to_str()?, "rev-parse", "--show-toplevel"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let root_raw = String::from_utf8(output.stdout).ok()?;
    let root = normalize_path(root_raw.trim());
    let file_norm = normalize_path(file_path.to_str()?);
    let root_prefix = if root.ends_with('/') { root.clone() } else { format!("{root}/") };
    if file_norm.starts_with(&root_prefix) {
        Some(file_norm[root_prefix.len()..].to_string())
    } else {
        None
    }
}

pub fn normalize_path(p: &str) -> String {
    let s = p.replace('\\', "/");
    if s.len() >= 3 {
        let b = s.as_bytes();
        if b[0] == b'/' && b[1].is_ascii_alphabetic() && b[2] == b'/' {
            let drive = (b[1] as char).to_ascii_uppercase();
            return format!("{drive}:/{}", &s[3..]);
        }
    }
    if s.len() >= 2 {
        let b = s.as_bytes();
        if b[0].is_ascii_lowercase() && b[1] == b':' {
            let drive = (b[0] as char).to_ascii_uppercase();
            return format!("{drive}:{}", &s[2..]);
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_blob_hash_empty() {
        assert_eq!(compute_blob_hash(b""), "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391");
    }

    #[test]
    fn test_compute_blob_hash_hello() {
        assert_eq!(compute_blob_hash(b"hello"), "b6fc4c620b67d95f953a5c1c1230aaab5db5a1b0");
    }

    #[test]
    fn test_normalize_windows_backslash() {
        assert_eq!(normalize_path("C:\\foo\\bar"), "C:/foo/bar");
    }

    #[test]
    fn test_normalize_msys() {
        assert_eq!(normalize_path("/c/foo/bar"), "C:/foo/bar");
    }

    #[test]
    fn test_normalize_lowercase_drive() {
        assert_eq!(normalize_path("c:/foo/bar"), "C:/foo/bar");
    }

    #[test]
    fn test_normalize_unix_passthrough() {
        assert_eq!(normalize_path("/home/user/file"), "/home/user/file");
    }
}
