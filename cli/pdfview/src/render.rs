use std::io::Write;
use std::sync::Arc;

use anyhow::Result;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;

use crate::cache::Quality;

#[derive(Debug, Clone)]
pub struct RenderedPage {
    pub png_bytes: Arc<Vec<u8>>,
    pub esc_seq: Arc<String>,
    pub dims: (u32, u32),
    pub quality: Quality,
}

/// Build the iTerm2 inline-image escape sequence for a PNG payload.
/// Both `cell_width` and `cell_height` constrain the display box; with
/// `preserveAspectRatio=1` the image is letter/pillarboxed to fit inside.
/// Pass `None` for either dimension to leave that axis unconstrained.
pub fn build_esc_seq(
    png: &[u8],
    cell_width: Option<u16>,
    cell_height: Option<u16>,
) -> String {
    let mut params = format!("File=inline=1;size={}", png.len());
    if let Some(w) = cell_width {
        params.push_str(&format!(";width={w}"));
    }
    if let Some(h) = cell_height {
        params.push_str(&format!(";height={h}"));
    }
    params.push_str(";preserveAspectRatio=1");
    let encoded = BASE64.encode(png);
    format!("\x1b]1337;{params}:{encoded}\x07")
}

pub fn build_rendered_page(
    png_bytes: Vec<u8>,
    dims: (u32, u32),
    quality: Quality,
    footer_rows: u16,
    total_rows: u16,
    total_cols: u16,
) -> RenderedPage {
    let cell_height = total_rows
        .saturating_sub(footer_rows)
        .max(1);
    let cell_width = total_cols.max(1);
    let esc = build_esc_seq(&png_bytes, Some(cell_width), Some(cell_height));
    RenderedPage {
        png_bytes: Arc::new(png_bytes),
        esc_seq: Arc::new(esc),
        dims,
        quality,
    }
}

pub fn clear_screen<W: Write>(out: &mut W) -> Result<()> {
    // ESC[2J clears the whole screen, ESC[H moves the cursor to the
    // top-left. Using raw escapes here (instead of crossterm::execute!)
    // keeps this module dependency-light and easy to unit-test.
    out.write_all(b"\x1b[2J\x1b[H")?;
    Ok(())
}

pub fn write_page<W: Write>(
    out: &mut W,
    rendered: &RenderedPage,
    page: u32,
    total: u32,
) -> Result<()> {
    clear_screen(out)?;
    out.write_all(rendered.esc_seq.as_bytes())?;
    write_footer(out, page, total)?;
    out.flush()?;
    Ok(())
}

pub fn write_footer<W: Write>(out: &mut W, page: u32, total: u32) -> Result<()> {
    write!(out, "\n[{page}/{total}] ")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode_payload(esc: &str) -> Vec<u8> {
        // Extract the bytes between ':' and the terminating BEL.
        let start = esc.find(':').unwrap() + 1;
        let end = esc.rfind('\x07').unwrap();
        BASE64.decode(&esc[start..end]).unwrap()
    }

    #[test]
    fn esc_seq_wraps_osc_1337_with_bel() {
        let esc = build_esc_seq(b"pngbytes", Some(80), Some(24));
        assert!(esc.starts_with("\x1b]1337;"));
        assert!(esc.ends_with('\x07'));
    }

    #[test]
    fn esc_seq_includes_size_width_and_height() {
        let esc = build_esc_seq(b"abc", Some(80), Some(23));
        assert!(esc.contains("size=3"));
        assert!(esc.contains("width=80"));
        assert!(esc.contains("height=23"));
        assert!(esc.contains("preserveAspectRatio=1"));
    }

    #[test]
    fn esc_seq_omits_axes_when_none() {
        let esc = build_esc_seq(b"x", None, None);
        assert!(!esc.contains("width="));
        assert!(!esc.contains("height="));
    }

    #[test]
    fn esc_seq_round_trips_payload() {
        let payload = b"\x00\x01\x02\xff\xfe sample png body";
        let esc = build_esc_seq(payload, None, None);
        assert_eq!(decode_payload(&esc), payload);
    }

    #[test]
    fn clear_screen_emits_csi_sequences() {
        let mut out = Vec::new();
        clear_screen(&mut out).unwrap();
        assert_eq!(out, b"\x1b[2J\x1b[H");
    }

    #[test]
    fn footer_formats_page_counter() {
        let mut out = Vec::new();
        write_footer(&mut out, 12, 120).unwrap();
        assert_eq!(out, b"\n[12/120] ");
    }

    #[test]
    fn build_rendered_page_reserves_footer_row() {
        let rp = build_rendered_page(
            b"PNG".to_vec(),
            (800, 600),
            Quality::High,
            1,
            24,
            80,
        );
        assert!(rp.esc_seq.contains("width=80"));
        assert!(rp.esc_seq.contains("height=23"));
        assert_eq!(rp.dims, (800, 600));
        assert_eq!(rp.quality, Quality::High);
    }

    #[test]
    fn build_rendered_page_clamps_tiny_terminal() {
        let rp = build_rendered_page(
            b"PNG".to_vec(),
            (100, 100),
            Quality::Low,
            1,
            1,
            1,
        );
        // With a 1-row terminal we still reserve at least 1 row for the image.
        assert!(rp.esc_seq.contains("height=1"));
        assert!(rp.esc_seq.contains("width=1"));
    }

    #[test]
    fn write_page_emits_clear_then_escape_then_footer() {
        let rp = build_rendered_page(
            b"PAYLOAD".to_vec(),
            (10, 10),
            Quality::High,
            1,
            10,
            80,
        );
        let mut out = Vec::new();
        write_page(&mut out, &rp, 2, 5).unwrap();
        let s = String::from_utf8_lossy(&out);
        assert!(s.starts_with("\x1b[2J\x1b[H"));
        assert!(s.contains("\x1b]1337;"));
        assert!(s.contains("[2/5]"));
    }
}
