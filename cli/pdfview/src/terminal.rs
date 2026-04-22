use std::io::{Read, Write};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalDims {
    pub cols: u16,
    pub rows: u16,
    pub px_w: u32,
    pub px_h: u32,
    pub cell_w: u32,
    pub cell_h: u32,
    pub is_fallback: bool,
}

impl TerminalDims {
    pub fn fallback(cols: u16, rows: u16) -> Self {
        const DEFAULT_CELL_W: u32 = 8;
        const DEFAULT_CELL_H: u32 = 16;
        Self {
            cols,
            rows,
            px_w: cols as u32 * DEFAULT_CELL_W,
            px_h: rows as u32 * DEFAULT_CELL_H,
            cell_w: DEFAULT_CELL_W,
            cell_h: DEFAULT_CELL_H,
            is_fallback: true,
        }
    }
}

/// Parse a CSI report of the form `ESC [ <kind> ; <a> ; <b> t`.
/// Returns (a, b) on success.
pub fn parse_csi_report(bytes: &[u8], expected_kind: u16) -> Option<(u32, u32)> {
    // Accept optional leading garbage before ESC, but require the full sequence.
    let esc_idx = bytes.iter().position(|&b| b == 0x1b)?;
    let rest = &bytes[esc_idx + 1..];
    // After ESC we expect '[' then kind ';' a ';' b 't'
    let (bracket, rest) = rest.split_first()?;
    if *bracket != b'[' {
        return None;
    }
    let t_idx = rest.iter().position(|&b| b == b't')?;
    let body = std::str::from_utf8(&rest[..t_idx]).ok()?;
    let mut parts = body.split(';');
    let kind: u16 = parts.next()?.parse().ok()?;
    if kind != expected_kind {
        return None;
    }
    let a: u32 = parts.next()?.parse().ok()?;
    let b: u32 = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((a, b))
}

/// Query a CSI report by writing `request` to stdout and reading the reply
/// from stdin until `t` or the timeout elapses. The terminal is expected to
/// already be in raw mode.
fn query_csi(request: &[u8], timeout: Duration) -> Result<Vec<u8>> {
    {
        let mut out = std::io::stdout().lock();
        out.write_all(request)
            .context("write CSI request to stdout")?;
        out.flush().context("flush stdout")?;
    }

    let (tx, rx) = mpsc::channel::<std::io::Result<Vec<u8>>>();
    thread::spawn(move || {
        let mut buf = Vec::with_capacity(64);
        let mut byte = [0u8; 1];
        let mut stdin = std::io::stdin().lock();
        loop {
            match stdin.read(&mut byte) {
                Ok(0) => break,
                Ok(_) => {
                    buf.push(byte[0]);
                    if byte[0] == b't' {
                        break;
                    }
                    if buf.len() > 128 {
                        break;
                    }
                }
                Err(e) => {
                    let _ = tx.send(Err(e));
                    return;
                }
            }
        }
        let _ = tx.send(Ok(buf));
    });

    match rx.recv_timeout(timeout) {
        Ok(Ok(buf)) => Ok(buf),
        Ok(Err(e)) => Err(e.into()),
        Err(_) => Err(anyhow::anyhow!("csi query timed out")),
    }
}

/// Query terminal dimensions, returning the fallback value if CSI reports
/// are unavailable. Puts the terminal into raw mode for the duration of the
/// queries.
pub fn query_dims() -> Result<TerminalDims> {
    let (cols, rows) = crossterm::terminal::size().context("get terminal size")?;

    crossterm::terminal::enable_raw_mode().context("enable raw mode")?;
    let result = query_dims_raw(cols, rows);
    let _ = crossterm::terminal::disable_raw_mode();
    Ok(result)
}

fn query_dims_raw(cols: u16, rows: u16) -> TerminalDims {
    let timeout = Duration::from_millis(150);

    let pixel = query_csi(b"\x1b[14t", timeout)
        .ok()
        .and_then(|buf| parse_csi_report(&buf, 4));
    let cell = query_csi(b"\x1b[16t", timeout)
        .ok()
        .and_then(|buf| parse_csi_report(&buf, 6));

    match (pixel, cell) {
        (Some((ph, pw)), Some((ch, cw))) => TerminalDims {
            cols,
            rows,
            px_w: pw,
            px_h: ph,
            cell_w: cw,
            cell_h: ch,
            is_fallback: false,
        },
        _ => TerminalDims::fallback(cols, rows),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_csi_14t_window_pixels() {
        // Window pixel size reply: kind=4, height=1080, width=1920
        let reply = b"\x1b[4;1080;1920t";
        assert_eq!(parse_csi_report(reply, 4), Some((1080, 1920)));
    }

    #[test]
    fn parse_csi_16t_cell_pixels() {
        // Cell pixel size reply: kind=6, cell_height=16, cell_width=8
        let reply = b"\x1b[6;16;8t";
        assert_eq!(parse_csi_report(reply, 6), Some((16, 8)));
    }

    #[test]
    fn parse_rejects_wrong_kind() {
        let reply = b"\x1b[4;1080;1920t";
        assert_eq!(parse_csi_report(reply, 6), None);
    }

    #[test]
    fn parse_rejects_truncated() {
        assert_eq!(parse_csi_report(b"\x1b[4;1080;1920", 4), None);
        assert_eq!(parse_csi_report(b"\x1b[4;1080t", 4), None);
    }

    #[test]
    fn parse_tolerates_leading_noise() {
        let reply = b"garbage\x1b[6;16;8t";
        assert_eq!(parse_csi_report(reply, 6), Some((16, 8)));
    }

    #[test]
    fn parse_rejects_extra_fields() {
        let reply = b"\x1b[4;1080;1920;99t";
        assert_eq!(parse_csi_report(reply, 4), None);
    }

    #[test]
    fn parse_rejects_non_numeric() {
        let reply = b"\x1b[4;abc;1920t";
        assert_eq!(parse_csi_report(reply, 4), None);
    }

    #[test]
    fn fallback_uses_8x16_cell() {
        let d = TerminalDims::fallback(80, 24);
        assert_eq!(d.cols, 80);
        assert_eq!(d.rows, 24);
        assert_eq!(d.cell_w, 8);
        assert_eq!(d.cell_h, 16);
        assert_eq!(d.px_w, 640);
        assert_eq!(d.px_h, 384);
        assert!(d.is_fallback);
    }
}
