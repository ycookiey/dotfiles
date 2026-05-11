// 高DPIスクショ + DOM計測値ダンプ。視覚と数値の不一致を検出する。

import { chromium } from "playwright";
import { writeFileSync } from "node:fs";
import { resolve } from "node:path";

const [, , url, selector, basename] = process.argv;

if (!url || !selector || !basename) {
    console.error("usage: node shoot.mjs <url> <selector> <output_basename>");
    process.exit(2);
}

const pngPath = resolve(`${basename}.png`);
const jsonPath = resolve(`${basename}.json`);

const browser = await chromium.launch();
// 等倍だと 1〜2px のズレが潰れる。3倍で文字エッジまで判別可能
const context = await browser.newContext({ deviceScaleFactor: 3 });
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
await page.screenshot({
    path: pngPath,
    clip: { x: rect.x, y: rect.y, width: rect.width, height: rect.height },
});

writeFileSync(jsonPath, JSON.stringify({ url, selector, ...measured }, null, 2));

await browser.close();

console.log(`PNG: ${pngPath}`);
console.log(`JSON: ${jsonPath}`);
