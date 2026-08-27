/**
 * scroll-behavior, scroll-snap and scroll-initial-target. The automation
 * clock is paused, so the glide of an offset is read at exact times.
 */
import { afterEach, beforeEach, describe, expect, it } from "vitest"
import React from "react"
import { createTestRoot, hasNativeTestRenderer, type TestRoot } from "../testing"
import type { StyleDesc } from "../types/host"

const describeNative = hasNativeTestRenderer ? describe : describe.skip

/** A scroll box of 200 by 200 with `count` rows of 100. */
function Box({ style, rows }: { style?: StyleDesc; rows: StyleDesc[] }) {
  return (
    <div testId="box" style={{ width: 200, height: 200, overflowY: "auto", ...style }}>
      {rows.map((row, i) => (
        <div key={i} testId={`row-${i}`} style={{ width: 200, height: 100, flexShrink: 0, ...row }} />
      ))}
    </div>
  )
}

const plain = (count: number): StyleDesc[] => Array.from({ length: count }, () => ({}))

describeNative("scroll-behavior", () => {
  let root: TestRoot
  beforeEach(() => {
    root = createTestRoot()
    root.renderer.clockPause()
  })
  afterEach(() => {
    root.unmount()
  })

  const boxId = () => root.renderer.findByTestId("box")!.id
  const offsetY = (id: number) => root.renderer.getScrollOffset(id)![1]

  it("scrollTo glides when the box asks for smooth", () => {
    root.render(<Box style={{ scrollBehavior: "smooth" }} rows={plain(6)} />)
    const id = boxId()
    root.renderer.scrollTo(id, 0, -200)
    expect(offsetY(id)).toBeCloseTo(0, 5)

    root.renderer.clockFastForward(150)
    root.renderer.flush()
    const midway = offsetY(id)
    expect(midway).toBeLessThan(0)
    expect(midway).toBeGreaterThan(-200)

    root.renderer.clockFastForward(400)
    root.renderer.flush()
    expect(offsetY(id)).toBe(-200)
  })

  it("an instant behavior beats the style, and smooth beats auto", () => {
    root.render(<Box style={{ scrollBehavior: "smooth" }} rows={plain(6)} />)
    const id = boxId()
    root.renderer.scrollTo(id, 0, -200, "instant")
    expect(offsetY(id)).toBe(-200)

    root.render(<Box rows={plain(6)} />)
    root.renderer.scrollTo(id, 0, 0, "smooth")
    expect(offsetY(id)).toBe(-200)
    root.renderer.clockFastForward(500)
    root.renderer.flush()
    expect(offsetY(id)).toBe(0)
  })

  it("a direct offset move cancels the glide", () => {
    root.render(<Box style={{ scrollBehavior: "smooth" }} rows={plain(6)} />)
    const id = boxId()
    root.renderer.scrollTo(id, 0, -300)
    root.renderer.clockFastForward(100)
    root.renderer.flush()

    // The user takes over. The glide must not fight the new offset.
    root.renderer.scrollTo(id, 0, -50, "instant")
    root.renderer.clockFastForward(500)
    root.renderer.flush()
    expect(offsetY(id)).toBe(-50)
  })

  it("scrollIntoView glides too", () => {
    root.render(<Box style={{ scrollBehavior: "smooth" }} rows={plain(6)} />)
    const id = boxId()
    const target = root.renderer.findByTestId("row-3")!
    root.renderer.scrollIntoView(target.id, "start")
    expect(offsetY(id)).toBeCloseTo(0, 5)
    root.renderer.clockFastForward(500)
    root.renderer.flush()
    expect(offsetY(id)).toBe(-300)
  })
})

describeNative("scroll snap", () => {
  let root: TestRoot
  beforeEach(() => {
    root = createTestRoot()
    root.renderer.clockPause()
  })
  afterEach(() => {
    root.unmount()
  })

  const boxId = () => root.renderer.findByTestId("box")!.id
  const offsetY = (id: number) => root.renderer.getScrollOffset(id)![1]

  /** Rest, then let the snap glide finish: one frame past the idle time
   *  arms the glide, and one more period lands it. */
  const settle = () => {
    root.renderer.clockFastForward(200)
    root.renderer.flush()
    root.renderer.clockFastForward(400)
    root.renderer.flush()
    root.renderer.flush()
  }

  it("a mandatory container rests on the nearest snap position", () => {
    const rows = plain(6).map(() => ({ scrollSnapAlign: "start" }))
    root.render(<Box style={{ scrollSnapType: "y mandatory" }} rows={rows} />)
    const id = boxId()
    root.renderer.scrollTo(id, 0, -130, "instant")
    settle()
    expect(offsetY(id)).toBe(-100)
  })

  it("proximity gives up beyond half a viewport", () => {
    const rows = plain(8).map((_, i) =>
      i === 0 || i === 7 ? { scrollSnapAlign: "start" } : {}
    )
    root.render(<Box style={{ scrollSnapType: "y proximity" }} rows={rows} />)
    const id = boxId()
    root.renderer.scrollTo(id, 0, -280, "instant")
    settle()
    expect(offsetY(id)).toBe(-280)
  })

  it("scroll-snap-stop always catches a long scroll", () => {
    const rows = plain(8).map((_, i) => ({
      scrollSnapAlign: "start",
      scrollSnapStop: i === 2 ? "always" : undefined,
    }))
    root.render(<Box style={{ scrollSnapType: "y mandatory" }} rows={rows} />)
    const id = boxId()
    root.renderer.scrollTo(id, 0, -440, "instant")
    settle()
    expect(offsetY(id)).toBe(-200)
  })
})

describeNative("logical scroll margins and paddings", () => {
  it("scrollIntoView reads the block variants", () => {
    const root = createTestRoot()
    const rows = plain(8).map((_, i) =>
      i === 5 ? { scrollMarginBlockStart: 16 } : {}
    )
    root.render(<Box style={{ scrollPaddingBlock: 12 }} rows={rows} />)
    const box = root.renderer.findByTestId("box")!
    const target = root.renderer.findByTestId("row-5")!
    root.renderer.scrollIntoView(target.id, "start")
    // The row sits at 500. The box keeps 12 inside its edge and the row
    // asks for 16 above itself, so the offset is 500 - 28.
    expect(root.renderer.getScrollOffset(box.id)![1]).toBe(-472)
    root.unmount()
  })
})

describeNative("scroll-initial-target", () => {
  it("scrolls the box to the element when it first paints", () => {
    const root = createTestRoot()
    root.renderer.clockPause()
    const rows = plain(8).map((_, i) =>
      i === 5 ? { scrollInitialTarget: "nearest" } : {}
    )
    root.render(<Box rows={rows} />)
    const box = root.renderer.findByTestId("box")!
    // The first frame paints the bounds, the second one reads them.
    root.renderer.flush()
    root.renderer.flush()
    expect(root.renderer.getScrollOffset(box.id)![1]).toBe(-500)
    root.unmount()
  })
})
