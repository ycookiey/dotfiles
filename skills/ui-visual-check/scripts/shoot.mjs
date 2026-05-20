// 高DPIスクショ + DOM計測値ダンプ。視覚と数値の不一致を検出する。
// 素のスクショに加え、整列ガイド(端/中心の基準線 + 兄弟要素の枠)を注入した
// 装飾版も出力し、要素間のデザイン整合性を視覚で炙り出す。

import { chromium } from "playwright";
import { writeFileSync } from "node:fs";
import { resolve } from "node:path";

const [, , url, selector, basename] = process.argv;

if (!url || !selector || !basename) {
    console.error("usage: node shoot.mjs <url> <selector> <output_basename>");
    process.exit(2);
}

// 等倍だと 1〜2px のズレが潰れる。3倍で文字エッジまで判別可能
const SCALE = Number(process.env.UI_CHECK_SCALE) || 3;
// Read(画像)で開ける長辺の上限。これを超えると読込み不可になるため分割の閾値にする。
// API側の長辺リサイズ(約1568px)未満に収め、3xの精度を落とさない値
const MAX_EDGE = Number(process.env.UI_CHECK_MAX_EDGE) || 1500;
// タイル数がこれを超えたら、撮影範囲が広すぎる(セレクタが大雑把)とみなし警告
const TILE_WARN = 9;

const jsonPath = resolve(`${basename}.json`);

const browser = await chromium.launch();
const context = await browser.newContext({ deviceScaleFactor: SCALE });
const page = await context.newPage();

// SPA hydration後まで待つ。`load` だと描画前で計測値がブレる
await page.goto(url, { waitUntil: "networkidle" });

// rect は要素を内側に含む clip 矩形(x/y/width/height)に正規化して返す
function toClip(rect) {
    return { x: rect.x, y: rect.y, width: rect.width, height: rect.height };
}

// clip を 1枚 or 縦横グリッドで撮影。生成した PNG パス配列と行列数を返す。
// 実ピクセル寸法 = CSS寸法 × scale。これが MAX_EDGE を超えると Read で開けないため分割する。
// 縦分割だけでは横長要素(navbar等)の幅超過を救えないため両軸でタイル化する。
async function shootClip(outBase, clip) {
    const tileCss = MAX_EDGE / SCALE; // タイル1辺のCSSサイズ
    const cols = Math.max(1, Math.ceil((clip.width * SCALE) / MAX_EDGE));
    const rows = Math.max(1, Math.ceil((clip.height * SCALE) / MAX_EDGE));
    const paths = [];
    const right = clip.x + clip.width;
    const bottom = clip.y + clip.height;

    if (cols === 1 && rows === 1) {
        const p = resolve(`${outBase}.png`);
        await page.screenshot({ path: p, clip });
        paths.push(p);
    } else {
        if (cols * rows > TILE_WARN) {
            console.error(
                `warn: 撮影範囲が ${Math.round(clip.width)}x${Math.round(clip.height)}px と広く ` +
                `${cols * rows} タイルに分割します。より具体的なセレクタの指定を推奨`,
            );
        }
        for (let r = 0; r < rows; r++) {
            for (let c = 0; c < cols; c++) {
                const x = clip.x + c * tileCss;
                const y = clip.y + r * tileCss;
                // 端のタイルは残り幅/高さに切り詰める
                const w = Math.min(tileCss, right - x);
                const h = Math.min(tileCss, bottom - y);
                if (w <= 0 || h <= 0) continue;
                const p = resolve(`${outBase}-r${r + 1}c${c + 1}.png`);
                await page.screenshot({ path: p, clip: { x, y, width: w, height: h } });
                paths.push(p);
            }
        }
    }
    return { paths, rows, cols };
}

