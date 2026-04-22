use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use pdfium_render::prelude::Pdfium;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::debug;

use crate::cache::{DiskCache, Quality};
use crate::pdf::render_via_cache;
use crate::state::PageCacheMap;
use crate::terminal::TerminalDims;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Forward,
    Backward,
}

/// Inspect the last few navigation entries to guess which way the user
/// is heading. Ties and short histories default to Forward since that's
/// by far the most common reading pattern.
pub fn detect_direction(history: &VecDeque<u32>) -> Direction {
    if history.len() < 2 {
        return Direction::Forward;
    }
    let recent: Vec<u32> = history.iter().rev().take(3).copied().collect();
    let mut forward = 0i32;
    let mut backward = 0i32;
    for w in recent.windows(2) {
        let delta = w[0] as i32 - w[1] as i32;
        if delta > 0 {
            forward += 1;
        } else if delta < 0 {
            backward += 1;
        }
    }
    if backward > forward {
        Direction::Backward
    } else {
        Direction::Forward
    }
}

/// Build the list of pages to prefetch around `current`, clamped to
/// `[1, total]` and skipping `current` itself. Layout per plan:
/// asymmetric 3/1 biased toward `dir`.
pub fn prefetch_targets(current: u32, total: u32, dir: Direction) -> Vec<u32> {
    if total == 0 {
        return Vec::new();
    }
    let candidates: [i64; 4] = match dir {
        Direction::Forward => [
            current as i64 + 1,
            current as i64 + 2,
            current as i64 + 3,
            current as i64 - 1,
        ],
        Direction::Backward => [
            current as i64 - 1,
            current as i64 - 2,
            current as i64 - 3,
            current as i64 + 1,
        ],
    };
    candidates
        .into_iter()
        .filter_map(|p| {
            if p >= 1 && p <= total as i64 {
                Some(p as u32)
            } else {
                None
            }
        })
        .filter(|&p| p != current)
        .collect()
}

pub struct PrefetchManager {
    token: CancellationToken,
    handle: Option<JoinHandle<()>>,
}

impl PrefetchManager {
    pub fn new() -> Self {
        Self {
            token: CancellationToken::new(),
            handle: None,
        }
    }

    /// Cancel any in-flight prefetch and schedule a new sequential pass
    /// for `pages`. Each page is rendered on a spawn_blocking worker and
    /// the result is written into the in-memory cache so subsequent
    /// navigation hits it without doing its own render.
    pub fn schedule(&mut self, ctx: PrefetchContext, pages: Vec<u32>) {
        self.cancel();
        if pages.is_empty() {
            return;
        }
        let token = CancellationToken::new();
        self.token = token.clone();
        let handle = tokio::spawn(run_prefetch(ctx, pages, token));
        self.handle = Some(handle);
    }

    pub fn cancel(&mut self) {
        self.token.cancel();
        if let Some(h) = self.handle.take() {
            h.abort();
        }
    }
}

