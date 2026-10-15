// Builds one HTML page that compares before/after screenshots item by item.
// usage: node build.mjs --images <dir> --config <config.mjs|json> --out <page.html>
//                       [--margin 32] [--threshold 48] [--gap 48]
import { execFileSync } from "node:child_process"
import { existsSync, readFileSync, readdirSync, writeFileSync, renameSync } from "node:fs"
import path from "node:path"
import { pathToFileURL } from "node:url"

import { assertMagick, bandsOf, diffRgb, readRgb } from "./diff.mjs"
import { pageHtml } from "./page.mjs"

const MAGICK = process.env.UI_BEFORE_AFTER_MAGICK ?? "magick"
const SIZE_LIMIT = 16 * 1024 * 1024

const args = (() => {
  const out = {}
  const argv = process.argv.slice(2)
  for (let i = 0; i < argv.length; i++) {
    if (!argv[i].startsWith("--")) throw new Error(`unexpected argument ${argv[i]}`)
    out[argv[i].slice(2)] = argv[++i]
  }
  for (const key of ["images", "config", "out"]) {
    if (out[key] === undefined) throw new Error(`--${key} is required (see SKILL.md)`)
  }
  return out
})()

const loadConfig = async (file) => {
  if (file.endsWith(".json")) return JSON.parse(readFileSync(file, "utf8"))
  const mod = await import(pathToFileURL(path.resolve(file)).href)
  return mod.default ?? mod.config ?? mod
}

assertMagick()
const config = await loadConfig(args.config)
const imagesDir = path.resolve(args.images)
const diffOptions = {
  threshold: Number(args.threshold ?? 48),
  gap: Number(args.gap ?? 48),
}
const margin = Number(args.margin ?? 32)

// Widths: the config's order, else what the file names show.
const files = readdirSync(imagesDir)
const discovered = [...new Set(files.map((f) => f.match(/-(?:before|after)-([^-.]+)\.png$/)?.[1]).filter(Boolean))]
const widthConfig = config.widths ?? Object.fromEntries(discovered.map((w) => [w, {}]))
const widths = Object.keys(widthConfig).filter((w) => discovered.includes(w))

/**
 * Rows `top`..`top + rows` of the image, `width` wide, padded with white to
 * `padTo` rows, as a WebP data URI no wider than `maxWidth`.
 */
const webp = (file, { width, top, rows, padTo, maxWidth }) => {
  const resize = maxWidth !== undefined && width > maxWidth ? ["-resize", `${maxWidth}x`] : []
  const data = execFileSync(
    MAGICK,
    [
      `${file}[0]`, "-background", "white", "-extent", `${width}x${top + rows}`,
      "-crop", `${width}x${rows}+0+${top}`, "+repage",
      "-extent", `${width}x${padTo}`,
      ...resize, "-quality", "72", "webp:-",
    ],
    { maxBuffer: 1024 * 1024 * 256 }
  )
  return `data:image/webp;base64,${data.toString("base64")}`
}

/** Boxes in percent of a crop `width` wide and `height` tall that starts at row `top`. */
const boxesIn = (regions, { width, top, height }) =>
  regions.map((r) => ({
    left: (r.x / width) * 100,
    top: ((r.y - top) / height) * 100,
    width: (r.w / width) * 100,
    height: (r.h / height) * 100,
  }))

const shotView = (key, width) => {
  const before = path.join(imagesDir, `${key}-before-${width}.png`)
  const after = path.join(imagesDir, `${key}-after-${width}.png`)
  const maxWidth = widthConfig[width]?.maxWidth
  const hasBefore = existsSync(before)
  const hasAfter = existsSync(after)
  if (!hasBefore && !hasAfter) return null
  if (!hasBefore || !hasAfter) {
    const only = hasBefore ? before : after
    const img = readRgb(only)
    return {
      width,
      only: hasBefore ? "before" : "after",
      full: webp(only, { width: img.width, top: 0, rows: img.height, padTo: img.height, maxWidth }),
    }
  }
  const diff = diffRgb(readRgb(before), readRgb(after), diffOptions)
  const w = diff.width
  // Each side shows its own rows; the shorter crop is padded so the frame
  // keeps its height while the view flips.
  const pair = (span, boxes) => {
    const height = Math.max(span.before.bottom - span.before.top, span.after.bottom - span.after.top)
    const side = (name, file) => ({
      src: webp(file, { width: w, top: span[name].top, rows: span[name].bottom - span[name].top, padTo: height, maxWidth }),
      boxes: boxesIn(boxes[name], { width: w, top: span[name].top, height }),
    })
    return { before: side("before", before), after: side("after", after) }
  }
  const boxesOf = (blocks, name) => blocks.flatMap((b) => b.boxes[name])
  const bands = bandsOf(diff, { margin }).map((band) =>
    pair(band, { before: boxesOf(band.blocks, "before"), after: boxesOf(band.blocks, "after") })
  )
  const full = pair(
    { before: { top: 0, bottom: diff.heights.before }, after: { top: 0, bottom: diff.heights.after } },
    { before: boxesOf(diff.blocks, "before"), after: boxesOf(diff.blocks, "after") }
  )
  return { width, blocks: diff.blocks, bands, full }
}

const normalizeShot = (shot) => (Array.isArray(shot) ? { key: shot[0], caption: shot[1] } : shot)
const groups = (config.groups ?? []).map((g) => ({
  ...g,
  label: g.label ?? g.items ?? g.id,
  shots: (g.shots ?? []).map(normalizeShot).map((shot) => ({
    ...shot,
    views: widths.map((w) => shotView(shot.key, w)).filter(Boolean),
  })),
}))

const html = pageHtml({ config, groups, widths, widthConfig })
const out = path.resolve(args.out)
const tmp = `${out}.tmp`
writeFileSync(tmp, html)
renameSync(tmp, out)

const bytes = Buffer.byteLength(html)
console.log(`page ${out} (${(bytes / 1024 / 1024).toFixed(2)} MB)`)
for (const g of groups) {
  for (const s of g.shots) {
    for (const v of s.views) {
      const what = v.only
        ? `${v.only} only`
        : v.blocks.length === 0
          ? "no difference"
          : v.blocks.map((b) => `before ${b.before.h}@${b.before.y} after ${b.after.h}@${b.after.y}`).join(" | ")
      console.log(`  ${s.key} ${v.width}: ${what}`)
    }
  }
}
if (bytes > SIZE_LIMIT) console.warn(`warning: the page is over 16 MB (${bytes} bytes); an Artifact page must stay at or under it`)