// 計測 + 兄弟要素の整列情報を収集（ガイド注入はまだしない＝素版を先に撮るため）
const measured = await page.evaluate((sel) => {
    const el = document.querySelector(sel);
    if (!el) return null;
    const rect = el.getBoundingClientRect();
    const cs = getComputedStyle(el);
    // 検証で頻繁に参照する代表値のみ抽出（全プロパティはノイズになる）
    const pick = [
        "fontSize", "fontFamily", "fontWeight", "lineHeight", "letterSpacing",
        "color", "backgroundColor",
        "width", "height",
        "marginTop", "marginRight", "marginBottom", "marginLeft",
        "paddingTop", "paddingRight", "paddingBottom", "paddingLeft",
        "borderTopWidth", "borderRightWidth", "borderBottomWidth", "borderLeftWidth",
        "display", "position", "alignItems", "justifyContent", "textAlign",
    ];
    const style = Object.fromEntries(pick.map((k) => [k, cs[k]]));

    const r2 = (n) => Math.round(n * 100) / 100;
    const edges = (el2) => {
        const r = el2.getBoundingClientRect();
        return {
            tag: el2.tagName.toLowerCase(),
            left: r2(r.left), right: r2(r.right), centerX: r2(r.left + r.width / 2),
            top: r2(r.top), bottom: r2(r.bottom), centerY: r2(r.top + r.height / 2),
            width: r2(r.width), height: r2(r.height),
        };
    };

    // 整合性チェックの本丸＝兄弟要素との揃い。親の直接の子(自身含む)の端/中心を数値で並べる。
    const parent = el.parentElement;
    const siblings = parent
        ? Array.from(parent.children)
            .filter((c) => { const r = c.getBoundingClientRect(); return r.width > 0 && r.height > 0; })
            .map((c) => ({ ...edges(c), self: c === el }))
        : [];

    return {
        rect: {
            x: rect.x, y: rect.y, width: rect.width, height: rect.height,
            top: rect.top, right: rect.right, bottom: rect.bottom, left: rect.left,
        },
        style,
        text: el.textContent?.trim().slice(0, 200) ?? null,
        siblings,
        parentRect: parent
            ? (() => { const p = parent.getBoundingClientRect(); return { x: p.x, y: p.y, width: p.width, height: p.height }; })()
            : null,
        viewport: { width: window.innerWidth, height: window.innerHeight },
    };
}, selector);

if (!measured) {
    console.error(`selector not found: ${selector}`);
    await browser.close();
    process.exit(1);
}

writeFileSync(jsonPath, JSON.stringify({ url, selector, ...measured }, null, 2));

// 1) 素版: 対象要素そのものを撮る（実際の見た目を保持）
const plain = await shootClip(basename, toClip(measured.rect));

// 2) ガイド版: 端/中心の基準線 + 兄弟枠を注入して撮る
//    対象の左端/中心ライン上に兄弟の端が乗っているか＝整列を一目で検証できる。
await page.evaluate((sel) => {
    const el = document.querySelector(sel);
    const rect = el.getBoundingClientRect();
    const vw = window.innerWidth, vh = window.innerHeight;

    // overlay は position:fixed でビューポート基準にし、getBoundingClientRect 値をそのまま使う
    const layer = document.createElement("div");
    layer.id = "__ui_check_guides__";
    layer.style.cssText = "position:fixed;inset:0;z-index:2147483647;pointer-events:none;margin:0;";
    document.body.appendChild(layer);
    const add = (css) => {
        const d = document.createElement("div");
        d.style.cssText = "position:fixed;pointer-events:none;box-sizing:border-box;" + css;
        layer.appendChild(d);
    };

    // 兄弟要素の枠（薄青）。基準線とエッジが一致するか目視する対象
    const parent = el.parentElement;
    if (parent) {
        for (const sib of parent.children) {
            const r = sib.getBoundingClientRect();
            if (r.width === 0 || r.height === 0) continue;
            add(`left:${r.left}px;top:${r.top}px;width:${r.width}px;height:${r.height}px;outline:1px solid rgba(0,128,255,0.55);`);
        }
    }

    // 対象要素の基準線。端=赤、中心=橙。ページ全幅/全高に延ばす
    const RED = "rgba(255,0,0,0.7)", ORG = "rgba(255,140,0,0.75)";
    const vline = (x, c) => add(`left:${x}px;top:0;width:1px;height:${vh}px;background:${c};`);
    const hline = (y, c) => add(`top:${y}px;left:0;height:1px;width:${vw}px;background:${c};`);
    vline(rect.left, RED);
    vline(rect.right, RED);
    vline(rect.left + rect.width / 2, ORG);
    hline(rect.top, RED);
    hline(rect.bottom, RED);
    hline(rect.top + rect.height / 2, ORG);
}, selector);

// ガイド版の撮影範囲: 兄弟群が入る親rectをビューポートと交差させる（巨大親でのタイル爆発を抑制）。
// 親が無い等で取れなければ対象rectにフォールバック。
let guideClip;
if (measured.parentRect && measured.parentRect.width > 0 && measured.parentRect.height > 0) {
    const p = measured.parentRect;
    const vp = measured.viewport;
    const x = Math.max(0, p.x), y = Math.max(0, p.y);
    const w = Math.min(vp.width, p.x + p.width) - x;
    const h = Math.min(vp.height, p.y + p.height) - y;
    guideClip = (w > 0 && h > 0) ? { x, y, width: w, height: h } : toClip(measured.rect);
} else {
    guideClip = toClip(measured.rect);
}
const guide = await shootClip(`${basename}-guides`, guideClip);

await browser.close();

// 分割時は左上→右下の順。タイル位置は r{行}c{列} で表現
for (const p of plain.paths) console.log(`PNG: ${p}`);
for (const p of guide.paths) console.log(`GUIDE: ${p}`);
console.log(`JSON: ${jsonPath}`);
console.log(`TILES: ${plain.rows}x${plain.cols}`);
console.log(`GUIDE_TILES: ${guide.rows}x${guide.cols}`);
