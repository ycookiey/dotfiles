use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::{Arc, Mutex};

use anyhow::Context;
use clap::Parser;
use tokio_util::sync::CancellationToken;
use tracing::info;

use pdfview::app;
use pdfview::cache::{DiskCache, pdf_sha256};
use pdfview::error::PdfViewError;
use pdfview::pdf::{load_pdfium, page_count};
use pdfview::position;
use pdfview::state::{AppState, DisplayStatus};
use pdfview::terminal::query_dims;

#[derive(Parser, Debug)]
#[command(name = "pdfview", about = "Terminal PDF viewer (iTerm2 inline images)")]
struct Cli {
    /// Path to the PDF to open.
    pdf: PathBuf,
    /// Optional 1-indexed page to show first.
    page: Option<u32>,
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("pdfview=info")),
        )
        .with_writer(std::io::stderr)
        .init();

    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("pdfview: {e}");
            ExitCode::from(e.exit_code() as u8)
        }
    }
}

async fn run() -> Result<(), PdfViewError> {
    let cli = Cli::parse();
    if !cli.pdf.exists() {
        return Err(PdfViewError::FileNotFound(cli.pdf.clone()));
    }
    info!(path = %cli.pdf.display(), "pdfview starting");

    let state = build_state(&cli).await?;
    let positions_path = position::default_path();
    let pdf_sha = state.pdf_sha256.clone();
    let pdf_path_str = state.pdf_path.to_string_lossy().to_string();

    crossterm::terminal::enable_raw_mode()
        .context("enable raw mode")
        .map_err(PdfViewError::Other)?;
    let _guard = RawModeGuard;

    let last_page = app::run(state)
        .await
        .map_err(PdfViewError::Other)?;
    save_position(&positions_path, &pdf_sha, &pdf_path_str, last_page);
    Ok(())
}

fn save_position(
    positions_path: &std::path::Path,
    pdf_sha: &str,
    pdf_path: &str,
    last_page: u32,
) {
    let mut store = match position::load(positions_path) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, "load positions failed");
            position::Store::default()
        }
    };
    store.upsert(
        pdf_sha.to_string(),
        pdf_path.to_string(),
        last_page,
        position::now_secs(),
    );
    if let Err(e) = position::save(positions_path, &store) {
        tracing::warn!(error = %e, "save positions failed");
    }
}

async fn build_state(cli: &Cli) -> Result<AppState, PdfViewError> {
    let pdf_path = cli.pdf.clone();
    let pdfium = load_pdfium()?;

    let pdf_bytes = {
        let path = pdf_path.clone();
        let bytes = tokio::task::spawn_blocking(move || std::fs::read(&path))
            .await
            .map_err(|e| PdfViewError::Other(e.into()))?
            .map_err(|e| PdfViewError::Other(e.into()))?;
        Arc::new(bytes)
    };

    let total_pages = {
        let pdfium = Arc::clone(&pdfium);
        let bytes = Arc::clone(&pdf_bytes);
        tokio::task::spawn_blocking(move || page_count(&pdfium, &bytes))
            .await
            .map_err(|e| PdfViewError::Other(e.into()))??
    };

    let pdf_sha = {
        let path = pdf_path.clone();
        tokio::task::spawn_blocking(move || pdf_sha256(&path))
            .await
            .map_err(|e| PdfViewError::Other(e.into()))?
            .map_err(PdfViewError::Other)?
    };

    // Restore last-seen page only when the user didn't pass an explicit
    // page on the CLI (matches the legacy bash viewer's behavior).
    let restored_page = if cli.page.is_none() {
        let pp = position::default_path();
        match position::load(&pp) {
            Ok(store) => store.get(&pdf_sha).map(|e| e.page),
            Err(e) => {
                tracing::warn!(error = %e, "load positions failed");
                None
            }
        }
    } else {
        None
    };

    let term_dims = tokio::task::spawn_blocking(query_dims)
        .await
        .map_err(|e| PdfViewError::Other(e.into()))?
        .map_err(PdfViewError::Other)?;

    let cache_root = dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("pdfview")
        .join(&pdf_sha);
    let disk = DiskCache::new(cache_root);
    disk.ensure_dir().map_err(PdfViewError::Other)?;
    let _ = disk.evict_to_limit();

    let start_page = cli
        .page
        .or(restored_page)
        .unwrap_or(1)
        .clamp(1, total_pages.max(1));

    Ok(AppState {
        pdf_path,
        pdf_bytes,
        pdf_sha256: pdf_sha,
        pdfium_lock: Arc::new(Mutex::new(())),
        nav_token: CancellationToken::new(),
        total_pages,
        current_page: start_page,
        term_dims,
        cache: Arc::new(Mutex::new(HashMap::new())),
        disk,
        history: VecDeque::new(),
        pdfium,
        shutdown: CancellationToken::new(),
        display: DisplayStatus::Empty,
    })
}

struct RawModeGuard;

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = crossterm::terminal::disable_raw_mode();
    }
}
