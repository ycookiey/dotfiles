use std::io::Write;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::EventStream;
use futures_util::StreamExt;
use tokio::signal;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::cache::Quality;
use crate::error::PdfViewError;
use crate::event::{Event as AppEvent, Input};
use crate::input::InputMapper;
use crate::pdf::render_via_cache;
use crate::prefetch::{PrefetchContext, PrefetchManager, detect_direction, prefetch_targets};
use crate::render::{RenderedPage, write_page};
use crate::state::{AppState, DisplayStatus, PageCache};
use crate::terminal::query_dims;

pub const FOOTER_ROWS: u16 = 1;
pub const RESIZE_DEBOUNCE_MS: u64 = 150;

/// Entry point for the interactive viewer. Wires crossterm events and
/// internal AppEvents onto a single mpsc channel and drives the display
/// state machine until a Shutdown event arrives. Returns the last page
/// the user was viewing so the caller can persist it.
pub async fn run(mut app: AppState) -> Result<u32> {
    let (tx, mut rx) = mpsc::channel::<AppEvent>(64);

    let shutdown_tx = tx.clone();
    let shutdown_token = app.shutdown.clone();
    tokio::spawn(async move {
        tokio::select! {
            _ = signal::ctrl_c() => {
                let _ = shutdown_tx.send(AppEvent::Shutdown).await;
            }
            _ = shutdown_token.cancelled() => {}
        }
    });

    let mut mapper = InputMapper::new();
    let mut stream = EventStream::new();
    let mut resize_timer: Option<JoinHandle<()>> = None;
    let mut prefetch = PrefetchManager::new();

    // Initial draw so the user sees something immediately.
    let initial = app.current_page;
    show(&mut app, initial, &tx).await?;
    schedule_prefetch(&app, &mut prefetch);

    loop {
        tokio::select! {
            Some(maybe_ev) = stream.next() => match maybe_ev {
                Ok(ev) => forward_crossterm(ev, &tx, &mut mapper).await,
                Err(e) => {
                    warn!(error = %e, "crossterm event error");
                    break;
                }
            },
            Some(app_ev) = rx.recv() => {
                if handle_event(&mut app, app_ev, &tx, &mut resize_timer, &mut prefetch).await? {
                    break;
                }
            }
        }
    }

    if let Some(h) = resize_timer.take() {
        h.abort();
    }
    prefetch.cancel();
    app.shutdown.cancel();
    Ok(app.current_page)
}

async fn forward_crossterm(
    ev: crossterm::event::Event,
    tx: &mpsc::Sender<AppEvent>,
    mapper: &mut InputMapper,
) {
    use crossterm::event::{Event as CE, KeyEventKind};
    match ev {
        CE::Key(ke) => {
            if ke.kind != KeyEventKind::Press {
                return;
            }
            let input = mapper.map(ke);
            if matches!(input, Input::Quit) {
                let _ = tx.send(AppEvent::Shutdown).await;
            } else if !matches!(input, Input::Noop) {
                let _ = tx.send(AppEvent::Input(input)).await;
            }
        }
        CE::Resize(cols, rows) => {
            let _ = tx.send(AppEvent::Resize { cols, rows }).await;
        }
        other => {
            debug!(?other, "ignored event");
        }
    }
}

async fn handle_event(
    app: &mut AppState,
    ev: AppEvent,
    tx: &mpsc::Sender<AppEvent>,
    resize_timer: &mut Option<JoinHandle<()>>,
    prefetch: &mut PrefetchManager,
) -> Result<bool> {
    match ev {
        AppEvent::Shutdown => {
            info!("shutdown requested");
            Ok(true)
        }
        AppEvent::Input(input) => {
            handle_input(app, input, tx).await?;
            schedule_prefetch(app, prefetch);
            Ok(false)
        }
        AppEvent::Resize { cols, rows } => {
            // Cell counts arrive synchronously via SIGWINCH; pixel dims
            // need a CSI round-trip, so defer the authoritative update
            // to the debounced path. Cancel any outstanding prefetch up
            // front because dims are about to change under its feet.
            prefetch.cancel();
            // Also cancel any in-flight render: its output is sized for
            // the old geometry and would draw in the wrong place.
            app.nav_token.cancel();
            app.nav_token = CancellationToken::new();
            app.term_dims.cols = cols;
            app.term_dims.rows = rows;
            // Blank the screen so the pre-resize image doesn't linger
            // at its stale cell coordinates during the debounce window.
            {
                let mut out = std::io::stdout().lock();
                let _ = crate::render::clear_screen(&mut out);
                let _ = out.flush();
            }
            app.last_drawn = None;
            app.display = DisplayStatus::Empty;
            schedule_resize_debounce(tx.clone(), resize_timer);
            Ok(false)
        }
        AppEvent::DebouncedResize(dims) => {
            handle_debounced_resize(app, dims, tx).await?;
            schedule_prefetch(app, prefetch);
            Ok(false)
        }
        AppEvent::RenderComplete {
            page,
            quality,
            result,
        } => {
            handle_render_complete(app, page, quality, result)?;
            Ok(false)
        }
        AppEvent::Tick => Ok(false),
    }
}

