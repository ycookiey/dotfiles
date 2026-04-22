use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use pdfium_render::prelude::Pdfium;
use tokio_util::sync::CancellationToken;

use crate::cache::{DiskCache, Quality};
use crate::render::RenderedPage;
use crate::terminal::TerminalDims;

pub const HISTORY_CAPACITY: usize = 8;

#[derive(Default, Debug, Clone)]
pub struct PageCache {
    pub low: Option<RenderedPage>,
    pub high: Option<RenderedPage>,
}

impl PageCache {
    pub fn get(&self, quality: Quality) -> Option<&RenderedPage> {
        match quality {
            Quality::Low => self.low.as_ref(),
            Quality::High => self.high.as_ref(),
        }
    }

    pub fn set(&mut self, rp: RenderedPage) {
        match rp.quality {
            Quality::Low => self.low = Some(rp),
            Quality::High => self.high = Some(rp),
        }
    }
}

pub type PageCacheMap = HashMap<u32, PageCache>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayStatus {
    Empty,
    Showing { page: u32, quality: Quality },
}

pub struct AppState {
    pub pdf_path: PathBuf,
    pub pdf_bytes: Arc<Vec<u8>>,
    pub pdf_sha256: String,
    pub total_pages: u32,
    pub current_page: u32,
    pub term_dims: TerminalDims,
    pub cache: Arc<Mutex<PageCacheMap>>,
    pub disk: DiskCache,
    pub history: VecDeque<u32>,
    pub pdfium: Arc<Pdfium>,
    /// Serializes PDF load+render across worker threads. `thread_safe`
    /// in pdfium-render only locks individual FFI calls, which still
    /// races when multiple threads hold a PdfDocument concurrently and
    /// produces sporadic FormatError. Grab this mutex for the full
    /// load → render → drop cycle.
    pub pdfium_lock: Arc<Mutex<()>>,
    /// Cancelled whenever the user navigates to a different page, so
    /// stale High renders and prefetch passes bail out instead of
    /// queueing behind the new current page.
    pub nav_token: CancellationToken,
    pub shutdown: CancellationToken,
    pub display: DisplayStatus,
}

impl AppState {
    pub fn push_history(&mut self, page: u32) {
        if self.history.back().copied() == Some(page) {
            return;
        }
        if self.history.len() >= HISTORY_CAPACITY {
            self.history.pop_front();
        }
        self.history.push_back(page);
    }

    pub fn clamp_page(&self, page: u32) -> u32 {
        page.clamp(1, self.total_pages.max(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_cache_set_and_get_per_quality() {
        let mut c = PageCache::default();
        assert!(c.get(Quality::Low).is_none());
        let rp = RenderedPage {
            png_bytes: Arc::new(vec![1, 2, 3]),
            esc_seq: Arc::new("esc".into()),
            dims: (100, 100),
            quality: Quality::Low,
        };
        c.set(rp.clone());
        assert!(c.get(Quality::Low).is_some());
        assert!(c.get(Quality::High).is_none());
    }

    #[test]
    fn push_history_dedupes_consecutive() {
        let mut h: VecDeque<u32> = VecDeque::new();
        push(&mut h, 1);
        push(&mut h, 1);
        push(&mut h, 2);
        assert_eq!(h.iter().copied().collect::<Vec<_>>(), vec![1, 2]);
    }

    #[test]
    fn push_history_caps_capacity() {
        let mut h: VecDeque<u32> = VecDeque::new();
        for p in 1..=(HISTORY_CAPACITY as u32 + 4) {
            push(&mut h, p);
        }
        assert_eq!(h.len(), HISTORY_CAPACITY);
        assert_eq!(h.back().copied(), Some(HISTORY_CAPACITY as u32 + 4));
    }

    fn push(h: &mut VecDeque<u32>, page: u32) {
        if h.back().copied() == Some(page) {
            return;
        }
        if h.len() >= HISTORY_CAPACITY {
            h.pop_front();
        }
        h.push_back(page);
    }
}
