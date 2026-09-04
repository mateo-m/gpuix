/// Identity tests: every host element that JS can style or target must own a
/// stable GPUI element id and record its painted bounds.
///
/// The gaps these cover were all invisible from JS: a prop type-checked, the
/// listener was registered, and nothing ever happened. `<text onClick>` was
/// dropped by a separate text builder, `hover` / `active` were parsed for every
/// element but only consumed by `<div>`, and `<img>` / `<svg>` / `<anchored>`
/// appeared in the automation tree with no box to click.

import fs from "fs"
import path from "path"
import React, { useState } from "react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import { createTestRoot, hasNativeTestRenderer, type TestRoot } from "../testing.js"
import { expectScreenshotsDiffer, SHOTS_DIR } from "./test-utils.js"

const describeNative = hasNativeTestRenderer ? describe : describe.skip

function shot(name: string): string {
  fs.mkdirSync(SHOTS_DIR, { recursive: true })
  const file = path.join(SHOTS_DIR, `${name}.png`)
  if (fs.existsSync(file)) fs.unlinkSync(file)
  return file
}

function bounds(renderer: TestRoot["renderer"], testId: string) {
  const element = renderer.findByTestId(testId)
  expect(element, `missing testId ${testId}`).toBeDefined()
  const rect = renderer.getElementBounds(element!.id)
  expect(rect, `no painted bounds for ${testId}`).toEqual(expect.any(Array))
  return { x: rect![0], y: rect![1], width: rect![2], height: rect![3] }
}

describeNative("text element identity", () => {
  let testRoot: TestRoot

  beforeEach(() => {
    testRoot = createTestRoot()
  })

  it("fires onClick on a <text> node", () => {
    function Clickable() {
      const [count, setCount] = useState(0)
      return (
        <text
          style={{ width: 200, height: 60 }}
          onClick={() => setCount((value) => value + 1)}
        >
          {`clicks: ${count}`}
        </text>
      )
    }

    testRoot.render(<Clickable />)
    expect(testRoot.renderer.getAllText()).toEqual(["clicks: 0"])

    testRoot.renderer.nativeSimulateClick(10, 10)
    expect(testRoot.renderer.getAllText()).toEqual(["clicks: 1"])
  })

  it("fires onMouseEnter and onMouseLeave on a <text> node", () => {
    function Hoverable() {
      const [hovered, setHovered] = useState(false)
      return (
        <text
          style={{ width: 200, height: 100 }}
          onMouseEnter={() => setHovered(true)}
          onMouseLeave={() => setHovered(false)}
        >
          {hovered ? "hovered" : "idle"}
        </text>
      )
    }

    testRoot.render(<Hoverable />)
    expect(testRoot.renderer.getAllText()).toEqual(["idle"])

    testRoot.renderer.nativeSimulateMouseMove(50, 50)
    expect(testRoot.renderer.getAllText()).toEqual(["hovered"])

    testRoot.renderer.nativeSimulateMouseMove(600, 600)
    expect(testRoot.renderer.getAllText()).toEqual(["idle"])
  })

  // Now that `<text>` runs through the same builder as `<div>`, a filled text
  // node inserts a hitbox and stops clicks behind it, exactly like an HTML
  // element with a background. The old text builder inserted none.
  it("blocks a click behind a filled <text>", () => {
    const behind = vi.fn()
    testRoot.render(
      <div style={{ width: 600, height: 400 }} onClick={behind}>
        <div
          style={{
            position: "absolute",
            top: 0,
            left: 0,
            width: 200,
            height: 100,
          }}
        >
          <text style={{ width: 200, height: 100, backgroundColor: "#f38ba8" }}>
            label
          </text>
        </div>
      </div>,
    )

    testRoot.renderer.nativeSimulateClick(100, 50)
    expect(behind).not.toHaveBeenCalled()

    testRoot.renderer.nativeSimulateClick(400, 300)
    expect(behind).toHaveBeenCalledTimes(1)
  })

  it("paints the hover style declared on a <text> node", () => {
    testRoot.render(
      <div style={{ width: "100%", height: "100%", backgroundColor: "#11111b" }}>
        <text
          style={{
            width: 400,
            height: 120,
            backgroundColor: "#1e1e2e",
            hover: { backgroundColor: "#f38ba8" },
          }}
        >
          hover target
        </text>
      </div>,
    )

    testRoot.renderer.nativeSimulateMouseMove(1200, 700)
    const before = shot("identity-text-hover-before")
    testRoot.renderer.captureScreenshot(before)

    testRoot.renderer.nativeSimulateMouseMove(200, 60)
    testRoot.renderer.flush()
    const after = shot("identity-text-hover-after")
    testRoot.renderer.captureScreenshot(after)

    expectScreenshotsDiffer(before, after)
  })
})

