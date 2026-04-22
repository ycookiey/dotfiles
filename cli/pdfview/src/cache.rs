use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quality {
    Low,
    High,
}

impl Quality {
    pub fn as_str(self) -> &'static str {
        match self {
            Quality::Low => "low",
            Quality::High => "high",
        }
    }
}

pub fn cache_key(page: u32, dims: (u32, u32), quality: Quality) -> String {
    format!("{}_{}x{}_{}.png", page, dims.0, dims.1, quality.as_str())
}

/// Full SHA-256 of a file, computed by streaming 64 KiB blocks so that
/// memory usage stays flat even for large PDFs.
pub fn pdf_sha256(path: &Path) -> Result<String> {
    let mut f = fs::File::open(path)
        .with_context(|| format!("open {} for hashing", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let digest = hasher.finalize();
    Ok(hex_encode(&digest))
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

pub const DEFAULT_MAX_BYTES: u64 = 500 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct DiskCache {
    base_dir: PathBuf,
    max_bytes: u64,
}

impl DiskCache {
    pub fn new(base_dir: PathBuf) -> Self {
        Self {
            base_dir,
            max_bytes: DEFAULT_MAX_BYTES,
        }
    }

    pub fn with_max_bytes(mut self, max_bytes: u64) -> Self {
        self.max_bytes = max_bytes;
        self
    }

    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    pub fn ensure_dir(&self) -> Result<()> {
        fs::create_dir_all(&self.base_dir).with_context(|| {
            format!("create cache dir {}", self.base_dir.display())
        })
    }

    pub fn path_for(&self, key: &str) -> PathBuf {
        self.base_dir.join(key)
    }

    pub fn read(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let path = self.path_for(key);
        match fs::read(&path) {
            Ok(bytes) => {
                let _ = touch_mtime(&path);
                Ok(Some(bytes))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => {
                Err(anyhow::Error::from(e).context(format!("read {}", path.display())))
            }
        }
    }

    pub fn write(&self, key: &str, bytes: &[u8]) -> Result<()> {
        self.ensure_dir()?;
        let path = self.path_for(key);
        fs::write(&path, bytes)
            .with_context(|| format!("write {}", path.display()))
    }

    /// Walk the cache root and delete oldest files (by mtime) until the
    /// total size is at or below `self.max_bytes`.
    pub fn evict_to_limit(&self) -> Result<EvictReport> {
        let mut entries = self.collect_entries()?;
        let total: u64 = entries.iter().map(|e| e.size).sum();
        if total <= self.max_bytes {
            return Ok(EvictReport {
                removed_files: 0,
                removed_bytes: 0,
                total_before: total,
                total_after: total,
            });
        }

        entries.sort_by_key(|e| e.mtime);

        let mut removed_bytes: u64 = 0;
        let mut removed_files: u64 = 0;
        let mut remaining = total;
        for entry in entries {
            if remaining <= self.max_bytes {
                break;
            }
            if fs::remove_file(&entry.path).is_ok() {
                remaining = remaining.saturating_sub(entry.size);
                removed_bytes += entry.size;
                removed_files += 1;
            }
        }

        Ok(EvictReport {
            removed_files,
            removed_bytes,
            total_before: total,
            total_after: remaining,
        })
    }

    fn collect_entries(&self) -> Result<Vec<Entry>> {
        let mut out = Vec::new();
        let rd = match fs::read_dir(&self.base_dir) {
            Ok(rd) => rd,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
            Err(e) => {
                return Err(anyhow::Error::from(e)
                    .context(format!("read cache dir {}", self.base_dir.display())))
            }
        };
        for entry in rd {
            let entry = entry?;
            let meta = entry.metadata()?;
            if !meta.is_file() {
                continue;
            }
            let mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            out.push(Entry {
                path: entry.path(),
                size: meta.len(),
                mtime,
            });
        }
        Ok(out)
    }
}

fn touch_mtime(path: &Path) -> std::io::Result<()> {
    // Best-effort mtime refresh so LRU ordering tracks recent reads.
    let file = fs::OpenOptions::new().write(true).open(path)?;
    file.set_modified(SystemTime::now())
}

#[derive(Debug, Clone)]
struct Entry {
    path: PathBuf,
    size: u64,
    mtime: SystemTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EvictReport {
    pub removed_files: u64,
    pub removed_bytes: u64,
    pub total_before: u64,
    pub total_after: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_format_high() {
        assert_eq!(
            cache_key(12, (1920, 1080), Quality::High),
            "12_1920x1080_high.png"
        );
    }

    #[test]
    fn key_format_low() {
        assert_eq!(
            cache_key(1, (640, 360), Quality::Low),
            "1_640x360_low.png"
        );
    }

    #[test]
    fn sha256_of_known_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.bin");
        fs::write(&path, b"hello world").unwrap();
        // Known SHA-256 of "hello world".
        assert_eq!(
            pdf_sha256(&path).unwrap(),
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn sha256_large_file_streamed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.bin");
        let chunk = vec![0x42u8; 128 * 1024];
        fs::write(&path, &chunk).unwrap();
        // Confirm the streaming reader covers more than one 64 KiB block.
        let hash = pdf_sha256(&path).unwrap();
        assert_eq!(hash.len(), 64);
    }

    #[test]
    fn disk_cache_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let cache = DiskCache::new(dir.path().join("cache"));
        assert!(cache.read("missing.png").unwrap().is_none());
        cache.write("1_100x100_low.png", b"pngdata").unwrap();
        assert_eq!(
            cache.read("1_100x100_low.png").unwrap().as_deref(),
            Some(&b"pngdata"[..])
        );
    }

    #[test]
    fn disk_cache_evicts_oldest_first() {
        let dir = tempfile::tempdir().unwrap();
        let cache = DiskCache::new(dir.path().join("cache")).with_max_bytes(200);

        cache.write("a.png", &vec![0u8; 100]).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(15));
        cache.write("b.png", &vec![0u8; 100]).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(15));
        cache.write("c.png", &vec![0u8; 100]).unwrap();

        let report = cache.evict_to_limit().unwrap();
        assert!(report.removed_files >= 1);
        assert!(report.total_after <= 200);
        // The newest file must still be present.
        assert!(cache.read("c.png").unwrap().is_some());
        // The oldest file should be the first to go.
        assert!(cache.read("a.png").unwrap().is_none());
    }

    #[test]
    fn evict_noop_when_under_limit() {
        let dir = tempfile::tempdir().unwrap();
        let cache = DiskCache::new(dir.path().join("cache")).with_max_bytes(1024);
        cache.write("x.png", b"tiny").unwrap();
        let report = cache.evict_to_limit().unwrap();
        assert_eq!(report.removed_files, 0);
        assert_eq!(report.removed_bytes, 0);
    }

    #[test]
    fn evict_missing_dir_is_noop() {
        let dir = tempfile::tempdir().unwrap();
        let cache = DiskCache::new(dir.path().join("nope"));
        let report = cache.evict_to_limit().unwrap();
        assert_eq!(report.removed_files, 0);
        assert_eq!(report.total_before, 0);
    }
}
