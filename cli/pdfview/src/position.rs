use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub const MAX_ENTRIES: usize = 500;
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Entry {
    pub path: String,
    pub page: u32,
    /// Unix timestamp in seconds; simpler than ISO-8601 and monotonic
    /// enough for LRU eviction.
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Store {
    pub version: u32,
    pub entries: HashMap<String, Entry>,
}

impl Default for Store {
    fn default() -> Self {
        Self {
            version: SCHEMA_VERSION,
            entries: HashMap::new(),
        }
    }
}

impl Store {
    pub fn get(&self, sha: &str) -> Option<&Entry> {
        self.entries.get(sha)
    }

    pub fn upsert(&mut self, sha: String, path: String, page: u32, now: u64) {
        self.entries.insert(
            sha,
            Entry {
                path,
                page,
                updated_at: now,
            },
        );
        self.evict_to_limit();
    }

    /// Drop oldest entries by `updated_at` until at most `MAX_ENTRIES`
    /// remain. O(n log n) which is fine at n=500.
    pub fn evict_to_limit(&mut self) {
        if self.entries.len() <= MAX_ENTRIES {
            return;
        }
        let mut keyed: Vec<(u64, String)> = self
            .entries
            .iter()
            .map(|(k, v)| (v.updated_at, k.clone()))
            .collect();
        keyed.sort_by_key(|(t, _)| *t);
        let excess = self.entries.len() - MAX_ENTRIES;
        for (_, k) in keyed.into_iter().take(excess) {
            self.entries.remove(&k);
        }
    }
}

/// Return the on-disk path for the position store. Honors XDG state on
/// Unix and falls back to LocalAppData on Windows.
pub fn default_path() -> PathBuf {
    let base = dirs::state_dir()
        .or_else(dirs::data_local_dir)
        .or_else(dirs::data_dir)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("pdfview").join("positions.json")
}

pub fn load(path: &Path) -> Result<Store> {
    match fs::read(path) {
        Ok(bytes) => {
            // A malformed file shouldn't brick the viewer; log and start fresh.
            match serde_json::from_slice::<Store>(&bytes) {
                Ok(mut s) => {
                    if s.version != SCHEMA_VERSION {
                        return Ok(Store::default());
                    }
                    s.evict_to_limit();
                    Ok(s)
                }
                Err(e) => {
                    tracing::warn!(error = %e, path = %path.display(), "positions.json parse failed, starting empty");
                    Ok(Store::default())
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Store::default()),
        Err(e) => Err(anyhow::Error::from(e).context(format!("read {}", path.display()))),
    }
}

pub fn save(path: &Path, store: &Store) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create {}", parent.display()))?;
    }
    // Atomic write: serialize to a sibling tmp file and rename, so a
    // crash mid-write never leaves a half-written JSON blob behind.
    let bytes = serde_json::to_vec_pretty(store).context("serialize positions")?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, &bytes)
        .with_context(|| format!("write {}", tmp.display()))?;
    fs::rename(&tmp, path)
        .with_context(|| format!("rename {} -> {}", tmp.display(), path.display()))?;
    Ok(())
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upsert_overwrites_same_sha() {
        let mut s = Store::default();
        s.upsert("abc".into(), "/a.pdf".into(), 1, 10);
        s.upsert("abc".into(), "/a.pdf".into(), 5, 20);
        assert_eq!(s.get("abc").unwrap().page, 5);
        assert_eq!(s.entries.len(), 1);
    }

    #[test]
    fn evict_drops_oldest_when_over_limit() {
        let mut s = Store::default();
        for i in 0..(MAX_ENTRIES as u64 + 3) {
            s.upsert(format!("sha{i}"), format!("/p{i}.pdf"), 1, i);
        }
        assert_eq!(s.entries.len(), MAX_ENTRIES);
        // The oldest three must have been evicted.
        assert!(s.get("sha0").is_none());
        assert!(s.get("sha1").is_none());
        assert!(s.get("sha2").is_none());
        // The newest one survives.
        assert!(s.get(&format!("sha{}", MAX_ENTRIES as u64 + 2)).is_some());
    }

    #[test]
    fn load_returns_default_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nope.json");
        let s = load(&path).unwrap();
        assert_eq!(s, Store::default());
    }

    #[test]
    fn load_returns_default_on_corrupt_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("positions.json");
        fs::write(&path, b"not json").unwrap();
        let s = load(&path).unwrap();
        assert_eq!(s, Store::default());
    }

    #[test]
    fn load_resets_on_version_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("positions.json");
        let mut m = HashMap::new();
        m.insert(
            "abc".to_string(),
            Entry {
                path: "/a.pdf".into(),
                page: 1,
                updated_at: 0,
            },
        );
        let stored = Store {
            version: 999,
            entries: m,
        };
        fs::write(&path, serde_json::to_vec(&stored).unwrap()).unwrap();
        let s = load(&path).unwrap();
        assert!(s.entries.is_empty());
    }

    #[test]
    fn save_then_load_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("positions.json");
        let mut s = Store::default();
        s.upsert("sha1".into(), "/a.pdf".into(), 7, 100);
        save(&path, &s).unwrap();
        let loaded = load(&path).unwrap();
        assert_eq!(loaded.get("sha1").unwrap().page, 7);
        assert_eq!(loaded.get("sha1").unwrap().path, "/a.pdf");
    }
}
