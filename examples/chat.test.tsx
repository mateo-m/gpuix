/**
 * Visual tests for the Waku-style chat example.
 *
 * Renders the real app through the GPU test renderer and captures screenshots
 * into `examples/screenshots/`, so the layout can be inspected after a run.
 */

import fs from 'fs'
import path from 'path'
import { fileURLToPath } from 'url'
import React from 'react'
import { beforeAll, describe, expect, it } from 'vitest'
import {
  createTestRoot,
  hasNativeTestRenderer,
  render,
  resetRender,
  TestRenderer,
} from '@gpuix/react'
import { connectTest } from '@gpuix/react/automation'
import { ChatApp, SafeMdxTranscript } from './chat'

const describeNative = hasNativeTestRenderer ? describe : describe.skip
const SHOTS = path.resolve(path.dirname(fileURLToPath(import.meta.url)), 'screenshots')

beforeAll(() => {
  fs.mkdirSync(SHOTS, { recursive: true })
})

describeNative('chat example', () => {
  it('renders safe-mdx through GPUIX primitives', () => {
    const { render, renderer } = createTestRoot()
    render(<SafeMdxTranscript />)

    const screenshot = path.join(SHOTS, 'chat-safe-mdx.png')
    renderer.captureScreenshot(screenshot)

    expect(renderer.findByType('markdown')).toHaveLength(0)
    expect(renderer.findByType('code')).toHaveLength(1)
    expect(fs.statSync(screenshot).size).toBeGreaterThan(0)
    expect(renderer.getPaintedText()).toMatchInlineSnapshot(`
      [
        "Can Markdown be composed as normal React elements instead?",
        "React-composed Markdown",
        "This message uses ",
        "safe-mdx",
        ", ",
        "styled spans",
        ", ",
        "deleted text",
        ", an
      ",
        "inline code value",
        ", and ",
        "a link",
        ".",
        "The parser runs in TypeScript. Every Markdown node becomes a normal React component.",
        "GPUIX renders the resulting ",
        "div",
        ", ",
        "text",
        ", and ",
        "code",
        " tree.",
        "•",
        "nested ",
        "inline formatting",
        " inside a list",
        "•",
        "a second item with a long sentence that must wrap without leaving the transcript column",
        "✓",
        "a GFM task item",
        "Path",
        "Renderer",
        "Native Markdown element",
        "Host nodes",
        "Scroll",
        "When to use",
        "safe-mdx",
        "React tree of div and text",
        "no",
        "many",
        "overflow-x on this grid",
        "Custom MDX components and React state inside a message",
        "pulldown-cmark",
        "one native markdown node",
        "yes",
        "one",
        "overflow-x inside Rust",
        "Default chat transcript. Cheapest paint.",
        "grid table",
        "one CSS grid of cells",
        "no",
        "one per cell",
        "overflow-x on the flex parent",
        "Wide comparison tables that must stay readable",
        "typescript",
        "1",
        "const tree = mdxParse(source)",
        "2",
        "return <SafeMdxRenderer markdown={source} mdast={tree} />",
        "Custom MDX component",
        "MDX components also map to ordinary GPUIX React components.",
      ]
    `)
  })

  it('renders the sidebar, transcript and composer', () => {
    const { render, renderer } = createTestRoot()
    render(<ChatApp />)

    const transcript = renderer.findByType('virtual-list')[0]
    expect(transcript).toBeDefined()
    // Every row declares `width: "100%"`, and the retained style now reports
    // back what was written rather than a number it read the percentage as.
    expect(
      transcript.children.map((id) => renderer.getElement(id)?.style.width)
    ).toEqual(Array(transcript.children.length).fill('100%'))

    const painted = renderer.getPaintedText()

    expect(painted).toContain('New Task')
    expect(painted).toContain('Search')
    expect(painted).toContain('Yesterday')
    expect(painted).toContain('give me a quick overview')
    expect(painted).toContain('Do anything...')
    const icons = renderer.findByType('svg')
    expect(icons.length).toBeGreaterThan(8)
    expect(
      icons.every((icon) => String(icon.customProps?.src ?? '').includes('svg'))
    ).toBe(true)

    expect(painted).toContain('DeepSeek V4 Flash')
    expect(painted).toContain('Local')
    expect(painted.some((line) => line.includes('control plane for local coding agents'))).toBe(
      true
    )
    expect(renderer.findByType('markdown').length).toBeGreaterThan(0)
  })

  it('scrolls the transcript past the first turn', () => {
    const { render, renderer } = createTestRoot()
    render(<ChatApp />)

    expect(renderer.getPaintedText()).not.toContain('Do I get hot reload')

    const transcript = renderer.findByType('virtual-list')[0]
    renderer.nativeSimulateScrollWheel(700, 400, 0, -1400)
    renderer.scrollToItem(transcript.id, 23)
    renderer.flush()

    expect(renderer.getPaintedText()).toContain('Which models should I wire up?')
    expect(
      renderer.getPaintedText().some((line) => line.includes('control plane for local coding agents'))
    ).toBe(false)
  })

  it('selects message text but never sidebar titles', () => {
    const { render, renderer } = createTestRoot()
    render(<ChatApp />)

    expect(renderer.dragSelect(30, 300, 240, 320)).toBeNull()

    const selected = renderer.dragSelect(980, 86, 1110, 86)
    expect(selected).not.toBeNull()
    expect(selected).not.toContain('Native SDK vs GPUI comparison')
  })

  it('opens the model picker and changes the selected model', () => {
    const { render, renderer } = createTestRoot()
    render(<ChatApp />)

    expect(renderer.getPaintedText()).toContain('DeepSeek V4 Flash')
    expect(renderer.getPaintedText()).not.toContain('Claude Opus 4.6')

    renderer.nativeSimulateClick(480, 724)
    renderer.flush()
    expect(renderer.getPaintedText()).toContain('Claude Opus 4.6')

    const shot = path.join(SHOTS, 'chat-model-picker.png')
    renderer.captureScreenshot(shot)
    expect(fs.statSync(shot).size).toBeGreaterThan(0)
  })

  it('types into the composer and clears on enter', () => {
    const { render, renderer } = createTestRoot()
    render(<ChatApp />)

    const textarea = renderer.findByType('textarea')[0]
    expect(textarea).toBeDefined()
    renderer.nativeSimulateKeystrokes(textarea.id, 'h e l l o')
    expect(renderer.getPaintedText()).toContain('hello')

    renderer.nativeSimulateKeystrokes(textarea.id, 'enter')
    expect(renderer.getPaintedText()).toContain('Do anything...')

    const transcript = renderer.findByType('virtual-list')[0]
    renderer.scrollToItem(transcript.id, 24)
    renderer.flush()
    expect(renderer.getPaintedText()).toContain('hello')
  })

  it('stays painted after render() remounts the tree', async () => {
    resetRender()
    const renderer = new TestRenderer()
    const before = path.join(SHOTS, 'chat-remount-before.png')
    const after = path.join(SHOTS, 'chat-remount-after.png')

    render(<ChatApp />, { renderer, width: 1180, height: 820 })
    renderer.flush()
    renderer.captureScreenshot(before)
    expect(renderer.getPaintedText()).toContain('New Task')
    expect(renderer.getPaintedText()).toContain('give me a quick overview')

    render(<ChatApp />, { renderer, width: 1180, height: 820 })
    renderer.flush()
    await new Promise((resolve) => setTimeout(resolve, 50))
    renderer.flush()
    renderer.captureScreenshot(after)

    expect(renderer.getRoot()).toBeDefined()
    expect(renderer.getPaintedText()).toContain('New Task')
    expect(renderer.getPaintedText()).toContain('give me a quick overview')
    expect(
      renderer.getPaintedText().some((line) => line.includes('control plane for local coding agents'))
    ).toBe(true)
    expect(fs.statSync(after).size).toBeGreaterThan(0)
  }, 20_000)

  it('keeps transcript row ids when the sidebar collapses', async () => {
    const { render, renderer } = createTestRoot()
    render(<ChatApp turnCount={80} />)
    const before = renderer.findByType('virtual-list')[0]?.children.slice() ?? []
    expect(before.length).toBeGreaterThan(0)
    expect(before.length).toBeLessThan(80)

    const app = await connectTest(renderer)
    await app.getByTestId('sidebar-collapse').click()

    expect(renderer.findByType('virtual-list')[0]?.children).toEqual(before)
    expect(await app.getByTestId('sidebar-expand').count()).toBe(1)
  })

  it('captures deterministic sidebar motion frames', async () => {
    const top = path.join(SHOTS, 'chat-top.png')
    const transitioning = path.join(SHOTS, 'chat-sidebar-transition.png')
    const collapsed = path.join(SHOTS, 'chat-sidebar-collapsed.png')

    const { render, renderer } = createTestRoot()
    render(<ChatApp />)
    renderer.captureScreenshot(top)

    const app = await connectTest(renderer)
    const startedAt = await app.clock.pause()
    await app.getByTestId('sidebar-collapse').click()
    await app.clock.set(startedAt + 100)
    await app.screenshot({ path: transitioning })
    await app.clock.set(startedAt + 200)
    await app.screenshot({ path: collapsed })
    await app.clock.resume()

    for (const shot of [top, transitioning, collapsed]) {
      expect(fs.existsSync(shot)).toBe(true)
      expect(fs.statSync(shot).size).toBeGreaterThan(0)
    }
  }, 15_000)
})
