use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use image::codecs::png::PngEncoder;
use image::{ColorType, ImageEncoder};
use pdfium_render::prelude::*;

use crate::cache::{Quality, cache_key, DiskCache};
use crate::error::PdfViewError;
use crate::render::{RenderedPage, build_rendered_page};
use crate::terminal::TerminalDims;

/// Load the pdfium dynamic library. Tries the executable's directory first
/// (Scoop-style side-by-side install), then falls back to whatever the
/// OS loader can find on PATH. Returns an Arc since the `thread_safe`
/// feature is enabled and the handle is shared across spawn_blocking tasks.
pub fn load_pdfium() -> std::result::Result<Arc<Pdfium>, PdfViewError> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."));

    let bindings = Pdfium::bind_to_library(
        Pdfium::pdfium_platform_library_name_at_path(&exe_dir),
    )
    .or_else(|_| Pdfium::bind_to_system_library())
    .map_err(|e| PdfViewError::PdfiumInit(e.to_string()))?;

    Ok(Arc::new(Pdfium::new(bindings)))
}

pub fn page_count(pdfium: &Pdfium, bytes: &[u8]) -> std::result::Result<u32, PdfViewError> {
    let doc = open_pdf(pdfium, bytes)?;
    Ok(doc.pages().len() as u32)
}

fn open_pdf<'a>(
    pdfium: &'a Pdfium,
    bytes: &'a [u8],
) -> std::result::Result<PdfDocument<'a>, PdfViewError> {
    pdfium.load_pdf_from_byte_slice(bytes, None).map_err(|e| match e {
        PdfiumError::PdfiumLibraryInternalError(
            PdfiumInternalError::PasswordError,
        ) => PdfViewError::PasswordProtected,
        other => PdfViewError::Other(
            anyhow::Error::msg(format!("open pdf: {other:?}")),
        ),
    })
}

/// Derive the render pixel size for the given quality. Low is roughly a
/// third of the terminal's width so a placeholder lands quickly; High
/// targets the full terminal width for pixel-accurate display.
pub fn compute_dims(term: &TerminalDims, quality: Quality) -> (u32, u32) {
    let w = match quality {
        Quality::Low => (term.px_w / 3).max(64),
        Quality::High => term.px_w.max(128),
    };
    // Reserve one row for the footer when computing height.
    let footer_px = term.cell_h;
    let h = term.px_h.saturating_sub(footer_px).max(64);
    (w, h)
}

/// Load a single page and render it as PNG bytes at the requested max
/// pixel box. The height is a hard cap; the output is letterboxed to
/// preserve the PDF's aspect ratio.
pub fn render_page_png(
    pdfium: &Pdfium,
    bytes: &[u8],
    page: u32,
    dims: (u32, u32),
) -> std::result::Result<(Vec<u8>, u32, u32), PdfViewError> {
    let doc = open_pdf(pdfium, bytes)?;
    let pages = doc.pages();
    let idx = page.checked_sub(1).ok_or_else(|| {
        PdfViewError::Other(anyhow::anyhow!("page must be 1-indexed"))
    })?;
    let page_obj = pages.get(idx as i32).map_err(|e| PdfViewError::RenderFailed {
        page,
        source: anyhow::Error::msg(format!("{e:?}")),
    })?;

    let cfg = PdfRenderConfig::new()
        .set_target_width(dims.0 as i32)
        .set_maximum_height(dims.1 as i32);

    let rendered = page_obj
        .render_with_config(&cfg)
        .map_err(|e| PdfViewError::RenderFailed {
            page,
            source: anyhow::Error::msg(format!("{e:?}")),
        })?;
    let img = rendered
        .as_image()
        .map_err(|e| PdfViewError::RenderFailed {
            page,
            source: anyhow::Error::msg(format!("{e:?}")),
        })?;
    let rgba = img.as_rgba8().ok_or_else(|| PdfViewError::RenderFailed {
        page,
        source: anyhow::anyhow!("rgba conversion"),
    })?;
    let (w, h) = (rgba.width(), rgba.height());

    let mut png = Vec::with_capacity(w as usize * h as usize);
    PngEncoder::new(&mut png)
        .write_image(rgba.as_raw(), w, h, ColorType::Rgba8.into())
        .context("png encode")?;
    Ok((png, w, h))
}

/// Full render pipeline: consult the disk cache, render via pdfium on
/// miss, write back to disk, and wrap the result in a RenderedPage with
/// the appropriate iTerm2 escape sequence.
pub fn render_via_cache(
    pdfium: &Pdfium,
    pdfium_lock: &std::sync::Mutex<()>,
    pdf_bytes: &[u8],
    disk: &DiskCache,
    page: u32,
    term: &TerminalDims,
    quality: Quality,
    footer_rows: u16,
    cancel: Option<&tokio_util::sync::CancellationToken>,
) -> std::result::Result<RenderedPage, PdfViewError> {
    let is_cancelled = || cancel.map(|t| t.is_cancelled()).unwrap_or(false);

    let target_dims = compute_dims(term, quality);
    // Try every key that matches (page, quality); dims may differ by a
    // few pixels after a resize but we only hit on exact matches here.
    let key = cache_key(page, target_dims, quality);
    if let Some(bytes) = disk.read(&key)? {
        return Ok(build_rendered_page(
            bytes,
            target_dims,
            quality,
            footer_rows,
            term.rows,
            term.cols,
        ));
    }

    // Cheap bail before fighting for the pdfium lock.
    if is_cancelled() {
        return Err(PdfViewError::Cancelled);
    }
    let _guard = pdfium_lock.lock().unwrap_or_else(|e| e.into_inner());
    // Re-check: the caller may have moved on while we waited for the lock.
    if is_cancelled() {
        return Err(PdfViewError::Cancelled);
    }
    let (png, w, h) = render_page_png(pdfium, pdf_bytes, page, target_dims)?;
    let actual_dims = (w, h);
    let out_key = cache_key(page, actual_dims, quality);
    if let Err(e) = disk.write(&out_key, &png) {
        tracing::warn!(error = %e, key = %out_key, "disk cache write failed");
    }
    Ok(build_rendered_page(
        png,
        actual_dims,
        quality,
        footer_rows,
        term.rows,
        term.cols,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dims(px_w: u32, px_h: u32) -> TerminalDims {
        TerminalDims {
            cols: 80,
            rows: 24,
            px_w,
            px_h,
            cell_w: 8,
            cell_h: 16,
            is_fallback: false,
        }
    }

    #[test]
    fn compute_dims_low_is_one_third_width() {
        let t = dims(1920, 1080);
        let (w, _) = compute_dims(&t, Quality::Low);
        assert_eq!(w, 640);
    }

    #[test]
    fn compute_dims_high_is_full_width() {
        let t = dims(1920, 1080);
        let (w, _) = compute_dims(&t, Quality::High);
        assert_eq!(w, 1920);
    }

    #[test]
    fn compute_dims_reserves_footer_row() {
        let t = dims(1920, 1080);
        let (_, h) = compute_dims(&t, Quality::High);
        assert_eq!(h, 1080 - 16);
    }

    #[test]
    fn compute_dims_enforces_floor() {
        let t = dims(10, 10);
        let (w, h) = compute_dims(&t, Quality::Low);
        assert!(w >= 64);
        assert!(h >= 64);
    }
}
