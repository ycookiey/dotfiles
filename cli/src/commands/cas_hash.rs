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

/// Returns the canonical git blob hash of a worktree file, applying clean filters
/// (autocrlf / .gitattributes text/eol/encoding / custom clean filters / lfs /
/// working-tree-encoding) so the result matches `git hash-object <path>` and is
/// directly comparable with `git rev-parse HEAD:<path>`.
///
/// Falls back to a raw-bytes hash when the file is outside any git repo or
/// `git hash-object` fails. Within a single repo the result is self-consistent
/// across calls.
pub fn canonical_worktree_blob_hash(file_path: &Path) -> Option<String> {
    if let Some(hash) = canonical_via_git(file_path) {
        return Some(hash);
    }
    let bytes = std::fs::read(file_path).ok()?;
    Some(compute_blob_hash(&bytes))
}

fn canonical_via_git(file_path: &Path) -> Option<String> {
    let parent = file_path.parent()?;
    // `git hash-object <path>` applies the full clean-filter pipeline derived from
    // repo config and .gitattributes — autocrlf, eol, working-tree-encoding, custom
    // clean filters (e.g. git-lfs) — so the result matches `git rev-parse HEAD:<path>`
    // bit-for-bit when the file is committed unchanged. Subprocess keeps semantics in
    // lockstep with the user's installed git, which gix-filter cannot reproduce
    // for non-builtin filters.
    let output = Command::new("git")
        .args(["-C", parent.to_str()?, "hash-object", "--"])
        .arg(file_path)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let hash = String::from_utf8(output.stdout).ok()?.trim().to_string();
    if hash.len() == 40 { Some(hash) } else { None }
}

/// Returns the blob hash of `file_path` at HEAD, or None if the path is not tracked
/// in HEAD (or the repo cannot be opened). Resolved entirely in-process via gix.
pub fn head_blob_hash_from_git(file_path: &Path) -> Option<String> {
    let parent = file_path.parent()?;
    let repo = gix::discover(parent).ok()?;
    let workdir = repo.workdir()?;
    let rel = file_path.strip_prefix(workdir).ok()?;

    let head_commit = repo.head_commit().ok()?;
    let tree = head_commit.tree().ok()?;
    let entry = tree.lookup_entry_by_path(rel).ok()??;
    Some(entry.id().to_hex().to_string())
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
