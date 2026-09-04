/** Exercises the bidirectional, cursor-paginated virtual chat example. */

import React from 'react'
import { connectTest } from '@gpuix/react/automation'
import { createTestRoot, hasNativeTestRenderer, type TestRenderer } from '@gpuix/react/testing'
import { describe, expect, it, vi } from 'vitest'
import { createFakeMessageApi, InfiniteChatApp } from './infinite-chat'

const describeNative = hasNativeTestRenderer ? describe : describe.skip

const LOADING = 'Loading older messages…'

function isLoading(renderer: TestRenderer) {
  return renderer.getAllText().includes(LOADING)
}

async function waitForIdle(renderer: TestRenderer) {
  await vi.waitFor(() => {
    renderer.flush()
    expect(isLoading(renderer)).toBe(false)
  })
}

function listOf(renderer: TestRenderer) {
  return renderer.findByType('virtual-list')[0]
}

/** Scroll to the edge row, which is the only thing that starts a fetch. */
async function pageTo(renderer: TestRenderer, side: 'previous' | 'next') {
  const list = listOf(renderer)
  renderer.scrollToItem(list.id, side === 'previous' ? 0 : list.children.length - 1)
  await waitForIdle(renderer)
}

describeNative('infinite chat example', () => {
  it('renders one page of Safe MDX messages plus the empty top edge', () => {
    const api = createFakeMessageApi({ messageCount: 48, pageSize: 8, delayMs: 5 })
    const { render, renderer } = createTestRoot({ width: 900, height: 640 })

    render(<InfiniteChatApp api={api} />)

    // 8 messages, plus the leading edge row. The initial page ends at the
    // newest message, so there is no trailing edge.
    expect(listOf(renderer).children).toHaveLength(9)
    expect(renderer.findByTestId('edge-previous')).toBeDefined()
    expect(renderer.findByTestId('edge-next')).toBeUndefined()
    expect(renderer.findByType('markdown')).toHaveLength(0)
  })

  // Identical rows hide both the estimate-then-measure path and the anchor bug
  // below, and they make a fast scroll look frozen.
  it('gives every message different content and a different height', () => {
    const api = createFakeMessageApi({ messageCount: 48, pageSize: 8, delayMs: 5 })
    const sources = api.initialPage().items.map((message) => message.source)

    expect(new Set(sources).size).toBe(sources.length)
    expect(new Set(sources.map((source) => source.length)).size).toBeGreaterThan(4)

    // Seeded, so a page fetched twice is byte-identical and tests are stable.
    expect(createFakeMessageApi({ messageCount: 48, pageSize: 8 }).initialPage().items[0].source)
      .toBe(sources[0])
  })

  it('opens a message route from a Safe MDX link', async () => {
    const api = createFakeMessageApi({ messageCount: 48, pageSize: 8, delayMs: 20 })
    const { render, renderer } = createTestRoot({ width: 900, height: 640 })
    render(<InfiniteChatApp api={api} initialMessageId="message-024" />)

    renderer.scrollToItem(listOf(renderer).id, 4)
    const link = renderer.getPaintedText().find((line) => /^Open message \d+$/.test(line))
    expect(link).toBeDefined()
    const target = `message-${link!.slice('Open message '.length)}`

    const app = await connectTest(renderer)
    try {
      await app.getByText(link!).click()
      expect(renderer.findByTestId('loading-route')).toBeDefined()
      await vi.waitFor(() => {
        renderer.flush()
        expect(renderer.findByTestId('loading-route')).toBeUndefined()
      })

      expect(renderer.getAllText()).toContain(`/messages/${target}`)
      expect(renderer.findByTestId(`message-${target}`)).toBeDefined()
      expect(api.requests[api.requests.length - 1]).toEqual({ direction: 'around', cursor: target })
    } finally {
      await app.close()
    }
  })

  // Nothing is prefetched. The reader reaches real empty space and waits in it,
  // which is what a production client does on a slow connection.
  it('starts a fetch only once the empty edge row is on screen', async () => {
    const api = createFakeMessageApi({ messageCount: 48, pageSize: 8, delayMs: 30 })
    const { render, renderer } = createTestRoot({ width: 900, height: 640 })
    render(<InfiniteChatApp api={api} initialMessageId="message-024" />)

    // Sitting on messages, well short of the edge: no request.
    renderer.scrollToItem(listOf(renderer).id, 4)
    expect(api.requests).toHaveLength(0)
    expect(isLoading(renderer)).toBe(false)

    renderer.scrollToItem(listOf(renderer).id, 0)
    expect(api.requests).toHaveLength(1)
    expect(isLoading(renderer)).toBe(true)
    await waitForIdle(renderer)
  })

  // The spinner lives inside the edge row, so a fetch re-renders the app. The
  // message rows must not be part of that. This does NOT prove `memo` skipped
  // the map; React reconciles by key either way. It catches a remounted list.
  it('keeps the list and its message rows intact while a fetch runs', async () => {
    const api = createFakeMessageApi({ messageCount: 48, pageSize: 8, delayMs: 30 })
    const { render, renderer } = createTestRoot({ width: 900, height: 640 })
    render(<InfiniteChatApp api={api} initialMessageId="message-024" />)

    const list = listOf(renderer)
    const rows = list.children.slice()

    renderer.scrollToItem(list.id, 0)
    expect(isLoading(renderer)).toBe(true)
    expect(listOf(renderer).id).toBe(list.id)
    expect(listOf(renderer).children).toEqual(rows)

    await waitForIdle(renderer)
  })

  /**
   * The pixel-level version of the anchor guarantee. While the reader waits in
   * the void, gpui's anchor IS the void, so the app re-anchors on the message
   * that was under it. Snapping that message to the viewport top (offset 0)
   * still moves it by up to EDGE_HEIGHT pixels; a browser preserves the exact
   * offset. The restore must keep the message at the same pixel.
   */
  it('keeps the message under the reader at the same pixel when an older page lands', async () => {
    const api = createFakeMessageApi({ messageCount: 200, pageSize: 8, delayMs: 20 })
    const { render, renderer } = createTestRoot({ width: 900, height: 640 })
    // The latest page, so the only void is the top one and the wheel below
    // can only ever start a `previous` fetch.
    render(<InfiniteChatApp api={api} />)

    // Wheel toward older messages until the void starts the fetch, then stop.
    let started = false
    for (let tick = 0; tick < 400 && !started; tick++) {
      renderer.nativeSimulateScrollWheel(450, 320, 0, 240)
      renderer.flush()
      started = isLoading(renderer)
    }
    expect(started).toBe(true)
    expect(api.requests[0]).toEqual({ direction: 'previous', cursor: 'message-192' })

    // message-192 is the oldest loaded message, sitting just under the void.
    const row = () => renderer.findByTestId('message-message-192')!
    const before = renderer.getElementBounds(row().id)!
    await waitForIdle(renderer)
    const after = renderer.getElementBounds(row().id)!
    expect(Math.abs(after[1] - before[1])).toBeLessThanOrEqual(1)
  })

  /**
   * The append twin of the pixel test above. A reader waiting at the bottom
   * rests on gpui's at-end sentinel (`logical_scroll_top` is the item count),
   * where a naive restore snaps the first new message to the viewport top and
   * the messages above visibly jump. The void's old top edge must stay put:
   * the new page fills the blank space below the text the reader is on.
   */
  it('keeps the trailing messages at the same pixel when a newer page lands', async () => {
    const api = createFakeMessageApi({ messageCount: 200, pageSize: 8, delayMs: 20 })
    const { render, renderer } = createTestRoot({ width: 900, height: 640 })
    render(<InfiniteChatApp api={api} initialMessageId="message-050" />)

    // Start on content, then wheel toward newer messages until the bottom
    // void starts the fetch, then stop.
    renderer.scrollToItem(listOf(renderer).id, 2)
    let started = false
    for (let tick = 0; tick < 600 && !started; tick++) {
      renderer.nativeSimulateScrollWheel(450, 320, 0, -120)
      renderer.flush()
      started = isLoading(renderer)
    }
    expect(started).toBe(true)
    expect(api.requests[0]).toEqual({ direction: 'next', cursor: 'message-053' })

    // message-053 is the newest loaded message, sitting just above the void.
    const row = () => renderer.findByTestId('message-message-053')!
    const before = renderer.getElementBounds(row().id)!
    await waitForIdle(renderer)
    const after = renderer.getElementBounds(row().id)!
    expect(Math.abs(after[1] - before[1])).toBeLessThanOrEqual(1)

    // The blank space under it became the new page.
    const painted = renderer.getPaintedText().filter((line) => /^message-\d+$/.test(line))
    expect(painted).toContain('message-054')
  })

  /**
   * The mirrored case. When the window is shorter than EDGE_HEIGHT the reader
   * can sit fully inside the bottom void. gpui then anchors on the void, the
   * new page splices in above it, and the reader keeps staring at blank space
   * while the next visibleRange event fetches the following page: the content
   * never reaches them. The page must land where the reader is looking.
   */
  it('lands a newer page inside the void the reader is staring at', async () => {
    const api = createFakeMessageApi({ messageCount: 200, pageSize: 8, delayMs: 20 })
    // 200 - 50px header leaves a viewport shorter than the 160px edge row.
    const { render, renderer } = createTestRoot({ width: 900, height: 200 })
    render(<InfiniteChatApp api={api} initialMessageId="message-050" />)

    const painted = () => renderer.getPaintedText().filter((line) => /^message-\d+$/.test(line))

    // Start on content, then wheel toward newer messages into the void.
    renderer.scrollToItem(listOf(renderer).id, 4)
    let started = false
    for (let tick = 0; tick < 600 && !started; tick++) {
      renderer.nativeSimulateScrollWheel(450, 120, 0, -240)
      renderer.flush()
      started = isLoading(renderer)
    }
    expect(started).toBe(true)

    // Keep wheeling until the void fills the whole window.
    for (let tick = 0; tick < 100 && painted().length > 0; tick++) {
      renderer.nativeSimulateScrollWheel(450, 120, 0, -240)
      renderer.flush()
    }
    expect(painted()).toHaveLength(0)

    await waitForIdle(renderer)

    expect(painted().length).toBeGreaterThan(0)
    expect(api.requests.filter((request) => request.direction === 'next')).toHaveLength(1)
  })

  /**
   * Two things move the reader when an older page lands, and only a real wheel
   * shows either of them.
   *
   * `VirtualListEntry::sync` diffs the child ids with ONE contiguous
   * prefix/suffix splice, so inserting at the front while evicting at the back
   * in the same commit shares no prefix and no suffix: the list is respliced
   * whole and the anchor index names a different message. The app commits the
   * insert and the evict separately.
   *
   * gpui then anchors on the topmost visible logical item, which while the
   * reader waits is the void itself. Older rows splice in directly beneath it,
   * so the anchor holds and the screen under it is replaced anyway. The app
   * puts the message they were reading back at the top.
   */
  it('keeps the reader on the same message when an older page lands', async () => {
    const api = createFakeMessageApi({ messageCount: 200, pageSize: 8, delayMs: 20 })
    const { render, renderer } = createTestRoot({ width: 900, height: 640 })
    render(<InfiniteChatApp api={api} initialMessageId="message-120" />)

    const painted = () => renderer.getPaintedText().filter((line) => /^message-\d+$/.test(line))

    // Wheel toward older messages until the void triggers a fetch, then keep
    // wheeling while it runs: scrolling faster than the network must not move
    // the reader once the page arrives.
    for (let page = 0; page < 5; page++) {
      let started = false
      for (let tick = 0; tick < 400 && !started; tick++) {
        renderer.nativeSimulateScrollWheel(450, 320, 0, 240)
        renderer.flush()
        started = isLoading(renderer)
      }
      expect(started).toBe(true)

      const before = painted()
      expect(before.length).toBeGreaterThan(0)
      await waitForIdle(renderer)

      // Every message they could see is still on screen. The new page and the
      // void are above it, which is the direction they were already going.
      expect(painted()).toEqual(expect.arrayContaining(before))
    }

    // The cache stayed bounded while all of that happened.
    expect(listOf(renderer).children.length).toBeLessThanOrEqual(42)
  })

  it('pages to both ends and stops requesting there', async () => {
    const api = createFakeMessageApi({ messageCount: 48, pageSize: 8, delayMs: 20 })
    const { render, renderer } = createTestRoot({ width: 900, height: 640 })
    render(<InfiniteChatApp api={api} initialMessageId="message-024" />)

    for (let page = 0; page < 6 && renderer.findByTestId('edge-previous'); page++) {
      await pageTo(renderer, 'previous')
    }

    // The edge row is gone, which is how the reader learns there is no more.
    expect(renderer.findByTestId('edge-previous')).toBeUndefined()
    expect(renderer.findByTestId('message-message-000')).toBeDefined()

    const requestsAtStart = api.requests.length
    renderer.scrollToItem(listOf(renderer).id, 0)
    expect(api.requests).toHaveLength(requestsAtStart)

    for (let page = 0; page < 10 && renderer.findByTestId('edge-next'); page++) {
      await pageTo(renderer, 'next')
      // PAGE_CACHE_SIZE pages of 8, plus at most two edge rows.
      expect(listOf(renderer).children.length).toBeLessThanOrEqual(42)
    }

    expect(renderer.findByTestId('edge-next')).toBeUndefined()
    expect(renderer.findByTestId('message-message-047')).toBeDefined()

    const requestsAtEnd = api.requests.length
    renderer.scrollToItem(listOf(renderer).id, listOf(renderer).children.length - 1)
    expect(api.requests).toHaveLength(requestsAtEnd)
  })
})