fn schedule_prefetch(app: &AppState, prefetch: &mut PrefetchManager) {
    if app.total_pages == 0 {
        return;
    }
    let dir = detect_direction(&app.history);
    let pages = prefetch_targets(app.current_page, app.total_pages, dir);
    let ctx = PrefetchContext {
        pdfium: Arc::clone(&app.pdfium),
        pdfium_lock: Arc::clone(&app.pdfium_lock),
        pdf_bytes: Arc::clone(&app.pdf_bytes),
        disk: app.disk.clone(),
        term_dims: app.term_dims,
        cache: Arc::clone(&app.cache),
        footer_rows: FOOTER_ROWS,
        quality: Quality::Low,
        shutdown: app.shutdown.clone(),
        nav: app.nav_token.clone(),
    };
    prefetch.schedule(ctx, pages);
}

fn schedule_resize_debounce(
    tx: mpsc::Sender<AppEvent>,
    resize_timer: &mut Option<JoinHandle<()>>,
) {
    // Cancel any in-flight timer so only the final resize in a burst
    // triggers the expensive re-query + re-render.
    if let Some(h) = resize_timer.take() {
        h.abort();
    }
    let handle = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(RESIZE_DEBOUNCE_MS)).await;
        let dims = match tokio::task::spawn_blocking(query_dims).await {
            Ok(Ok(d)) => d,
            Ok(Err(e)) => {
                warn!(error = %e, "query_dims after resize failed");
                return;
            }
            Err(e) => {
                warn!(error = %e, "query_dims join failed");
                return;
            }
        };
        let _ = tx.send(AppEvent::DebouncedResize(dims)).await;
    });
    *resize_timer = Some(handle);
}

async fn handle_debounced_resize(
    app: &mut AppState,
    dims: crate::terminal::TerminalDims,
    tx: &mpsc::Sender<AppEvent>,
) -> Result<()> {
    if dims == app.term_dims {
        return Ok(());
    }
    app.term_dims = dims;
    // Drop every cached RenderedPage: their PNG dims and height= in the
    // OSC 1337 payload are tied to the old terminal geometry.
    if let Ok(mut cache) = app.cache.lock() {
        cache.clear();
    }
    let page = app.current_page;
    app.display = DisplayStatus::Empty;
    show(app, page, tx).await
}

async fn handle_input(
    app: &mut AppState,
    input: Input,
    tx: &mpsc::Sender<AppEvent>,
) -> Result<()> {
    match input {
        Input::Next => navigate(app, app.current_page + 1, tx).await,
        Input::Prev => navigate(app, app.current_page.saturating_sub(1).max(1), tx).await,
        Input::First => navigate(app, 1, tx).await,
        Input::Last => navigate(app, app.total_pages, tx).await,
        Input::Goto(p) => navigate(app, p, tx).await,
        Input::Quit | Input::Noop | Input::Digit(_) | Input::GotoStart => Ok(()),
    }
}

pub async fn navigate(
    app: &mut AppState,
    page: u32,
    tx: &mpsc::Sender<AppEvent>,
) -> Result<()> {
    let target = app.clamp_page(page);
    if app.current_page == target
        && matches!(
            app.display,
            DisplayStatus::Showing { page: p, .. } if p == target
        )
    {
        return Ok(());
    }
    show(app, target, tx).await
}

pub async fn show(
    app: &mut AppState,
    page: u32,
    tx: &mpsc::Sender<AppEvent>,
) -> Result<()> {
    let page = app.clamp_page(page);
    // Cancel any in-flight High render or prefetch from the previous
    // page so they don't keep the pdfium lock busy while the user waits
    // for the new page's render to start.
    app.nav_token.cancel();
    app.nav_token = CancellationToken::new();
    app.current_page = page;
    app.push_history(page);

    // Hit: a High-quality render for the current terminal is already cached.
    if let Some(rp) = cached(app, page, Quality::High) {
        draw(app, &rp)?;
        app.display = DisplayStatus::Showing {
            page,
            quality: Quality::High,
        };
        return Ok(());
    }

    // Near-hit: a Low-quality render is cached — print it while a High
    // render is kicked off in the background.
    if let Some(rp) = cached(app, page, Quality::Low) {
        draw(app, &rp)?;
        app.display = DisplayStatus::Showing {
            page,
            quality: Quality::Low,
        };
    } else {
        // Miss: synchronously render a Low version so the user sees
        // something immediately.
        let low = render_blocking(app, page, Quality::Low).await?;
        store(app, page, low.clone());
        draw(app, &low)?;
        app.display = DisplayStatus::Showing {
            page,
            quality: Quality::Low,
        };
    }

    spawn_high_render(app, page, tx);
    Ok(())
}

fn cached(app: &AppState, page: u32, quality: Quality) -> Option<RenderedPage> {
    app.cache
        .lock()
        .ok()?
        .get(&page)
        .and_then(|pc| pc.get(quality).cloned())
}

