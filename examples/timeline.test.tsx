/**
 * Behaviour tests for the video-editor timeline example.
 *
 * The app paints its own state into three `<text testId>` readouts, so every
 * assertion here is an exact number rather than a screenshot comparison.
 * Screenshots are still written into `examples/screenshots/` for inspection.
 *
 * Geometry is fixed by passing an explicit viewport size, so a window resize
 * cannot move a hit point.
 *
 * That size is the one the platform GRANTED, never the one requested. The
 * offscreen test window is a real window, so a smaller display clamps it: a
 * 1024x768 CI runner handed back 1024x642 and every hit point derived from
 * 800 landed below the window, which reads as a broken drag rather than a
 * clamped window.
 */

import fs from 'fs'
import path from 'path'
import { fileURLToPath } from 'url'
import React from 'react'
import { beforeAll, describe, expect, it } from 'vitest'
import { connectTest, type App } from '@gpuix/react/automation'
import { createTestRoot, hasNativeTestRenderer } from '@gpuix/react/testing'
import { TimelineApp } from './timeline'
import { createProject, type Clip } from './timeline-data'

const describeNative = hasNativeTestRenderer ? describe : describe.skip
const SHOTS = path.resolve(path.dirname(fileURLToPath(import.meta.url)), 'screenshots')

const REQUESTED = { width: 1280, height: 800 }

// One probe window, so every constant below is in the coordinate space the
// tests actually click in.
const { width: WIDTH, height: HEIGHT } = hasNativeTestRenderer
  ? createTestRoot(REQUESTED).renderer.getWindowSize()
  : REQUESTED

// Enough tracks that the content is taller than the body, so a vertical pan
// has somewhere to go.
const TRACK_COUNT = 12

// Must mirror the constants in timeline.tsx. A test that guessed these would
// silently start clicking on empty space after a layout tweak.
const HEADER_WIDTH = 220
const RULER_HEIGHT = 30
const FOOTER_HEIGHT = 34
const PX_PER_SECOND = 24

const project = createProject({ trackCount: TRACK_COUNT })

function trackHeight(kind: string): number {
  return kind === 'audio' ? 56 : kind === 'caption' ? 30 : 34
}

const contentHeight = project.tracks.reduce(
  (total, track) => total + trackHeight(track.kind),
  0
)
const bodyHeight = Math.max(120, Math.min(contentHeight, Math.round(HEIGHT * 0.42)))
const panelHeight = RULER_HEIGHT + bodyHeight + FOOTER_HEIGHT
const GRID_TOP = HEIGHT - panelHeight + RULER_HEIGHT
const GRID_CENTER = { x: HEADER_WIDTH + 400, y: GRID_TOP + Math.round(bodyHeight / 2) }
const RULER_POINT = { x: HEADER_WIDTH + 400, y: HEIGHT - panelHeight + RULER_HEIGHT / 2 }

/** Window y of the vertical middle of a track row, before any vertical pan. */
function rowCenterY(trackId: string): number {
  let top = 0
  for (const track of project.tracks) {
    const height = trackHeight(track.kind)
    if (track.id === trackId) return GRID_TOP + top + height / 2
    top += height
  }
  throw new Error(`Unknown track ${trackId}`)
}

/** A window point over a track row where no clip is painted. */
function emptyPointOn(trackId: string): { x: number; y: number } {
  const clips = project.clips
    .filter((clip) => clip.trackId === trackId)
    .sort((a, b) => a.start - b.start)
  for (let index = 0; index + 1 < clips.length; index += 1) {
    const gapStart = clips[index].start + clips[index].duration
    const gapEnd = clips[index + 1].start
    const middle = (gapStart + gapEnd) / 2
    if ((gapEnd - gapStart) * PX_PER_SECOND > 12 && middle * PX_PER_SECOND < 900) {
      return { x: HEADER_WIDTH + middle * PX_PER_SECOND, y: rowCenterY(trackId) }
    }
  }
  throw new Error(`No visible gap on ${trackId}`)
}