describeNative("pseudo styles on native surfaces", () => {
  let testRoot: TestRoot

  beforeEach(() => {
    testRoot = createTestRoot()
  })

  it("paints the hover style declared on a <code> block", () => {
    testRoot.render(
      <div style={{ width: "100%", height: "100%", backgroundColor: "#11111b" }}>
        <code
          code={"const answer = 42\n"}
          language="ts"
          style={{
            width: 500,
            height: 120,
            backgroundColor: "#1e1e2e",
            hover: { backgroundColor: "#f38ba8" },
          }}
        />
      </div>,
    )

    testRoot.renderer.nativeSimulateMouseMove(1200, 700)
    const before = shot("identity-code-hover-before")
    testRoot.renderer.captureScreenshot(before)

    testRoot.renderer.nativeSimulateMouseMove(250, 60)
    testRoot.renderer.flush()
    const after = shot("identity-code-hover-after")
    testRoot.renderer.captureScreenshot(after)

    expectScreenshotsDiffer(before, after)
  })

  it("paints the active style on a <div> that has no click listener", () => {
    testRoot.render(
      <div style={{ width: "100%", height: "100%", backgroundColor: "#11111b" }}>
        <div
          style={{
            width: 400,
            height: 160,
            backgroundColor: "#1e1e2e",
            active: { backgroundColor: "#f38ba8" },
          }}
        />
      </div>,
    )

    testRoot.renderer.nativeSimulateMouseMove(200, 80)
    testRoot.renderer.flush()
    const before = shot("identity-active-idle")
    testRoot.renderer.captureScreenshot(before)

    testRoot.renderer.nativeSimulateMouseDown(200, 80)
    testRoot.renderer.flush()
    const after = shot("identity-active-pressed")
    testRoot.renderer.captureScreenshot(after)
    testRoot.renderer.nativeSimulateMouseUp(200, 80)

    expectScreenshotsDiffer(before, after)
  })
})

describeNative("events on native surfaces", () => {
  let testRoot: TestRoot

  beforeEach(() => {
    testRoot = createTestRoot()
  })

  const ICON =
    '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><rect width="16" height="16" fill="#000"/></svg>'

  it("fires onClick on an <svg>", () => {
    function Clickable() {
      const [count, setCount] = useState(0)
      return (
        <div style={{ display: "flex", flexDirection: "column" }}>
          <svg
            source={ICON}
            style={{ width: 120, height: 80, color: "#5ca9ff" }}
            onClick={() => setCount((value) => value + 1)}
          />
          <text>{`clicks: ${count}`}</text>
        </div>
      )
    }

    testRoot.render(<Clickable />)
    expect(testRoot.renderer.getAllText()).toEqual(["clicks: 0"])

    testRoot.renderer.nativeSimulateClick(60, 40)
    expect(testRoot.renderer.getAllText()).toEqual(["clicks: 1"])
  })

  it("fires onMouseEnter and onMouseLeave on an <img>", () => {
    const fixture = path.join(SHOTS_DIR, "identity-img-events.svg")
    fs.mkdirSync(SHOTS_DIR, { recursive: true })
    fs.writeFileSync(fixture, ICON, "utf8")

    function Hoverable() {
      const [hovered, setHovered] = useState(false)
      return (
        <div style={{ display: "flex", flexDirection: "column" }}>
          <img
            src={fixture}
            objectFit="fill"
            style={{ width: 200, height: 120 }}
            onMouseEnter={() => setHovered(true)}
            onMouseLeave={() => setHovered(false)}
          />
          <text>{hovered ? "hovered" : "idle"}</text>
        </div>
      )
    }

    testRoot.render(<Hoverable />)
    expect(testRoot.renderer.getAllText()).toEqual(["idle"])

    testRoot.renderer.nativeSimulateMouseMove(100, 60)
    expect(testRoot.renderer.getAllText()).toEqual(["hovered"])

    testRoot.renderer.nativeSimulateMouseMove(900, 700)
    expect(testRoot.renderer.getAllText()).toEqual(["idle"])
  })

  it("fires onClick on an <anchored> overlay", () => {
    function Menu() {
      const [count, setCount] = useState(0)
      return (
        <div style={{ width: 800, height: 500 }}>
          <text>{`picked: ${count}`}</text>
          <anchored
            position={{ x: 300, y: 200 }}
            style={{ width: 240, height: 100, backgroundColor: "#1e1e2e" }}
            onClick={() => setCount((value) => value + 1)}
          >
            <text>item</text>
          </anchored>
        </div>
      )
    }

    testRoot.render(<Menu />)
    expect(testRoot.renderer.getAllText()).toEqual(["picked: 0", "item"])

    testRoot.renderer.nativeSimulateClick(420, 250)
    expect(testRoot.renderer.getAllText()).toEqual(["picked: 1", "item"])
  })
})