fn store(app: &AppState, page: u32, rp: RenderedPage) {
    if let Ok(mut cache) = app.cache.lock() {
        let entry = cache.entry(page).or_insert_with(PageCache::default);
        entry.set(rp);
    }
}

fn draw(app: &mut AppState, rp: &RenderedPage) -> Result<()> {
    let mut out = std::io::stdout().lock();
    // 初回描画 (last_drawn=None) の場合、起動直後のシェルプロンプト残骸が
    // 残らないよう一度だけ画面をクリアする。以降の描画は差分 erase で済む。
    if app.last_drawn.is_none() {
        crate::render::clear_screen(&mut out)?;
    }
    write_page(
        &mut out,
        rp,
        app.last_drawn.as_ref(),
        app.current_page,
        app.total_pages,
        app.term_dims.rows,
    )?;
    out.flush()?;
    drop(out);
    app.last_drawn = Some(rp.clone());
    Ok(())
}

async fn render_blocking(
    app: &AppState,
    page: u32,
    quality: Quality,
) -> std::result::Result<RenderedPage, PdfViewError> {
    let pdfium = Arc::clone(&app.pdfium);
    let lock = Arc::clone(&app.pdfium_lock);
    let bytes = Arc::clone(&app.pdf_bytes);
    let disk = app.disk.clone();
    let term = app.term_dims;
    let token = app.nav_token.clone();
    let rp = tokio::task::spawn_blocking(move || {
        render_via_cache(
            &pdfium,
            &lock,
            &bytes,
            &disk,
            page,
            &term,
            quality,
            FOOTER_ROWS,
            Some(&token),
        )
    })
    .await
    .map_err(|e| PdfViewError::Other(e.into()))??;
    Ok(rp)
}

fn spawn_high_render(app: &AppState, page: u32, tx: &mpsc::Sender<AppEvent>) {
    let pdfium = Arc::clone(&app.pdfium);
    let lock = Arc::clone(&app.pdfium_lock);
    let bytes = Arc::clone(&app.pdf_bytes);
    let disk = app.disk.clone();
    let term = app.term_dims;
    let tx = tx.clone();
    let shutdown = app.shutdown.clone();
    let nav = app.nav_token.clone();
    tokio::spawn(async move {
        if shutdown.is_cancelled() || nav.is_cancelled() {
            return;
        }
        let nav_inner = nav.clone();
        let result = tokio::task::spawn_blocking(move || {
            render_via_cache(
                &pdfium,
                &lock,
                &bytes,
                &disk,
                page,
                &term,
                Quality::High,
                FOOTER_ROWS,
                Some(&nav_inner),
            )
        })
        .await;
        let outcome = match result {
            Ok(Ok(rp)) => Ok(rp),
            Ok(Err(e)) => Err(e),
            Err(join_err) => Err(PdfViewError::Other(join_err.into())),
        };
        let _ = tx
            .send(AppEvent::RenderComplete {
                page,
                quality: Quality::High,
                result: outcome,
            })
            .await;
    });
}

pub fn handle_render_complete(
    app: &mut AppState,
    page: u32,
    quality: Quality,
    result: Result<RenderedPage, PdfViewError>,
) -> Result<()> {
    let rp = match result {
        Ok(rp) => rp,
        Err(PdfViewError::Cancelled) => {
            debug!(page, ?quality, "render cancelled");
            return Ok(());
        }
        Err(e) => {
            warn!(page, ?quality, error = %e, "render failed");
            return Ok(());
        }
    };

    // Stale: the user has navigated away since this render was queued.
    if page != app.current_page {
        store(app, page, rp);
        return Ok(());
    }

    // Stale in a subtler way: the terminal resized and this render is
    // for the wrong pixel dims.
    if !matches_dims(&rp, app) {
        store(app, page, rp);
        return Ok(());
    }

    store(app, page, rp.clone());
    // Only upgrade the display if we're currently showing a Low version
    // of this same page; otherwise the user already has High up.
    if matches!(
        app.display,
        DisplayStatus::Showing { page: p, quality: Quality::Low } if p == page
    ) {
        draw(app, &rp)?;
        app.display = DisplayStatus::Showing {
            page,
            quality: Quality::High,
        };
    }
    Ok(())
}

fn matches_dims(rp: &RenderedPage, app: &AppState) -> bool {
    // 画像はアスペクト比保持でラスタライズされ、PDF が縦長なら高さ制約・
    // 横長なら幅制約のどちらか一方の軸が target に達する。よってどちらか
    // 一方の軸が target と一致していれば現在のターミナル寸法用の画像。
    let (target_w, target_h) = match rp.quality {
        Quality::Low => (app.term_dims.px_w / 3, app.term_dims.px_h),
        Quality::High => (app.term_dims.px_w, app.term_dims.px_h),
    };
    let dw = rp.dims.0.abs_diff(target_w);
    let dh = rp.dims.1.abs_diff(target_h);
    dw <= 2 || dh <= 2
}

// TODO(T-11 follow-up): add handle_render_complete / navigate tests
// by extracting pure logic that doesn't require an Arc<Pdfium>. The
// current shape forces dummy pdfium construction which is unsound.