function mount(props: Partial<React.ComponentProps<typeof TimelineApp>> = {}) {
  const root = createTestRoot({ width: WIDTH, height: HEIGHT })
  root.render(
    <TimelineApp
      trackCount={TRACK_COUNT}
      viewportWidth={WIDTH}
      viewportHeight={HEIGHT}
      {...props}
    />
  )
  return root
}

async function readout(app: App): Promise<Record<string, string>> {
  const text = await app.getByTestId('readout').textContent()
  return Object.fromEntries(
    text.split(' ').map((pair) => {
      const [key, ...rest] = pair.split('=')
      return [key, rest.join('=')]
    })
  )
}

async function selectionOf(app: App): Promise<{
  id: string
  trackId: string
  start: number
  duration: number
} | null> {
  const text = await app.getByTestId('selection').textContent()
  if (text === 'none') return null
  const [id, trackId, start, duration] = text.split(' ')
  return { id, trackId, start: Number(start), duration: Number(duration) }
}

/** Tracks whose whole row is inside the body height before any vertical pan. */
const VISIBLE_TRACKS = (() => {
  const ids: string[] = []
  let top = 0
  for (const track of project.tracks) {
    const height = trackHeight(track.kind)
    if (top + height <= bodyHeight) ids.push(track.id)
    top += height
  }
  return ids
})()

function hasDraggableClip(trackId: string): boolean {
  return project.clips.some(
    (clip) =>
      clip.trackId === trackId &&
      clip.start > 1 &&
      clip.start < 12 &&
      clip.duration > 3 &&
      clip.duration < 20
  )
}

// The two shape tracks hold one project-wide clip each, so they cannot be
// dragged apart. Pick the first two visible tracks that hold real clips.
const [TOP_TRACK, SECOND_TRACK] = (() => {
  const usable = VISIBLE_TRACKS.filter(hasDraggableClip)
  const first = usable.find(
    (id) => usable.includes(VISIBLE_TRACKS[VISIBLE_TRACKS.indexOf(id) + 1])
  )
  if (!first) throw new Error('No adjacent pair of usable tracks')
  return [first, VISIBLE_TRACKS[VISIBLE_TRACKS.indexOf(first) + 1]]
})()

/** A clip on a visible track, wide enough for trim handles and on screen. */
function pickClip(trackId: string): Clip {
  const clip = project.clips.find(
    (candidate) =>
      candidate.trackId === trackId &&
      candidate.start > 1 &&
      candidate.start < 12 &&
      candidate.duration > 3 &&
      candidate.duration < 20
  )
  if (!clip) throw new Error(`No suitable clip on ${trackId}`)
  return clip
}

beforeAll(() => {
  fs.mkdirSync(SHOTS, { recursive: true })
})

