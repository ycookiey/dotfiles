// The comparison page: items with their notes, and each shot's changed bands
// shown before or after in one frame, with the changed regions boxed in red.

const esc = (s) => String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;")
/** Escapes text and turns `code` spans into <code>. */
const md = (s) => esc(s).replace(/`([^`]+)`/g, "<code>$1</code>")

const DEFAULT_TEXT = {
  before: "変更前",
  after: "変更後",
  side: "表示",
  width: "画像の幅",
  keys: "← → か 1 2 で切り替え。画像を押しても切り替わる",
  noDiff: "変更前と変更後で違いはない",
  onlyAfter: "この場面は変更後にしか起きない",
  onlyBefore: "この場面は変更前にしかない",
  full: "画面全体を開く",
  changes: (n) => `変わった所 ${n} か所`,
  summary: ["項目", "内容", "種類", "推奨"],
  now: "今",
  afterChange: "変更後",
  rec: "推奨",
}

const boxes = (list, side) =>
  list
    .map(
      (b) =>
        `<span class="box side-${side}" style="left:${b.left.toFixed(3)}%;top:${b.top.toFixed(3)}%;width:${b.width.toFixed(3)}%;height:${b.height.toFixed(3)}%"></span>`
    )
    .join("")

const stage = ({ pair, caption, t, tall }) => `
      <div class="stage${tall ? " tall" : ""}" role="button" tabindex="0" aria-label="${esc(caption)}: ${esc(t.before)}と${esc(t.after)}を切り替え">
        <div class="layer">
          <img class="side-before" alt="${esc(caption)}(${esc(t.before)})" src="${pair.before.src}">
          <img class="side-after" alt="${esc(caption)}(${esc(t.after)})" src="${pair.after.src}">
          ${boxes(pair.before.boxes, "before")}${boxes(pair.after.boxes, "after")}
        </div>
        <span class="tag tag-before">${esc(t.before)}</span><span class="tag tag-after">${esc(t.after)}</span>
      </div>`

const view = (v, caption, t) => {
  const cls = `view w-${esc(v.width)}`
  if (v.only !== undefined) {
    return `<div class="${cls}"><p class="note">${esc(v.only === "after" ? t.onlyAfter : t.onlyBefore)}</p>
      <div class="still"><img alt="${esc(caption)}" src="${v.full}"></div></div>`
  }
  const head =
    v.blocks.length === 0
      ? `<p class="note">${esc(t.noDiff)}</p>`
      : `<p class="count">${esc(t.changes(v.bands.length))}</p>`
  const bands = v.bands.map((pair) => stage({ pair, caption, t })).join("")
  return `<div class="${cls}">${head}${bands}
      <details${v.blocks.length === 0 ? " open" : ""}><summary>${esc(t.full)}</summary>${stage({
        pair: v.full,
        caption,
        t,
        tall: true,
      })}</details></div>`
}

const section = (g, t) => {
  const pairs = [
    g.now !== undefined ? `<div><dt>${esc(t.now)}</dt><dd>${md(g.now)}</dd></div>` : "",
    g.after !== undefined ? `<div><dt>${esc(t.afterChange)}</dt><dd>${md(g.after)}</dd></div>` : "",
  ].join("")
  return `
<section id="${esc(g.id)}">
  <header class="sec-head">
    <p class="eyebrow">${esc(g.label)}</p>
    <h2>${esc(g.title)}</h2>
    ${g.where !== undefined ? `<p class="where">${md(g.where)}</p>` : ""}
  </header>
  ${pairs !== "" ? `<dl class="ba">${pairs}</dl>` : ""}
  ${g.facts?.length ? `<ul class="facts">${g.facts.map((f) => `<li>${md(f)}</li>`).join("")}</ul>` : ""}
  ${g.rec !== undefined ? `<p class="rec"><span>${esc(t.rec)}</span>${md(g.rec)}</p>` : ""}
  ${g.shots
    .map(
      (s) => `<div class="shot"><h3>${esc(s.caption)}</h3>${s.note !== undefined ? `<p class="where">${md(s.note)}</p>` : ""}${s.views
        .map((v) => view(v, s.caption, t))
        .join("")}</div>`
    )
    .join("")}
</section>`
}

export const pageHtml = ({ config, groups, widths, widthConfig }) => {
  const t = { ...DEFAULT_TEXT, ...(config.text ?? {}) }
  const storageKey = config.storageKey ?? "ui-before-after"
  const firstWidth = config.defaultWidth ?? widths[0] ?? ""
  const summary =
    config.summary === false
      ? ""
      : `<div class="tablewrap"><table><thead><tr>${t.summary.map((h) => `<th>${esc(h)}</th>`).join("")}</tr></thead><tbody>${groups
          .map(
            (g) =>
              `<tr><td><a href="#${esc(g.id)}">${esc(g.label)}</a></td><td>${esc(g.title)}</td><td>${g.kind !== undefined ? `<span class="chip">${esc(g.kind)}</span>` : ""}</td><td>${g.rec !== undefined ? md(String(g.rec).split("。")[0]) : ""}</td></tr>`
          )
          .join("")}</tbody></table></div>`
  const widthButtons = widths
    .map((w) => `<button type="button" class="seg-btn" data-width="${esc(w)}" aria-pressed="false">${esc(widthConfig[w]?.label ?? w)}</button>`)
    .join("")
  const hideOtherWidths = widths
    .map((w) => `body[data-width="${w}"] .view:not(.w-${w}) { display: none; }`)
    .join("\n")
  const narrow = widths
    .filter((w) => widthConfig[w]?.frameMaxWidth !== undefined)
    .map((w) => `body[data-width="${w}"] .stage, body[data-width="${w}"] .still { max-width: ${Number(widthConfig[w].frameMaxWidth)}px; }`)
    .join("\n")
  return `<title>${esc(config.title ?? "変更前後の比較")}</title>
