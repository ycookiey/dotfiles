use std::io::Write;
use std::sync::Arc;

use anyhow::Result;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;

use crate::cache::Quality;

/// フッターをどこに描画するか。縦長 PDF では画像の右側に余白が
/// 出るのでそちらにフッターを置けば画像高さを画面いっぱいに使える。
/// 横長 PDF では画像下に余白が出るので従来どおり最下行に置く。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FooterPlacement {
    /// 画面最下行 (列 1)。画像は `total_rows - footer_rows` 行で描画。
    Bottom,
    /// 最下行の指定列 (1-indexed)。画像は `total_rows` 行いっぱいで描画。
    RightOf { col: u16 },
}

#[derive(Debug, Clone)]
pub struct RenderedPage {
    pub png_bytes: Arc<Vec<u8>>,
    pub esc_seq: Arc<String>,
    pub dims: (u32, u32),
    pub quality: Quality,
    pub footer_placement: FooterPlacement,
    /// 画像が画面上で占有するセル幅 (1-indexed の最右列までの数)。
    /// 画像は (1,1) から描画される前提。差分 erase で旧画像との差を消す。
    pub image_cells_w: u16,
    /// 画像が画面上で占有するセル高さ (1-indexed の最下行までの数)。
    pub image_cells_h: u16,
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
    // doNotMoveCursor=1: wezterm 拡張。画像描画後にカーソルを移動させない。
    // これを指定しないと、画像下端が画面最終行に達したときに wezterm が
    // 画面を 1 行スクロールしてしまい、画像上端が画面外へ追い出されて
    // 「上が見切れる」状態になる (wezterm/wezterm#3266)。
    let mut params = format!("File=inline=1;doNotMoveCursor=1;size={}", png.len());
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

/// 1 行分のフッター文字列が確実に収まる文字数の見積もり。
/// `[<page>/<total>] ` 形式で page/total の桁が大きくなっても 16 列あれば足りる。
const FOOTER_RESERVE_COLS: u16 = 16;

pub fn build_rendered_page(
    png_bytes: Vec<u8>,
    dims: (u32, u32),
    quality: Quality,
    footer_rows: u16,
    total_rows: u16,
    total_cols: u16,
    cell_w_px: u32,
    cell_h_px: u32,
) -> RenderedPage {
    // 画面表示エリア全体のアスペクト比 (横/縦, px単位)。
    // 画像比がこれより小さい(=より縦長)なら縦が制約となり横に余白が出る。
    let screen_aspect = if total_rows == 0 || cell_h_px == 0 {
        1.0
    } else {
        (u32::from(total_cols) * cell_w_px) as f32 / (u32::from(total_rows) * cell_h_px) as f32
    };
    let image_aspect = if dims.1 == 0 {
        1.0
    } else {
        dims.0 as f32 / dims.1 as f32
    };

    // height のみ指定。wezterm では width+height 両指定の preserveAspectRatio=1 が
    // box への letterbox として動かず、片軸 fit + アスペクト比保持で他方が box を
    // 超え画面下端まで溢れる (上は doNotMoveCursor=1 で抑止済み)。height だけなら
    // 縦は指定行数に収まり、横はアスペクト比から weztermが自動算出する。
    let (cell_height, footer_placement) = if image_aspect < screen_aspect {
        // 縦長画像: 画面全高を画像に使い、フッターを画像右の余白に置く。
        let h = total_rows.max(1);
        let image_w_cells = if dims.1 == 0 || cell_w_px == 0 {
            total_cols
        } else {
            let raw =
                (dims.0 as f32 * h as f32 * cell_h_px as f32) / (dims.1 as f32 * cell_w_px as f32);
            raw.ceil().min(total_cols as f32) as u16
        };
        // フッター文字列が右余白に収まるか確認。収まらなければ Bottom にフォールバック。
        let right_margin = total_cols.saturating_sub(image_w_cells);
        if right_margin >= FOOTER_RESERVE_COLS {
            // 画像右隣を 1 列あけて配置。
            let col = image_w_cells.saturating_add(2).min(total_cols.max(1));
            (h, FooterPlacement::RightOf { col })
        } else {
            let h_bottom = total_rows.saturating_sub(footer_rows).max(1);
            (h_bottom, FooterPlacement::Bottom)
        }
    } else {
        // 横長画像: 縦が制約され下に余白。フッターは最下行に置く。
        let h = total_rows.saturating_sub(footer_rows).max(1);
        (h, FooterPlacement::Bottom)
    };

    // height だけ指定なので画像の実 cell 横幅はアスペクト比から自動算出。
    // 差分 erase のため事前に占有セル幅を計算しておく。
    let image_cells_w = if dims.1 == 0 || cell_w_px == 0 {
        total_cols
    } else {
        let raw = (dims.0 as f32 * cell_height as f32 * cell_h_px as f32)
            / (dims.1 as f32 * cell_w_px as f32);
        raw.ceil().min(total_cols as f32) as u16
    };
    // width も明示指定する。width 未指定だと wezterm は box幅 = total_cols と
    // 解釈し、画像実描画領域外の右セルも「画像セル」属性として確保するため、
    // そこに書いたショートカットガイドが画像で覆われて見えなくなる。
    // box を画像比に合わせて (image_cells_w, cell_height) に限定すれば
    // box 外のセルは通常テキスト領域として扱われガイドが表示される。
    // box 比が画像比とほぼ一致するため preserveAspectRatio=1 でも縦が
    // 画面を超えることは起きない。
    let esc = build_esc_seq(&png_bytes, Some(image_cells_w), Some(cell_height));
    tracing::info!(
        png_dims_w = dims.0,
        png_dims_h = dims.1,
        ?quality,
        footer_rows,
        total_rows,
        total_cols,
        image_aspect,
        screen_aspect,
        ?footer_placement,
        esc_cell_h = cell_height,
        image_cells_w,
        "build_rendered_page"
    );
    RenderedPage {
        png_bytes: Arc::new(png_bytes),
        esc_seq: Arc::new(esc),
        dims,
        quality,
        footer_placement,
        image_cells_w,
        image_cells_h: cell_height,
    }
}

pub fn clear_screen<W: Write>(out: &mut W) -> Result<()> {
    // ESC[2J clears the visible screen; ESC[3J drops the scrollback
    // buffer as well so inline images that drifted off-screen during a
    // pane zoom don't resurface. ESC[H moves the cursor home. Using
    // raw escapes (instead of crossterm::execute!) keeps this module
    // dependency-light and easy to unit-test.
    out.write_all(b"\x1b[3J\x1b[2J\x1b[H")?;
    Ok(())
}

pub fn write_page<W: Write>(
    out: &mut W,
    rendered: &RenderedPage,
    previous: Option<&RenderedPage>,
    page: u32,
    total: u32,
    total_rows: u16,
) -> Result<()> {
    // clear_screen は呼ばない: ターミナルが画面を一度 blank にしてから
    // 画像を描画しなおすと、blank 〜 decode 完了の間が点滅として知覚される。
    // 代わりに前回 RenderedPage との差分セルだけ erase してから新画像で
    // 上書きする。初回描画 (previous=None) は呼び出し側で clear する想定。

    // 1) 旧画像が新画像より右に伸びていた場合: その差分列を画像の高さ分 erase。
    // 2) 旧画像が新画像より下に伸びていた場合: その差分行を画面幅分 erase。
    if let Some(prev) = previous {
        if prev.image_cells_w > rendered.image_cells_w {
            let start_col = rendered.image_cells_w.saturating_add(1).max(1);
            let n_cols = prev.image_cells_w - rendered.image_cells_w;
            let max_row = prev.image_cells_h.min(total_rows);
            for row in 1..=max_row {
                // CSI <n>X (ECH): 現在位置から n セルを空白に置換。
                write!(out, "\x1b[{row};{start_col}H\x1b[{n_cols}X")?;
            }
        }
        if prev.image_cells_h > rendered.image_cells_h {
            let start_row = rendered.image_cells_h.saturating_add(1).max(1);
            let end_row = prev.image_cells_h.min(total_rows);
            for row in start_row..=end_row {
                write!(out, "\x1b[{row};1H\x1b[K")?;
            }
        }
    }

    // 3) 画像描画。doNotMoveCursor=1 によりカーソルは (1,1) のまま。
    out.write_all(b"\x1b[1;1H")?;
    out.write_all(rendered.esc_seq.as_bytes())?;

    // 4) ガイド描画。RightOf 配置時は画像右余白に縦並びで、
    //    Bottom 配置時はフッター行に inline で表示する。
    //    フッター行への描画前に行うことで、Bottom時は同じ行に追記される。
    let total_cols_safe = if total_rows == 0 { 0 } else { rendered.image_cells_w };
    let _ = total_cols_safe; // 互換: 後続で `image_cells_w` を直接参照する。

    // 5) フッター行を 1 度 erase line してから新フッター描画。
    //    旧 footer の placement が違っても同じ最下行にいるためこれで一括カバー。
    let row = total_rows.max(1);
    write!(out, "\x1b[{row};1H\x1b[K")?;
    let col = match rendered.footer_placement {
        FooterPlacement::Bottom => 1,
        FooterPlacement::RightOf { col } => col.max(1),
    };
    write!(out, "\x1b[{row};{col}H")?;
    write_footer(out, page, total)?;

    // 6) ガイド描画。
    write_shortcut_guide(out, rendered, total_rows)?;

    out.flush()?;
    Ok(())
}

/// 各ガイド行は短い "key  desc" の固定文字列。
/// RightOf 配置の縦並びで使い、Bottom 配置時は inline 連結用に別途定数を使う。
const SHORTCUT_GUIDE_LINES: &[&str] = &[
    "[Keys]",
    "n/j  Next",
    "p/k  Prev",
    "gg   First",
    "G    Last",
    "<N>g Goto",
    "q    Quit",
];


/// Bottom 配置時にフッター行右側に追記する 1 行ガイド。
const SHORTCUT_GUIDE_INLINE: &str =
    "  n/j:Next p/k:Prev gg:First G:Last <N>g:Goto q:Quit";

fn write_shortcut_guide<W: Write>(
    out: &mut W,
    rendered: &RenderedPage,
    total_rows: u16,
) -> Result<()> {
    match rendered.footer_placement {
        FooterPlacement::Bottom => {
            // フッター行の残り幅にガイドを inline 追記。
            // 直前で `[<page>/<total>] ` を書いた後にカーソルがある想定。
            // 残り幅の判定は省略 (ターミナル側で wrap される懸念は薄く、
            // 多くの場合フッター + ガイドは画面幅 (>=64) に収まる)。
            out.write_all(SHORTCUT_GUIDE_INLINE.as_bytes())?;
        }
        FooterPlacement::RightOf { .. } => {
            // 画像右余白の最上行から下方へガイドを並べる。
            // 開始列 = 画像 cell 幅 + 2 (1 列空けて余白)。
            let start_col = rendered.image_cells_w.saturating_add(2).max(1);
            // 余白幅 (画像右隣からターミナル右端まで)。
            // `total_cols` は write_page に渡されていないため、ガイド行の長さ
            // 自体で抑える (各行 12 文字以内に収まるよう設計)。
            let footer_row = total_rows.max(1);
            for (i, line) in SHORTCUT_GUIDE_LINES.iter().enumerate() {
                let row = (i as u16).saturating_add(1);
                if row >= footer_row {
                    break; // フッター行と被らないよう手前で停止。
                }
                // start_col から行末まで erase line してから書く。
                // Low→High で画像幅が 1〜2 セル動くと前回ガイドの一部が
                // ECH の固定幅では消しきれずに残るので、行末まで一括で消去する。
                write!(out, "\x1b[{row};{start_col}H\x1b[K{line}")?;
            }
        }
    }
    Ok(())
}

pub fn write_footer<W: Write>(out: &mut W, page: u32, total: u32) -> Result<()> {
    write!(out, "[{page}/{total}] ")?;
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
        assert_eq!(out, b"\x1b[3J\x1b[2J\x1b[H");
    }