impl Default for PrefetchManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for PrefetchManager {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// Delay before the first prefetch render in a scheduled pass. Gives the
/// current page's High render a head-start on the pdfium lock so rapid
/// page turns aren't bottlenecked by stale prefetches.
pub const PREFETCH_WARMUP_MS: u64 = 200;

#[derive(Clone)]
pub struct PrefetchContext {
    pub pdfium: Arc<Pdfium>,
    pub pdfium_lock: Arc<Mutex<()>>,
    pub pdf_bytes: Arc<Vec<u8>>,
    pub disk: DiskCache,
    pub term_dims: TerminalDims,
    pub cache: Arc<Mutex<PageCacheMap>>,
    pub footer_rows: u16,
    pub quality: Quality,
    pub shutdown: CancellationToken,
    pub nav: CancellationToken,
}

async fn run_prefetch(
    ctx: PrefetchContext,
    pages: Vec<u32>,
    token: CancellationToken,
) {
    // Let the user's current-page render get to the pdfium lock first.
    // If the user presses another key during the warmup, nav.cancel()
    // trips here and we bail before doing any work.
    tokio::select! {
        _ = tokio::time::sleep(std::time::Duration::from_millis(PREFETCH_WARMUP_MS)) => {}
        _ = token.cancelled() => return,
        _ = ctx.nav.cancelled() => return,
        _ = ctx.shutdown.cancelled() => return,
    }

    for page in pages {
        if token.is_cancelled() || ctx.nav.is_cancelled() || ctx.shutdown.is_cancelled() {
            debug!(page, "prefetch cancelled");
            return;
        }
        // Skip if the memory cache already has an entry at this quality.
        let already = ctx
            .cache
            .lock()
            .ok()
            .and_then(|m| m.get(&page).and_then(|pc| pc.get(ctx.quality).cloned()))
            .is_some();
        if already {
            continue;
        }

        let pdfium = Arc::clone(&ctx.pdfium);
        let lock = Arc::clone(&ctx.pdfium_lock);
        let bytes = Arc::clone(&ctx.pdf_bytes);
        let disk = ctx.disk.clone();
        let term = ctx.term_dims;
        let quality = ctx.quality;
        let footer_rows = ctx.footer_rows;
        let nav = ctx.nav.clone();
        let join = tokio::task::spawn_blocking(move || {
            render_via_cache(
                &pdfium,
                &lock,
                &bytes,
                &disk,
                page,
                &term,
                quality,
                footer_rows,
                Some(&nav),
            )
        });
        let rp = match join.await {
            Ok(Ok(rp)) => rp,
            Ok(Err(e)) => {
                debug!(page, error = %e, "prefetch render failed");
                continue;
            }
            Err(e) => {
                debug!(page, error = %e, "prefetch join failed");
                return;
            }
        };
        if token.is_cancelled() || ctx.nav.is_cancelled() || ctx.shutdown.is_cancelled() {
            return;
        }
        if let Ok(mut cache) = ctx.cache.lock() {
            let entry = cache.entry(page).or_default();
            entry.set(rp);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(pages: &[u32]) -> VecDeque<u32> {
        pages.iter().copied().collect()
    }

    #[test]
    fn direction_defaults_forward_on_short_history() {
        assert_eq!(detect_direction(&h(&[])), Direction::Forward);
        assert_eq!(detect_direction(&h(&[5])), Direction::Forward);
    }

    #[test]
    fn direction_forward_for_ascending_history() {
        assert_eq!(detect_direction(&h(&[1, 2, 3, 4])), Direction::Forward);
    }

    #[test]
    fn direction_backward_for_descending_history() {
        assert_eq!(detect_direction(&h(&[10, 9, 8, 7])), Direction::Backward);
    }

    #[test]
    fn direction_ignores_old_entries() {
        // Older entries shouldn't sway a clearly-backward tail.
        let hist = h(&[1, 2, 3, 4, 5, 4, 3]);
        assert_eq!(detect_direction(&hist), Direction::Backward);
    }

    #[test]
    fn targets_forward_prefers_three_ahead_one_behind() {
        assert_eq!(prefetch_targets(5, 100, Direction::Forward), vec![6, 7, 8, 4]);
    }

    #[test]
    fn targets_backward_prefers_three_behind_one_ahead() {
        assert_eq!(prefetch_targets(10, 100, Direction::Backward), vec![9, 8, 7, 11]);
    }

    #[test]
    fn targets_clamp_at_start_of_document() {
        assert_eq!(prefetch_targets(1, 10, Direction::Forward), vec![2, 3, 4]);
        assert_eq!(prefetch_targets(1, 10, Direction::Backward), vec![2]);
    }

    #[test]
    fn targets_clamp_at_end_of_document() {
        assert_eq!(prefetch_targets(10, 10, Direction::Forward), vec![9]);
        assert_eq!(prefetch_targets(10, 10, Direction::Backward), vec![9, 8, 7]);
    }

    #[test]
    fn targets_empty_for_zero_total() {
        assert!(prefetch_targets(1, 0, Direction::Forward).is_empty());
    }
}
