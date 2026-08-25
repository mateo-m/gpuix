/// `className` painting the same pixels as the style it stands for.
///
/// The merge rules are covered without a GPU in `host-config-style.test.tsx`.
/// What these add is that the style a class declares reaches the renderer at
/// mount and on an update, through the same path a real application uses.

import fs from "fs"
import path from "path"
import React from "react"
import { beforeAll, describe, expect, it } from "vitest"
import { createTestRoot, hasNativeTestRenderer } from "../testing.js"
import { expectScreenshotsEqual, SHOTS_DIR } from "./test-utils.js"
import type { ClassNameResolver } from "../types/host.js"

const describeNative = hasNativeTestRenderer ? describe : describe.skip

beforeAll(() => {
  fs.mkdirSync(SHOTS_DIR, { recursive: true })
})

const shot = (name: string) => path.join(SHOTS_DIR, `class-${name}.png`)

const BOX = { width: 200, height: 120 } as const

const TABLE: Record<string, Record<string, unknown>> = {
  box: BOX,
  "bg-red": { backgroundColor: "#ff0000" },
  "bg-blue": { backgroundColor: "#0000ff" },
  "p-5": { padding: 20 },
  "child-green": { width: 40, height: 40, backgroundColor: "#00ff00" },
}

const resolveClassName: ClassNameResolver = (token) => TABLE[token] ?? null

function paint(name: string, tree: React.ReactElement, withResolver = true) {
  const root = createTestRoot(withResolver ? { resolveClassName } : {})
  root.render(tree)
  root.renderer.captureScreenshot(shot(name))
  root.unmount()
}

describeNative("className", () => {
  it("paints a class the same as the style it stands for", () => {
    paint("through", <div className="box bg-red" />)
    paint("direct", <div style={{ ...BOX, backgroundColor: "#ff0000" }} />, false)
    expectScreenshotsEqual(shot("through"), shot("direct"))
  })

  it("paints a class and a style prop together", () => {
    paint("mixed", <div className="box p-5 bg-red">
      <div className="child-green" />
    </div>)
    paint(
      "mixed-direct",
      <div style={{ ...BOX, padding: 20, backgroundColor: "#ff0000" }}>
        <div style={{ width: 40, height: 40, backgroundColor: "#00ff00" }} />
      </div>,
      false
    )
    expectScreenshotsEqual(shot("mixed"), shot("mixed-direct"))
  })

  it("lets the style prop beat the class", () => {
    paint("override", <div className="box bg-red" style={{ backgroundColor: "#0000ff" }} />)
    paint("override-direct", <div style={{ ...BOX, backgroundColor: "#0000ff" }} />, false)
    expectScreenshotsEqual(shot("override"), shot("override-direct"))
  })

  it("repaints when the class string changes", () => {
    const root = createTestRoot({ resolveClassName })
    root.render(<div className="box bg-red" />)
    root.render(<div className="box bg-blue" />)
    root.renderer.captureScreenshot(shot("changed"))
    root.unmount()

    paint("changed-expected", <div style={{ ...BOX, backgroundColor: "#0000ff" }} />, false)
    expectScreenshotsEqual(shot("changed"), shot("changed-expected"))
  })

  it("paints nothing from a class when the root has no resolver", () => {
    const warn = console.warn
    console.warn = () => {}
    try {
      paint("no-resolver", <div className="box bg-red" style={BOX} />, false)
    } finally {
      console.warn = warn
    }
    paint("no-resolver-direct", <div style={BOX} />, false)
    expectScreenshotsEqual(shot("no-resolver"), shot("no-resolver-direct"))
  })

  it("resolves nothing again when the same class string comes back", () => {
    const { renderer, render } = createTestRoot({ resolveClassName })
    render(<div className="box bg-red" />)
    renderer.resetStyleResolutions()
    render(<div className="box bg-red" />)
    expect(renderer.styleResolutions()).toBe(0)
  })
})