    #[test]
    fn footer_formats_page_counter() {
        let mut out = Vec::new();
        write_footer(&mut out, 12, 120).unwrap();
        assert_eq!(out, b"[12/120] ");
    }

    #[test]
    fn build_rendered_page_landscape_uses_bottom_footer() {
        // 横長画像 (3200, 600, 比 5.33) vs 画面比 80*8/(24*16) = 1.667
        // 画像比 > 画面比 → 横長判定 → Bottom 配置。
        let rp = build_rendered_page(
            b"PNG".to_vec(),
            (3200, 600),
            Quality::High,
            1,
            24,
            80,
            8,
            16,
        );
        assert!(rp.esc_seq.contains("width="));
        assert!(rp.esc_seq.contains("height=23"));
        assert_eq!(rp.footer_placement, FooterPlacement::Bottom);
    }

    #[test]
    fn build_rendered_page_portrait_uses_right_footer() {
        // 縦長画像 (400, 800, 比 0.5) vs 画面比 80*8/(24*16) = 1.667
        // 画像比 < 画面比 → 縦が制約 → 右余白あり → RightOf
        let rp = build_rendered_page(
            b"PNG".to_vec(),
            (400, 800),
            Quality::High,
            1,
            24,
            80,
            8,
            16,
        );
        assert!(rp.esc_seq.contains("height=24"));
        match rp.footer_placement {
            FooterPlacement::RightOf { col } => assert!(col > 1 && col <= 80),
            other => panic!("expected RightOf, got {other:?}"),
        }
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
            8,
            16,
        );
        assert!(rp.esc_seq.contains("height=1"));
        assert!(rp.esc_seq.contains("width="));
    }

    #[test]
    fn write_page_writes_image_and_footer_without_clear() {
        // 画像比 (3200/600=5.33) > 画面比 (80*8/10*16=4.0) → Bottom 配置
        let rp = build_rendered_page(
            b"PAYLOAD".to_vec(),
            (3200, 600),
            Quality::High,
            1,
            10,
            80,
            8,
            16,
        );
        let mut out = Vec::new();
        write_page(&mut out, &rp, None, 2, 5, 10).unwrap();
        let s = String::from_utf8_lossy(&out);
        // clear_screen は呼ばない (差分 erase で点滅回避するため)。
        assert!(!s.contains("\x1b[2J"));
        assert!(s.contains("\x1b]1337;"));
        // フッター行 erase + フッター描画
        assert!(s.contains("\x1b[10;1H\x1b[K"));
        assert!(s.contains("[2/5]"));
    }

    #[test]
    fn write_page_erases_diff_cells_when_image_shrinks() {
        // 前ページが大きく、新ページが小さい場合、差分セルが erase される。
        let prev = build_rendered_page(
            b"PREV".to_vec(),
            (3200, 600),
            Quality::High,
            1,
            10,
            80,
            8,
            16,
        );
        let next = build_rendered_page(
            b"NEXT".to_vec(),
            (1600, 600),
            Quality::High,
            1,
            10,
            80,
            8,
            16,
        );
        // 念のため前提確認: prev の方が横に広い。
        assert!(prev.image_cells_w > next.image_cells_w);
        let mut out = Vec::new();
        write_page(&mut out, &next, Some(&prev), 2, 5, 10).unwrap();
        let s = String::from_utf8_lossy(&out);
        // ECH (`\x1b[<n>X`) で差分セル消去。
        assert!(s.contains("X"));
        assert!(s.contains("\x1b]1337;"));
    }
}