describeNative('timeline example', () => {
  it('paints the ruler, the track column and the clips', async () => {
    const root = mount()
    const app = await connectTest(root.renderer)

    root.renderer.captureScreenshot(path.join(SHOTS, 'timeline.png'))

    const painted = root.renderer.getPaintedText()
    expect(painted).toContain('Track 1')
    expect(painted).toContain('Caption')
    expect(painted).toContain('Audio 1')
    // Ruler labels are seconds at the current zoom.
    expect(painted).toContain('0')
    expect(await readout(app)).toMatchObject({ x: '0', y: '0', pps: '24.00' })
    await app.close()
  })

  it('pans on both axes from one wheel listener', async () => {
    const root = mount()
    const app = await connectTest(root.renderer)

    await app.mouse.wheel(GRID_CENTER, 0, -60)
    expect(await readout(app)).toMatchObject({ x: '0', y: '60' })

    await app.mouse.wheel(GRID_CENTER, -140, 0)
    expect(await readout(app)).toMatchObject({ x: '140', y: '60' })

    // shift swaps the axis, so a vertical wheel pans horizontally.
    await app.mouse.wheel(GRID_CENTER, 0, -35, { modifiers: 'shift' })
    expect(await readout(app)).toMatchObject({ x: '175', y: '60' })
    await app.close()
  })

  it('keeps the frozen ruler and track column aligned with the grid', async () => {
    const root = mount()
    const app = await connectTest(root.renderer)

    const clip = pickClip(TOP_TRACK)
    const before = await app.getByTestId(`clip-${clip.id}`).bounds()
    const headerBefore = await app.getByTestId(`track-header-${TOP_TRACK}`).bounds()
    const playheadBefore = await app.getByTestId('timeline-playhead').bounds()

    await app.mouse.wheel(GRID_CENTER, -120, -40)

    const after = await app.getByTestId(`clip-${clip.id}`).bounds()
    const headerAfter = await app.getByTestId(`track-header-${TOP_TRACK}`).bounds()
    const playheadAfter = await app.getByTestId('timeline-playhead').bounds()

    // The playhead lives in the ruler and the clip lives in the grid. They
    // moved by the same pixels in the same frame, which is the whole reason
    // React owns the offset instead of a native scroll container.
    expect(Math.round(before.x - after.x)).toBe(120)
    expect(Math.round(playheadBefore.x - playheadAfter.x)).toBe(120)
    expect(Math.round(before.y - after.y)).toBe(40)
    expect(Math.round(headerBefore.y - headerAfter.y)).toBe(40)
    await app.close()
  })

  it('clamps the pan at both ends of the content', async () => {
    const root = mount()
    const app = await connectTest(root.renderer)

    await app.mouse.wheel(GRID_CENTER, 900, 900)
    expect(await readout(app)).toMatchObject({ x: '0', y: '0' })

    await app.mouse.wheel(GRID_CENTER, -99_999, -99_999)
    const maxed = await readout(app)
    const contentWidth = project.durationSeconds * PX_PER_SECOND
    expect(Number(maxed.x)).toBe(Math.round(contentWidth - (WIDTH - HEADER_WIDTH)))
    expect(Number(maxed.y)).toBe(Math.round(contentHeight - bodyHeight))
    await app.close()
  })

  it('scrolls the media bin natively on both axes', async () => {
    const root = mount()
    const app = await connectTest(root.renderer)

    const bin = await app.getByTestId('media-bin').element()
    expect(root.renderer.getScrollOffset(bin.id)).toEqual([0, 0])

    await app.getByTestId('media-bin').wheel(-60, -40)

    const offset = root.renderer.getScrollOffset(bin.id)
    expect(offset).not.toBeNull()
    expect(offset![0]).toBeLessThan(0)
    expect(offset![1]).toBeLessThan(0)
    await app.close()
  })

  it('moves a clip in time and reports one dragstart and one dragend', async () => {
    const root = mount()
    const app = await connectTest(root.renderer)

    const clip = pickClip(TOP_TRACK)
    await app.getByTestId(`clip-${clip.id}`).dragBy(120, 0, { steps: 6 })

    const moved = await selectionOf(app)
    expect(moved?.id).toBe(clip.id)
    expect(moved?.trackId).toBe(TOP_TRACK)
    // 120px at 24 px/s is 5 seconds. Snapping may pull it up to SNAP_PX.
    expect(moved!.start).toBeCloseTo(clip.start + 5, 0)
    expect(moved!.duration).toBeCloseTo(clip.duration, 1)

    const events = await app.getByTestId('events').textContent()
    expect(events).toBe(`dragstart:move:${clip.id} dragend:move`)
    await app.close()
  })

  it('moves a clip to another track when the drag goes down a row', async () => {
    const root = mount()
    const app = await connectTest(root.renderer)

    const clip = pickClip(SECOND_TRACK)
    const rows = project.tracks.map((track) => track.id)
    const from = rows.indexOf(SECOND_TRACK)

    await app.getByTestId(`clip-${clip.id}`).dragBy(0, 34, { steps: 4 })

    const moved = await selectionOf(app)
    expect(moved?.trackId).toBe(rows[from + 1])
    expect(moved!.start).toBeCloseTo(clip.start, 1)
    await app.close()
  })

  it('keeps a horizontal drag on the same track', async () => {
    // The track used to come from an absolute window position, so a wrong
    // viewport size dropped every dragged clip onto the last row.
    const root = mount()
    const app = await connectTest(root.renderer)

    const clip = pickClip(TOP_TRACK)
    await app.getByTestId(`clip-${clip.id}`).dragBy(90, 0, { steps: 4 })

    expect((await selectionOf(app))?.trackId).toBe(TOP_TRACK)
    await app.close()
  })

  it('drags correctly when the app measures the window itself', async () => {
    // The live app passes no viewport props, so it reads `useWindowSize()`.
    // This is the path that broke: the renderer answered a hardcoded 800x600.
    const root = createTestRoot({ width: WIDTH, height: HEIGHT })
    root.render(<TimelineApp trackCount={TRACK_COUNT} />)
    const app = await connectTest(root.renderer)

    expect(root.renderer.getWindowSize()).toEqual({ width: WIDTH, height: HEIGHT })

    const clip = pickClip(TOP_TRACK)
    await app.getByTestId(`clip-${clip.id}`).dragBy(72, 0, { steps: 4 })

    const moved = await selectionOf(app)
    expect(moved?.trackId).toBe(TOP_TRACK)
    expect(moved!.start).toBeCloseTo(clip.start + 3, 0)
    await app.close()
  })

  it('trims the start edge without moving the end', async () => {
    const root = mount()
    const app = await connectTest(root.renderer)

    const clip = pickClip(TOP_TRACK)
    await app.getByTestId(`clip-${clip.id}-trim-start`).dragBy(48, 0, { steps: 4 })

    const trimmed = await selectionOf(app)
    expect(trimmed!.start).toBeCloseTo(clip.start + 2, 0)
    expect(trimmed!.start + trimmed!.duration).toBeCloseTo(clip.start + clip.duration, 1)

    const events = await app.getByTestId('events').textContent()
    expect(events).toBe(`dragstart:trim-start:${clip.id} dragend:trim-start`)
    await app.close()
  })

  it('trims the end edge without moving the start', async () => {
    const root = mount()
    const app = await connectTest(root.renderer)

    const clip = pickClip(TOP_TRACK)
    await app.getByTestId(`clip-${clip.id}-trim-end`).dragBy(48, 0, { steps: 4 })

    const trimmed = await selectionOf(app)
    expect(trimmed!.start).toBeCloseTo(clip.start, 1)
    expect(trimmed!.duration).toBeCloseTo(clip.duration + 2, 0)
    await app.close()
  })

  it('snaps a dragged edge onto a neighbour edge', async () => {
    const root = mount()
    const app = await connectTest(root.renderer)

    const clip = pickClip(TOP_TRACK)
    const neighbour = project.clips.find(
      (candidate) =>
        candidate.trackId === TOP_TRACK && candidate.start > clip.start + clip.duration
    )!
    // Land 4px short of the neighbour's start. SNAP_PX is 6, so the edge
    // must jump flush instead of leaving a sliver of a gap.
    const wantedEnd = neighbour.start - 4 / PX_PER_SECOND
    const deltaPx = (wantedEnd - (clip.start + clip.duration)) * PX_PER_SECOND
    await app.getByTestId(`clip-${clip.id}-trim-end`).dragBy(deltaPx, 0, { steps: 6 })

    const trimmed = await selectionOf(app)
    expect(trimmed!.start + trimmed!.duration).toBeCloseTo(neighbour.start, 2)
    await app.close()
  })

  it('finishes a drag released far outside the window', async () => {
    // Pointer capture, not a full-window overlay hit test, is what keeps the
    // gesture alive here.
    const root = mount()
    const app = await connectTest(root.renderer)

    const clip = pickClip(TOP_TRACK)
    await app.getByTestId(`clip-${clip.id}`).dragTo(
      { x: WIDTH + 600, y: HEIGHT + 400 },
      { steps: 4 }
    )

    const events = await app.getByTestId('events').textContent()
    expect(events).toBe(`dragstart:move:${clip.id} dragend:move`)

    // A clip parked past the last frame could never be reached again.
    const moved = await selectionOf(app)
    expect(moved!.start + moved!.duration).toBeLessThanOrEqual(
      project.durationSeconds + 0.001
    )
    await app.close()
  })

  it('scrubs the playhead by dragging the ruler', async () => {
    const root = mount()
    const app = await connectTest(root.renderer)

    await app.mouse.drag(RULER_POINT, { x: RULER_POINT.x + 240, y: RULER_POINT.y }, {
      steps: 5,
    })

    const after = await readout(app)
    // (400 + 240) px from the grid origin at 24 px/s.
    expect(Number(after.head)).toBeCloseTo(640 / PX_PER_SECOND, 1)

    const events = await app.getByTestId('events').textContent()
    expect(events).toBe('dragstart:scrub dragend:scrub')
    await app.close()
  })

  it('stops the playhead at both ends of the project', async () => {
    const root = mount()
    const app = await connectTest(root.renderer)

    // Past the last frame, not merely past the window edge.
    const pastTheEnd = HEADER_WIDTH + project.durationSeconds * PX_PER_SECOND + 500
    await app.mouse.drag(RULER_POINT, { x: pastTheEnd, y: RULER_POINT.y }, { steps: 3 })
    expect(Number((await readout(app)).head)).toBe(project.durationSeconds)

    await app.mouse.drag(RULER_POINT, { x: -4000, y: RULER_POINT.y }, { steps: 3 })
    expect(Number((await readout(app)).head)).toBe(0)
    await app.close()
  })

  it('zooms under the pointer and keeps that time in place', async () => {
    const root = mount()
    const app = await connectTest(root.renderer)

    const pointer = { x: HEADER_WIDTH + 600, y: GRID_CENTER.y }
    const before = await readout(app)
    const timeUnderPointer =
      (pointer.x - HEADER_WIDTH + Number(before.x)) / Number(before.pps)

    await app.mouse.wheel(pointer, 0, 120, { modifiers: 'cmd' })

    const after = await readout(app)
    expect(Number(after.pps)).toBeGreaterThan(Number(before.pps))
    const timeAfter = (pointer.x - HEADER_WIDTH + Number(after.x)) / Number(after.pps)
    expect(timeAfter).toBeCloseTo(timeUnderPointer, 1)
    await app.close()
  })

  it('marquee-selects the clips it covers', async () => {
    const root = mount()
    const app = await connectTest(root.renderer)

    const empty = emptyPointOn(TOP_TRACK)
    await app.mouse.drag(empty, { x: empty.x + 260, y: empty.y + 60 }, { steps: 6 })

    const events = await app.getByTestId('events').textContent()
    expect(events).toBe('dragstart:marquee dragend:marquee')
    expect(await selectionOf(app)).not.toBeNull()
    await app.close()
  })

  it('culls the clips outside the viewport', async () => {
    const root = mount()
    const app = await connectTest(root.renderer)

    const culled = await readout(app)
    const [shown, total] = culled.clips.split('/').map(Number)
    expect(total).toBe(project.clips.length)
    expect(shown).toBeLessThan(total)

    await app.getByTestId('cull-toggle').click()
    const all = await readout(app)
    expect(all.clips).toBe(`${total}/${total}`)
    await app.close()
  })

  it('collapses a track from its header', async () => {
    const root = mount()
    const app = await connectTest(root.renderer)

    const before = await readout(app)
    const rowBefore = await app.getByTestId(`track-header-${TOP_TRACK}`).bounds()
    const belowBefore = await app.getByTestId(`track-header-${SECOND_TRACK}`).bounds()

    await app.getByTestId(`track-header-${TOP_TRACK}`).click()

    const after = await readout(app)
    const rowAfter = await app.getByTestId(`track-header-${TOP_TRACK}`).bounds()
    const belowAfter = await app.getByTestId(`track-header-${SECOND_TRACK}`).bounds()

    expect(Number(after.clips.split('/')[0])).toBeLessThan(
      Number(before.clips.split('/')[0])
    )
    // The row shrinks and everything under it moves up, so a collapsed track
    // costs a header strip instead of an empty band.
    expect(rowAfter.height).toBeLessThan(rowBefore.height)
    expect(belowAfter.y).toBeLessThan(belowBefore.y)
    await app.close()
  })
})
