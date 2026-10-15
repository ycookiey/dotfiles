// node scripts/diff.test.mjs — checks the changed blocks on small generated images.
import assert from "node:assert/strict"
import { mkdtempSync, rmSync } from "node:fs"
import { tmpdir } from "node:os"
import path from "node:path"
import { execFileSync } from "node:child_process"

import { alignRows, assertMagick, bandsOf, diffFiles } from "./diff.mjs"

assertMagick()
const dir = mkdtempSync(path.join(tmpdir(), "ui-before-after-"))
const magick = (...args) => execFileSync(process.env.UI_BEFORE_AFTER_MAGICK ?? "magick", args)
const file = (name) => path.join(dir, name)
let failures = 0
const check = (name, run) => {
  try {
    run()
    console.log(`ok ${name}`)
  } catch (error) {
    failures++
    console.log(`NG ${name}: ${error.message}`)
  }
}
// A page of distinct gray stripes, so each row can be told apart.
const stripes = (name, { insertAt = -1, insert = 0, mark = null } = {}) => {
  const draws = []
  let y = 0
  for (let row = 0; row < 30; row++) {
    if (row === insertAt) {
      draws.push("-fill", "blue", "-draw", `rectangle 0,${y} 199,${y + insert - 1}`)
      y += insert
    }
    const g = 40 + ((row * 37) % 200)
    draws.push("-fill", `rgb(${g},${g},${g})`, "-draw", `rectangle 0,${y} 199,${y + 9}`)
    y += 10
  }
  if (mark !== null) draws.push("-fill", "red", "-draw", mark)
  magick("-size", `200x${y}`, "xc:white", ...draws, file(name))
}

try {
  stripes("base.png")
  stripes("mark.png", { mark: "rectangle 40,100 59,109" })
  stripes("two.png", { mark: "rectangle 40,100 59,109" })
  magick(file("two.png"), "-fill", "red", "-draw", "rectangle 10,260 19,269", file("two.png"))
  stripes("inserted.png", { insertAt: 5, insert: 40 })

  check("identical images have no block", () => {
    const d = diffFiles(file("base.png"), file("base.png"))
    assert.deepEqual(d.blocks, [])
    assert.deepEqual(bandsOf(d), [])
  })
  check("a changed rectangle in place gets a box on the cell grid", () => {
    const d = diffFiles(file("base.png"), file("mark.png"))
    assert.equal(d.blocks.length, 1)
    const [box] = d.blocks[0].boxes.after
    assert.ok(box.x <= 40 && box.x + box.w >= 60, `x ${box.x}+${box.w}`)
    assert.ok(box.y <= 100 && box.y + box.h >= 110, `y ${box.y}+${box.h}`)
    assert.ok(box.w <= 40 && box.h <= 24, `too big ${box.w}x${box.h}`)
  })
  check("rows pushed down by an inserted part are not counted as changed", () => {
    const d = diffFiles(file("base.png"), file("inserted.png"))
    assert.equal(d.blocks.length, 1)
    const [block] = d.blocks
    assert.deepEqual(block.before, { y: 50, h: 0 })
    assert.deepEqual(block.after, { y: 50, h: 40 })
  })
  check("changes far apart are separate bands unless their margins meet", () => {
    const d = diffFiles(file("base.png"), file("two.png"))
    assert.equal(d.blocks.length, 2)
    assert.equal(bandsOf(d, { margin: 16 }).length, 2)
    assert.equal(bandsOf(d, { margin: 120 }).length, 1)
  })
  check("the row alignment keeps the order on both sides", () => {
    const pairs = alignRows([1, 2, 3, 4, 5], [1, 9, 2, 3, 5])
    assert.deepEqual(pairs, [[0, 0], [1, 2], [2, 3], [4, 4]])
  })
} finally {
  rmSync(dir, { recursive: true, force: true })
}
if (failures > 0) process.exit(1)
console.log("all cases passed")
