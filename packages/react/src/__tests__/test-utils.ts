/// Test utilities shared across GPUIX test files.

import fs from "fs"
import path from "path"
import { fileURLToPath } from "url"
import { expect } from "vitest"

export const isCI = !!process.env.CI

/** Where visual tests write their PNGs. Kept in the repo (gitignored) rather
 *  than /tmp so the output can actually be looked at after a run, and because
 *  `/tmp` does not exist on Windows: native `save()` there fails the whole
 *  test with `The system cannot find the path specified`.
 *
 *  Created on import, so a file that only writes a screenshot needs no
 *  `beforeAll`. Native never creates the parent directory itself. */
export const SHOTS_DIR = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "../../screenshots"
)

fs.mkdirSync(SHOTS_DIR, { recursive: true })

/** A monospace family the host actually has installed.
 *
 *  A test that asserts a font *changes* the picture must name a real family:
 *  an unknown one falls back to the default and paints byte-identical output.
 *  `Menlo` is macOS only, which is what failed once Windows started running
 *  the full suite. */
export const MONO_FAMILY =
  process.platform === "win32"
    ? "Consolas"
    : process.platform === "darwin"
      ? "Menlo"
      : "DejaVu Sans Mono"

/** Compute byte-level similarity between two buffers (0..1).
 *  For PNGs from the same renderer, identical pixels → identical bytes
 *  (same encoder settings). Any pixel change cascades through compression,
 *  so even small visual diffs produce low byte similarity. */
export function bufferSimilarity(a: Buffer, b: Buffer): number {
  const len = Math.max(a.length, b.length)
  if (len === 0) return 1
  let matching = 0
  for (let i = 0; i < len; i++) {
    if (a[i] === b[i]) matching++
  }
  return matching / len
}

/** Assert two screenshot PNGs exist, are non-empty, and are visually
 *  different (less than 99% byte similarity).
 *  Skipped on CI — Metal on macOS VMs doesn't repaint between captures,
 *  producing byte-identical screenshots regardless of state changes. */
export function expectScreenshotsDiffer(beforePath: string, afterPath: string) {
  expect(fs.existsSync(beforePath)).toBe(true)
  expect(fs.existsSync(afterPath)).toBe(true)
  expect(fs.statSync(beforePath).size).toBeGreaterThan(0)
  expect(fs.statSync(afterPath).size).toBeGreaterThan(0)

  if (isCI) return

  const before = fs.readFileSync(beforePath)
  const after = fs.readFileSync(afterPath)
  const similarity = bufferSimilarity(before, after)
  expect(similarity).toBeLessThan(0.99)
}

export function expectScreenshotsEqual(leftPath: string, rightPath: string) {
  expect(fs.existsSync(leftPath)).toBe(true)
  expect(fs.existsSync(rightPath)).toBe(true)
  const left = fs.readFileSync(leftPath)
  const right = fs.readFileSync(rightPath)
  expect(left.equals(right)).toBe(true)
}