<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=BIZ+UDPGothic:wght@400;700&family=IBM+Plex+Mono:wght@400;500&display=swap">
<style>
/* Layout: one reading column; each item lists its notes, then each shot's changed bands in one frame that flips between before and after. */
:root {
  --bg: #f6f7f9; --panel: #ffffff; --fg: #1b2230; --muted: #5a6475; --line: #dde1e8;
  --accent: #2553b5; --accent-soft: #e6ecf8; --before: #5b4a1e; --before-soft: #fff1cc; --box: #e0242b;
  --font-body: "BIZ UDPGothic", "Hiragino Sans", "Yu Gothic UI", system-ui, sans-serif;
  --font-mono: "IBM Plex Mono", ui-monospace, Consolas, monospace;
}
@media (prefers-color-scheme: dark) { :root:not([data-theme="light"]) {
  --bg: #12161d; --panel: #1a2029; --fg: #e5e9f0; --muted: #9aa4b5; --line: #2c3442;
  --accent: #8fb0ff; --accent-soft: #1f2a40; --before: #f3d58a; --before-soft: #3a3016; --box: #ff5a5f; color-scheme: dark } }
:root[data-theme="dark"] {
  --bg: #12161d; --panel: #1a2029; --fg: #e5e9f0; --muted: #9aa4b5; --line: #2c3442;
  --accent: #8fb0ff; --accent-soft: #1f2a40; --before: #f3d58a; --before-soft: #3a3016; --box: #ff5a5f; color-scheme: dark }
body { background: var(--bg); color: var(--fg); font-family: var(--font-body); font-size: 15px; line-height: 1.75; }
.wrap { max-width: 1120px; margin: 0 auto; padding-inline: 16px; padding-block: 24px 64px; display: grid; gap: 40px; }
h1 { font-size: 1.6rem; line-height: 1.3; margin: 0; text-wrap: balance; }
h2 { font-size: 1.25rem; line-height: 1.4; margin: 0; text-wrap: balance; }
h3 { font-size: .95rem; margin: 0; }
p { margin: 0; }
code { font-family: var(--font-mono); font-size: .88em; background: var(--accent-soft); padding: 0 .3em; border-radius: 3px; }
a { color: var(--accent); }
a:focus-visible, button:focus-visible, .stage:focus-visible, summary:focus-visible { outline: 3px solid var(--accent); outline-offset: 2px; }
.intro { display: grid; gap: 12px; max-width: 70ch; }
.intro .muted, .note, .count, .hint { color: var(--muted); }
.toolbar { position: sticky; top: env(safe-area-inset-top, 0px); z-index: 3; background: var(--bg); padding-block: 8px; border-bottom: 1px solid var(--line); display: flex; flex-wrap: wrap; gap: 8px 16px; align-items: center; }
.toolbar .label { font-size: .85rem; color: var(--muted); letter-spacing: .04em; }
.hint { font-size: .8rem; flex-basis: 100%; }
.seg { display: inline-flex; border: 1px solid var(--line); border-radius: 6px; overflow: hidden; }
.seg-btn { border: 0; padding: 4px 14px; cursor: pointer; font: inherit; font-size: .9rem; background: var(--panel); color: var(--fg); }
.seg-btn[aria-pressed="true"] { background: var(--accent); color: var(--panel); font-weight: 700; }
.seg-btn[data-side="before"][aria-pressed="true"] { background: var(--before); color: var(--panel); }
.tablewrap { overflow-x: auto; }
table { border-collapse: collapse; width: 100%; font-size: .92rem; background: var(--panel); }
th, td { text-align: left; padding: 8px 10px; border-bottom: 1px solid var(--line); vertical-align: top; }
th { font-size: .8rem; letter-spacing: .04em; color: var(--muted); font-weight: 700; }
.chip { display: inline-block; font-size: .78rem; padding: 0 8px; border-radius: 999px; background: var(--accent-soft); color: var(--accent); white-space: nowrap; }
section { display: grid; gap: 16px; padding-top: 8px; border-top: 2px solid var(--line); scroll-margin-top: 72px; }
.sec-head { display: grid; gap: 4px; }
.eyebrow { font-family: var(--font-mono); font-size: .85rem; color: var(--accent); letter-spacing: .04em; }
.where { color: var(--muted); font-size: .9rem; }
.ba { display: grid; grid-template-columns: repeat(auto-fit, minmax(260px, 1fr)); gap: 12px; margin: 0; }
.ba div { background: var(--panel); border: 1px solid var(--line); border-radius: 6px; padding: 12px 14px; min-width: 0; }
.ba dt { font-size: .8rem; font-weight: 700; color: var(--muted); letter-spacing: .04em; }
.ba dd { margin: 4px 0 0; }
.facts { margin: 0; padding-left: 1.2em; display: grid; gap: 6px; max-width: 80ch; }
.rec { display: flex; gap: 10px; align-items: baseline; flex-wrap: wrap; }
.rec span { font-size: .8rem; font-weight: 700; color: var(--panel); background: var(--accent); padding: 0 8px; border-radius: 4px; }
.shot { display: grid; gap: 8px; min-width: 0; }
.view { display: grid; gap: 8px; min-width: 0; }
.count, .note { font-size: .85rem; }
.stage { position: relative; border: 2px solid var(--accent); border-radius: 6px; background: var(--panel); cursor: pointer; overflow: hidden; }
body[data-side="before"] .stage { border-color: var(--before); }
.stage.tall { max-height: 80vh; overflow: auto; }
.layer { position: relative; }
.layer img, .still img { display: block; width: 100%; height: auto; }
.still { border: 1px solid var(--line); border-radius: 6px; overflow: auto; max-height: 80vh; }
.box { position: absolute; border: 2px solid var(--box); border-radius: 2px; box-shadow: 0 0 0 1px rgba(255,255,255,.7); pointer-events: none; }
.tag { position: absolute; top: 6px; left: 6px; font-size: .78rem; font-weight: 700; padding: 1px 8px; border-radius: 4px; pointer-events: none; }
.tag-before { background: var(--before); color: var(--panel); }
.tag-after { background: var(--accent); color: var(--panel); }
body:not([data-side="before"]) .side-before, body:not([data-side="before"]) .tag-before,
body[data-side="before"] .side-after, body[data-side="before"] .tag-after { display: none; }
${widths.length > 0 ? `body:not([data-width]) .view:not(.w-${firstWidth}) { display: none; }` : ""}
details summary { cursor: pointer; font-size: .85rem; color: var(--accent); }
details[open] summary { margin-bottom: 8px; }
${hideOtherWidths}
${narrow}
</style>
<div class="wrap">
  <header class="intro">
    <h1>${esc(config.heading ?? config.title ?? "変更前後の比較")}</h1>
    ${(config.intro ?? []).map((p, i) => `<p${i > 0 ? ' class="muted"' : ""}>${md(p)}</p>`).join("\n    ")}
  </header>
  ${summary}
  <div class="toolbar" role="group" aria-label="${esc(t.side)}">
    <span class="label">${esc(t.side)}</span>
    <span class="seg"><button type="button" class="seg-btn" data-side="before" aria-pressed="false">${esc(t.before)}</button><button type="button" class="seg-btn" data-side="after" aria-pressed="true">${esc(t.after)}</button></span>
    ${widths.length > 1 ? `<span class="label">${esc(t.width)}</span><span class="seg">${widthButtons}</span>` : ""}
    <span class="hint">${esc(t.keys)}</span>
  </div>
  ${groups.map((g) => section(g, t)).join("\n")}
</div>
<script>
(function () {
  var key = ${JSON.stringify(storageKey)};
  var body = document.body;
  function press(attr, value) {
    document.querySelectorAll(".seg-btn[data-" + attr + "]").forEach(function (b) {
      b.setAttribute("aria-pressed", String(b.getAttribute("data-" + attr) === value));
    });
  }
  function setSide(side) { body.setAttribute("data-side", side); press("side", side); }
  function setWidth(width) {
    body.setAttribute("data-width", width); press("width", width);
    try { localStorage.setItem(key + ":width", width) } catch (e) {}
  }
  var saved = null; try { saved = localStorage.getItem(key + ":width") } catch (e) {}
  var widths = ${JSON.stringify(widths)};
  setWidth(widths.indexOf(saved) >= 0 ? saved : ${JSON.stringify(firstWidth)});
  setSide("after");
  function flip() { setSide(body.getAttribute("data-side") === "after" ? "before" : "after"); }
  document.querySelectorAll(".seg-btn[data-side]").forEach(function (b) { b.addEventListener("click", function () { setSide(b.getAttribute("data-side")) }) });
  document.querySelectorAll(".seg-btn[data-width]").forEach(function (b) { b.addEventListener("click", function () { setWidth(b.getAttribute("data-width")) }) });
  document.querySelectorAll(".stage").forEach(function (s) {
    s.addEventListener("click", flip);
    s.addEventListener("keydown", function (e) { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); flip() } });
  });
  document.addEventListener("keydown", function (e) {
    if (e.target && /^(INPUT|TEXTAREA|SELECT)$/.test(e.target.tagName)) return;
    if (e.key === "ArrowLeft" || e.key === "1") { setSide("before"); e.preventDefault() }
    else if (e.key === "ArrowRight" || e.key === "2") { setSide("after"); e.preventDefault() }
  });
})();
</script>
`
}