describeNative("painted bounds for leaf surfaces", () => {
  let testRoot: TestRoot

  beforeEach(() => {
    testRoot = createTestRoot()
  })

  it("records bounds for <svg> without changing its layout box", () => {
    testRoot.render(
      <div style={{ display: "flex", width: 600, height: 300, padding: 20 }}>
        <svg
          testId="icon"
          source={'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><rect width="16" height="16" fill="#000"/></svg>'}
          style={{ width: 48, height: 32, color: "#5ca9ff" }}
        />
      </div>,
    )

    const icon = bounds(testRoot.renderer, "icon")
    expect(icon.width).toBeCloseTo(48, 0)
    expect(icon.height).toBeCloseTo(32, 0)
    expect(icon.x).toBeCloseTo(20, 0)
    expect(icon.y).toBeCloseTo(20, 0)
  })

  it("records bounds for <img> without changing its layout box", () => {
    const fixture = path.join(SHOTS_DIR, "identity-img-fixture.svg")
    fs.mkdirSync(SHOTS_DIR, { recursive: true })
    fs.writeFileSync(
      fixture,
      '<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"><rect width="40" height="20" fill="#5ca9ff"/></svg>',
      "utf8",
    )

    testRoot.render(
      <div style={{ display: "flex", width: 600, height: 300, padding: 24 }}>
        <img testId="picture" src={fixture} style={{ width: 200, height: 100 }} />
      </div>,
    )

    const picture = bounds(testRoot.renderer, "picture")
    expect(picture.width).toBeCloseTo(200, 0)
    expect(picture.height).toBeCloseTo(100, 0)
    expect(picture.x).toBeCloseTo(24, 0)
    expect(picture.y).toBeCloseTo(24, 0)
  })

  it("records the overlay bounds of a deferred <anchored>, not its trigger", () => {
    testRoot.render(
      <div style={{ width: 800, height: 400 }}>
        <div testId="trigger" style={{ width: 120, height: 40 }}>
          <text>trigger</text>
          <anchored
            testId="overlay"
            position={{ x: 300, y: 200 }}
            style={{ width: 260, height: 90, backgroundColor: "#1e1e2e" }}
          >
            <text>overlay</text>
          </anchored>
        </div>
      </div>,
    )

    const trigger = bounds(testRoot.renderer, "trigger")
    const overlay = bounds(testRoot.renderer, "overlay")
    expect(overlay.width).toBeCloseTo(260, 0)
    expect(overlay.height).toBeCloseTo(90, 0)
    expect(overlay.x).toBeCloseTo(300, 0)
    expect(overlay.y).toBeCloseTo(200, 0)
    expect(overlay.x).not.toBeCloseTo(trigger.x, 0)
  })
})

describeNative("gpui image state", () => {
  // gpui keeps `ImgState` (the animated-GIF frame index and the delayed loading
  // placeholder) in `InteractiveElementState`, which only exists when the
  // element has a `GlobalElementId`.
  //
  // The animation itself cannot be asserted here: `Img::request_layout` only
  // advances a frame while `window.is_window_active()`, and the test renderer
  // builds its window through gpui's `VisualTestContext::open_offscreen_window`,
  // which passes `focus: false`, so the window never becomes active. `active`
  // styling reads the same element state through the same id, so it proves the
  // id is there without depending on the animation clock.
  it("gives <img> element state that only a GPUI id can provide", () => {
    const testRoot = createTestRoot()
    const fixture = path.join(SHOTS_DIR, "identity-img-state.svg")
    fs.mkdirSync(SHOTS_DIR, { recursive: true })
    fs.writeFileSync(
      fixture,
      '<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"><rect width="40" height="20" fill="#5ca9ff"/></svg>',
      "utf8",
    )

    testRoot.render(
      <div style={{ width: "100%", height: "100%", backgroundColor: "#11111b" }}>
        <img
          src={fixture}
          objectFit="fill"
          style={{ width: 320, height: 200, active: { opacity: 0.2 } }}
        />
      </div>,
    )

    for (let i = 0; i < 5; i++) testRoot.renderer.flush()
    testRoot.renderer.nativeSimulateMouseMove(160, 100)
    testRoot.renderer.flush()
    const idle = shot("identity-img-active-idle")
    testRoot.renderer.captureScreenshot(idle)

    testRoot.renderer.nativeSimulateMouseDown(160, 100)
    testRoot.renderer.flush()
    const pressed = shot("identity-img-active-pressed")
    testRoot.renderer.captureScreenshot(pressed)
    testRoot.renderer.nativeSimulateMouseUp(160, 100)

    expectScreenshotsDiffer(idle, pressed)
  })
})
