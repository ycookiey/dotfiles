// 高DPIスクショ + DOM計測値ダンプ。視覚と数値の不一致を検出する。

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
    return {
        rect: {
            x: rect.x, y: rect.y, width: rect.width, height: rect.height,
            top: rect.top, right: rect.right, bottom: rect.bottom, left: rect.left,
        },
        style,
        text: el.textContent?.trim().slice(0, 200) ?? null,
    };
}, selector);

if (!measured) {
    console.error(`selector not found: ${selector}`);
    await browser.close();
    process.exit(1);
}

const { rect } = measured;
writeFileSync(jsonPath, JSON.stringify({ url, selector, ...measured }, null, 2));

// 実ピクセル寸法 = CSS寸法 × scale。これが MAX_EDGE を超えると Read で開けない。
// 1枚に収まるなら従来通り単一PNG、超えるなら縦横グリッドに分割する。
// 縦分割だけでは横長要素(navbar等)の幅超過を救えないため両軸でタイル化する。
const tileCss = MAX_EDGE / SCALE; // タイル1辺のCSSサイズ
const cols = Math.max(1, Math.ceil((rect.width * SCALE) / MAX_EDGE));
const rows = Math.max(1, Math.ceil((rect.height * SCALE) / MAX_EDGE));
const pngPaths = [];

if (cols === 1 && rows === 1) {
    const pngPath = resolve(`${basename}.png`);
    await page.screenshot({
        path: pngPath,
        clip: { x: rect.x, y: rect.y, width: rect.width, height: rect.height },
    });
    pngPaths.push(pngPath);
} else {
    if (cols * rows > TILE_WARN) {
        console.error(
            `warn: 撮影範囲が ${Math.round(rect.width)}x${Math.round(rect.height)}px と広く ` +
            `${cols * rows} タイルに分割します。より具体的なセレクタの指定を推奨`,
        );
    }
    for (let r = 0; r < rows; r++) {
        for (let c = 0; c < cols; c++) {
            const x = rect.x + c * tileCss;
            const y = rect.y + r * tileCss;
            // 端のタイルは残り幅/高さに切り詰める
            const w = Math.min(tileCss, rect.right - x);
            const h = Math.min(tileCss, rect.bottom - y);
            if (w <= 0 || h <= 0) continue;
            const pngPath = resolve(`${basename}-r${r + 1}c${c + 1}.png`);
            await page.screenshot({ path: pngPath, clip: { x, y, width: w, height: h } });
            pngPaths.push(pngPath);
        }
    }
}

await browser.close();

// 分割時は左上→右下の順。タイル位置は r{行}c{列} で表現
for (const p of pngPaths) console.log(`PNG: ${p}`);
console.log(`JSON: ${jsonPath}`);
console.log(`TILES: ${rows}x${cols}`);
