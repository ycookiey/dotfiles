// Where two screenshots differ, using ImageMagick to decode them.
//
// Rows are aligned first (a patience diff on row hashes), so content that
// only moved down because something above it grew is not counted as
// changed. What is left are changed blocks: rows only one side has, or rows
// both sides have at the same place in the alignment but with other pixels.
import { execFileSync } from "node:child_process"

const MAGICK = process.env.UI_BEFORE_AFTER_MAGICK ?? "magick"
const MAX_BUFFER = 1024 * 1024 * 512

/** Throws an error that says what to install when ImageMagick is missing. */
export const assertMagick = () => {
  try {
    execFileSync(MAGICK, ["-version"], { stdio: "ignore" })
  } catch {
    throw new Error(
      `ImageMagick (${MAGICK}) was not found. Install ImageMagick 7 or set UI_BEFORE_AFTER_MAGICK to its path.`
    )
  }
}

/** The image's width, height and RGB bytes (3 per pixel, row by row). */
export const readRgb = (file) => {
  const [w, h] = execFileSync(MAGICK, ["identify", "-format", "%w %h", `${file}[0]`], { encoding: "utf8" })
    .trim()
    .split(" ")
    .map(Number)
  const data = execFileSync(MAGICK, [`${file}[0]`, "-alpha", "off", "-depth", "8", "rgb:-"], {
    maxBuffer: MAX_BUFFER,
  })
  if (data.length !== w * h * 3) throw new Error(`unexpected pixel data for ${file}`)
  return { width: w, height: h, data }
}

/** One hash per pixel row, over `width` pixels (missing pixels count as white). */
const rowHashes = (img, width) => {
  const hashes = new Array(img.height)
  for (let y = 0; y < img.height; y++) {
    let h = 2166136261
    const base = y * img.width * 3
    for (let x = 0; x < width; x++) {
      for (let k = 0; k < 3; k++) {
        const v = x < img.width ? img.data[base + x * 3 + k] >> 2 : 63
        h ^= v
        h = Math.imul(h, 16777619)
      }
    }
    hashes[y] = h >>> 0
  }
  return hashes
}

/** Pairs [i, j] of rows of `a` and `b` that match, in order (a patience diff). */
export const alignRows = (a, b) => {
  const pairs = []
  const walk = (a0, a1, b0, b1) => {
    // Equal rows at the start and the end of the range.
    while (a0 < a1 && b0 < b1 && a[a0] === b[b0]) pairs.push([a0++, b0++])
    const tail = []
    while (a0 < a1 && b0 < b1 && a[a1 - 1] === b[b1 - 1]) tail.push([--a1, --b1])
    if (a0 < a1 && b0 < b1) {
      // Rows unique on both sides anchor the middle.
      const count = new Map()
      for (let i = a0; i < a1; i++) {
        const c = count.get(a[i]) ?? { a: 0, b: 0, ai: i, bi: -1 }
        c.a++
        c.ai = i
        count.set(a[i], c)
      }
      for (let j = b0; j < b1; j++) {
        const c = count.get(b[j])
        if (c !== undefined) {
          c.b++
          c.bi = j
        }
      }
      const anchors = [...count.values()].filter((c) => c.a === 1 && c.b === 1).sort((p, q) => p.ai - q.ai)
      // Longest run of anchors increasing on both sides.
      const tails = []
      const prev = new Array(anchors.length)
      for (let k = 0; k < anchors.length; k++) {
        let lo = 0
        let hi = tails.length
        while (lo < hi) {
          const mid = (lo + hi) >> 1
          if (anchors[tails[mid]].bi < anchors[k].bi) lo = mid + 1
          else hi = mid
        }
        prev[k] = lo > 0 ? tails[lo - 1] : -1
        tails[lo] = k
      }
      const chain = []
      for (let k = tails.length > 0 ? tails[tails.length - 1] : -1; k >= 0; k = prev[k]) chain.push(anchors[k])
      chain.reverse()
      let pa = a0
      let pb = b0
      for (const c of chain) {
        walk(pa, c.ai, pb, c.bi)
        pairs.push([c.ai, c.bi])
        pa = c.ai + 1
        pb = c.bi + 1
      }
      if (chain.length > 0) walk(pa, a1, pb, b1)
      else lcs(a0, a1, b0, b1)
    }
    pairs.push(...tail.reverse())
  }
  // With no row unique on both sides (blank or striped areas), the longest
  // common run of rows, if the range is small enough to table.
  const lcs = (a0, a1, b0, b1) => {
    const n = a1 - a0
    const m = b1 - b0
    if (n * m > 4_000_000) return
    const table = new Uint32Array((n + 1) * (m + 1))
    for (let i = n - 1; i >= 0; i--) {
      for (let j = m - 1; j >= 0; j--) {
        table[i * (m + 1) + j] =
          a[a0 + i] === b[b0 + j]
            ? table[(i + 1) * (m + 1) + j + 1] + 1
            : Math.max(table[(i + 1) * (m + 1) + j], table[i * (m + 1) + j + 1])
      }
    }
    let i = 0
    let j = 0
    while (i < n && j < m) {
      if (a[a0 + i] === b[b0 + j]) {
        pairs.push([a0 + i, b0 + j])
        i++
        j++
      } else if (table[(i + 1) * (m + 1) + j] >= table[i * (m + 1) + j + 1]) i++
      else j++
    }
  }
  walk(0, a.length, 0, b.length)
  pairs.sort((p, q) => p[0] - q[0])
  return pairs
}

