/**
 * Timeline performance regression. Times mount, wheel pan, and drag move.
 *
 * The question this answers: does a pannable editing surface stay cheap when
 * React owns the scroll offset?
 *
 * **Every measured sample must include a `flush()`.** A pannable surface has no
 * virtualizer, so GPUI rebuilds and lays out every retained child each frame.
 * `dispatchScrollWheel()` alone measures the React update and hides that. The
 * first version of this file did exactly that and reported 0.6ms for the case
 * that actually costs about 100ms.
 *
 * That is the whole point of the cull-on / cull-off pair: `memo(ClipLayer)`
 * removes the React work, and only culling removes the GPUI work.
 *
 * Excluded from `bun run test` by the `*.perf.test.tsx` glob, because these
 * budgets assume an unclamped M-series CPU.
 *
 * THROTTLE=utility|background re-execs under taskpolicy -c and only logs.
 */

import React from 'react'
import { describe, expect, it } from 'vitest'
import {
  createTestRoot,
  hasNativeTestRenderer,
  readMacCpuThrottle,
  type TestRoot,
} from '@gpuix/react/testing'
import { TimelineApp } from './timeline'
import { createProject } from './timeline-data'

const describeNative = hasNativeTestRenderer ? describe : describe.skip
const throttle = readMacCpuThrottle()

const WIDTH = 1280
const HEIGHT = 800
const TRACK_COUNT = 24
const DURATION = 900
const WARMUP = 5
const SAMPLES = 40
const WHEEL_AT = { x: 700, y: 600 }

const BUDGET = {
  mountMs: 400,
  // Pan and drag samples both include one full GPUI paint, so they are
  // comparable. A drag also runs the snap search over every clip.
  panP95Ms: 18,
  panMaxMs: 34,
  dragP95Ms: 22,
  dragMaxMs: 40,
  // Culling off is the control, not a supported configuration: GPUI lays out
  // all 3,200 clips every frame. The budget only has to catch an order of
  // magnitude, so that the cull-on numbers cannot quietly become this.
  noCullP95Ms: 400,
  noCullMaxMs: 600,
}

function percentile(sorted: number[], p: number): number {
  if (sorted.length === 0) return 0
  const index = Math.min(sorted.length - 1, Math.ceil((p / 100) * sorted.length) - 1)
  return sorted[Math.max(0, index)]!
}

function report(label: string, samples: number[]) {
  const sorted = [...samples].sort((a, b) => a - b)
  const stats = {
    n: samples.length,
    p50: percentile(sorted, 50),
    p95: percentile(sorted, 95),
    max: sorted.at(-1) ?? 0,
  }
  console.log(
    `[timeline.perf] ${label} throttle=${throttle ?? 'off'} n=${stats.n} ` +
      `p50=${stats.p50.toFixed(2)}ms p95=${stats.p95.toFixed(2)}ms ` +
      `max=${stats.max.toFixed(2)}ms`
  )
  return stats
}

function expectBudget(args: {
  label: string
  samples: number[]
  p95Max: number
  maxMax: number
}) {
  const stats = report(args.label, args.samples)
  if (throttle) return
  expect(stats.p95, `${args.label} p95 exceeds ${args.p95Max}ms`).toBeLessThan(
    args.p95Max
  )
  expect(stats.max, `${args.label} max exceeds ${args.maxMax}ms`).toBeLessThan(
    args.maxMax
  )
}

function mount(cull: boolean): TestRoot {
  const root = createTestRoot({ width: WIDTH, height: HEIGHT })
  root.render(
    <TimelineApp
      trackCount={TRACK_COUNT}
      durationSeconds={DURATION}
      viewportWidth={WIDTH}
      viewportHeight={HEIGHT}
      cull={cull}
    />
  )
  return root
}

/** Wheel deltas that keep the pan inside the content on both axes. */
function panSamples(root: TestRoot): number[] {
  const samples: number[] = []
  for (let index = 0; index < WARMUP + SAMPLES; index += 1) {
    // Alternate direction so the pan never parks against a clamp, which would
    // measure a no-op render instead of a real one.
    const direction = Math.floor(index / 8) % 2 === 0 ? -1 : 1
    const started = performance.now()
    root.renderer.dispatchScrollWheel(
      WHEEL_AT.x,
      WHEEL_AT.y,
      direction * 24,
      direction * 12
    )
    // The wheel only marks the view dirty. Without this the sample is the
    // React update and none of the GPUI build, layout or paint that follows.
    root.renderer.flush()
    const elapsed = performance.now() - started
    if (index >= WARMUP) samples.push(elapsed)
  }
  return samples
}

describeNative('timeline performance', () => {
  it('mounts a large project', () => {
    const project = createProject({ trackCount: TRACK_COUNT, durationSeconds: DURATION })
    const started = performance.now()
    const root = mount(true)
    root.renderer.flush()
    const elapsed = performance.now() - started
    console.log(
      `[timeline.perf] mount throttle=${throttle ?? 'off'} ` +
        `tracks=${project.tracks.length} clips=${project.clips.length} ` +
        `ms=${elapsed.toFixed(1)}`
    )
    if (!throttle) expect(elapsed).toBeLessThan(BUDGET.mountMs)
  })

  it('pans with culling on', () => {
    const root = mount(true)
    root.renderer.flush()
    expectBudget({
      label: 'pan cull=on',
      samples: panSamples(root),
      p95Max: BUDGET.panP95Ms,
      maxMax: BUDGET.panMaxMs,
    })
  })

  it('pans with culling off', () => {
    // `memo(ClipLayer)` skips the whole subtree, so the wheel changes three
    // styles and costs almost no React time. GPUI still builds and lays out
    // every retained clip, so the draw is an order of magnitude worse. This is
    // why a pannable surface has to cull: memo fixes the JS half only.
    const root = mount(false)
    root.renderer.flush()
    expectBudget({
      label: 'pan cull=off',
      samples: panSamples(root),
      p95Max: BUDGET.noCullP95Ms,
      maxMax: BUDGET.noCullMaxMs,
    })
  })

  it('drags a clip across the timeline', () => {
    const root = mount(true)
    root.renderer.flush()

    const clip = root.renderer
      .findByType('div')
      .map((element) => element)
      .find((element) => String(element.testId ?? '').startsWith('clip-clip-'))
    expect(clip, 'no clip found to drag').toBeTruthy()
    const bounds = root.renderer.getElementBounds(clip!.id)
    expect(bounds, 'clip was never painted').toBeTruthy()

    const startX = bounds![0] + bounds![2] / 2
    const startY = bounds![1] + bounds![3] / 2
    root.renderer.nativeSimulateMouseDown(startX, startY)

    const samples: number[] = []
    for (let index = 0; index < WARMUP + SAMPLES; index += 1) {
      const started = performance.now()
      // One flush per sample, same as the pan, so the two are comparable.
      root.renderer.dispatchMouseMove(startX + index * 3, startY, 0)
      root.renderer.flush()
      const elapsed = performance.now() - started
      if (index >= WARMUP) samples.push(elapsed)
    }
    root.renderer.nativeSimulateMouseUp(startX + 120, startY)

    expectBudget({
      label: 'drag move',
      samples,
      p95Max: BUDGET.dragP95Ms,
      maxMax: BUDGET.dragMaxMs,
    })
  })
})