/** Boxes where two equally tall slices differ, on a grid of `cell` px. */
const pixelBoxes = (a, ay, b, by, height, width, { threshold, cell, gap }) => {
  const cols = Math.ceil(width / cell)
  const rows = Math.ceil(height / cell)
  const changed = new Uint8Array(cols * rows)
  const px = (img, x, y) => (x < img.width ? (y * img.width + x) * 3 : -1)
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const c = Math.floor(y / cell) * cols + Math.floor(x / cell)
      if (changed[c] === 1) continue
      const i = px(a, x, ay + y)
      const j = px(b, x, by + y)
      const va = (k) => (i < 0 ? 255 : a.data[i + k])
      const vb = (k) => (j < 0 ? 255 : b.data[j + k])
      if (Math.abs(va(0) - vb(0)) + Math.abs(va(1) - vb(1)) + Math.abs(va(2) - vb(2)) > threshold) changed[c] = 1
    }
  }
  const reach = Math.max(0, Math.ceil(gap / cell))
  const seen = new Uint8Array(cols * rows)
  const boxes = []
  for (let s = 0; s < changed.length; s++) {
    if (changed[s] === 0 || seen[s] === 1) continue
    let minC = cols
    let minR = rows
    let maxC = -1
    let maxR = -1
    const stack = [s]
    seen[s] = 1
    while (stack.length > 0) {
      const c = stack.pop()
      const cx = c % cols
      const cy = Math.floor(c / cols)
      minC = Math.min(minC, cx)
      maxC = Math.max(maxC, cx)
      minR = Math.min(minR, cy)
      maxR = Math.max(maxR, cy)
      for (let dy = -reach; dy <= reach; dy++) {
        for (let dx = -reach; dx <= reach; dx++) {
          const nx = cx + dx
          const ny = cy + dy
          if (nx < 0 || ny < 0 || nx >= cols || ny >= rows) continue
          const n = ny * cols + nx
          if (changed[n] === 1 && seen[n] === 0) {
            seen[n] = 1
            stack.push(n)
          }
        }
      }
    }
    const x = minC * cell
    const y = minR * cell
    boxes.push({ x, y, w: Math.min(width, (maxC + 1) * cell) - x, h: Math.min(height, (maxR + 1) * cell) - y })
  }
  return boxes
}

/**
 * The changed blocks between two images. Each block has the rows it covers
 * on each side (`before`/`after`: `{ y, h }`, `h` may be 0) and boxes in
 * page coordinates for each side. A block as tall on both sides is compared
 * pixel by pixel; otherwise its rows are boxed whole.
 */
export const diffRgb = (a, b, { threshold = 48, cell = 8, gap = 48 } = {}) => {
  const width = Math.max(a.width, b.width)
  const pairs = alignRows(rowHashes(a, width), rowHashes(b, width))
  const blocks = []
  let pa = 0
  let pb = 0
  const close = (a1, b1) => {
    if (a1 > pa || b1 > pb) {
      const before = { y: pa, h: a1 - pa }
      const after = { y: pb, h: b1 - pb }
      let boxes
      if (before.h === after.h) {
        const inner = pixelBoxes(a, pa, b, pb, before.h, width, { threshold, cell, gap })
        boxes = {
          before: inner.map((r) => ({ ...r, y: r.y + pa })),
          after: inner.map((r) => ({ ...r, y: r.y + pb })),
        }
      } else {
        boxes = {
          before: before.h > 0 ? [{ x: 0, y: pa, w: width, h: before.h }] : [],
          after: after.h > 0 ? [{ x: 0, y: pb, w: width, h: after.h }] : [],
        }
      }
      if (boxes.before.length > 0 || boxes.after.length > 0) blocks.push({ before, after, boxes })
    }
  }
  for (const [i, j] of pairs) {
    close(i, j)
    pa = i + 1
    pb = j + 1
  }
  close(a.height, b.height)
  return { width, heights: { before: a.height, after: b.height }, blocks }
}

/** The changed blocks between two image files (see `diffRgb`). */
export const diffFiles = (beforeFile, afterFile, options) =>
  diffRgb(readRgb(beforeFile), readRgb(afterFile), options)

/**
 * Bands to show the blocks in: each side's rows with `margin` px of context
 * around them; blocks whose bands would overlap on either side are merged.
 */
export const bandsOf = ({ heights, blocks }, { margin = 32 } = {}) => {
  const bands = []
  for (const block of blocks) {
    const span = (side) => ({
      top: Math.max(0, block[side].y - margin),
      bottom: Math.min(heights[side], block[side].y + block[side].h + margin),
    })
    const next = { before: span("before"), after: span("after"), blocks: [block] }
    const last = bands.at(-1)
    if (last !== undefined && (next.before.top <= last.before.bottom || next.after.top <= last.after.bottom)) {
      last.before.bottom = Math.max(last.before.bottom, next.before.bottom)
      last.after.bottom = Math.max(last.after.bottom, next.after.bottom)
      last.blocks.push(block)
    } else {
      bands.push(next)
    }
  }
  return bands
}
