# Changelog

## 0.7.0

1. **Add native two-stop linear gradients to `style.background`.** Gradients use GPUI's GPU shaders on every renderer. Angles follow CSS direction, stop positions range from `0` to `1`, rounded corners work as expected, and `hover` or `active` can replace the gradient.

   ```tsx
   <div
     style={{
       background: {
         type: 'linear-gradient',
         angle: 90,
         stops: [
           { color: '#7c3aed', position: 0 },
           { color: '#06b6d4', position: 1 },
         ],
         colorSpace: 'oklab',
       },
       borderRadius: 12,
     }}
   />
   ```

   `colorSpace` accepts `"srgb"` or `"oklab"` and defaults to `"srgb"`. GPUI does not support radial, conic, repeating, or gradients with more than two stops.

2. **Add data URL sources to `<img>`.** Images created or loaded in memory can now render without a temporary file:

   ```tsx
   const src = `data:image/png;base64,${Buffer.from(pngBytes).toString('base64')}`

   <img src={src} style={{ width: 240, height: 140 }} />
   ```

   Base64 and percent-encoded data URLs support PNG, JPEG, WebP, GIF, SVG, BMP, TIFF, ICO, and Netpbm images. Filesystem paths continue to work.

   Fixes https://github.com/remorses/gpuix/issues/35

3. **Applications now own Tab key behavior.** GPUIX no longer binds Tab or Shift+Tab to focus traversal. Both keys reach normal element keyboard handlers and the renderer-level `onKeyDown` callback, so terminals and editors can process them directly.

   Applications that want focus traversal can call the direct GPUI wrappers from the renderer callback:

   ```tsx
   render(<App />, {
     onKeyDown(event, renderer) {
       if (event.key !== 'tab') return
       if (event.modifiers?.shift) renderer.focusPrevious?.()
       else renderer.focusNext?.()
     },
   })
   ```

   `render()` also accepts `onKeyUp`. Element callbacks run before renderer callbacks for raw keys that no GPUI action consumed. Renderer callbacks observe events but cannot cancel native propagation.

   Fixes https://github.com/remorses/gpuix/issues/36

4. **Export `TestGpuixRenderer` consistently on every platform.** macOS and Windows builds with test support construct the GPU renderer. Linux and other builds without test support now throw a clear availability error instead of `TypeError: TestGpuixRenderer is not a constructor`.

   ```ts
   import { TestGpuixRenderer, hasTestGpuixRenderer } from '@gpuix/native'

   if (hasTestGpuixRenderer()) {
     const renderer = new TestGpuixRenderer()
   }
   ```

   `@gpuix/react/testing` exposes the matching `hasNativeTestRenderer` guard. `GpuixRenderer` is unchanged and continues to work on Linux.

   Fixes https://github.com/remorses/gpuix/issues/30

5. **Fix blurry Windows text above 100% display scaling.** The native UI thread requests Per-Monitor V2 DPI awareness before GPUI creates a window, so Windows no longer bitmap-stretches GPUIX apps hosted by Node or Bun. Processes that already have Per-Monitor V2 awareness keep their existing configuration.

   Fixes https://github.com/remorses/gpuix/issues/31

6. **Quit after the last window closes on Windows and Linux.** Closing the final GPUIX window now ends the Node or Bun process, matching macOS. `tick()` reports when the native UI thread has ended, and `render()` exits on that signal.

   Fixes https://github.com/remorses/gpuix/issues/32

7. **Add a macOS frosted-glass window example.** It combines GPUI's native vibrancy backdrop with a transparent titlebar and translucent React surfaces:

   ```tsx
   render(<App />, {
     titlebarTransparent: true,
     windowBackground: 'blurred',
   })
   ```

   Run it from `examples/` with `bun run blurred-window`.

## 0.6.0

1. **Every interactive surface now has a stable GPUI identity.** `hover` and `active` work on `<text>`, `<input>`, `<textarea>`, `<code>`, `<markdown>`, `<diff>`, `<img>`, `<svg>`, and `<anchored>`, not only `<div>`. `<text>` also receives its declared click, mouse, keyboard, focus, and pointer-capture events.

   ```tsx
   <text style={{ padding: 8, hover: { color: '#f38ba8' } }} onClick={select}>
     {label}
   </text>

   <img src={avatar} onClick={openProfile} />
   <anchored side="bottom" onMouseLeave={close}>{items}</anchored>
   ```

   `<img>`, `<svg>`, and `<anchored>` now report painted bounds to automation, so `getByTestId(...).click()` works on them. Animated GIFs retain their frame state and animate. An `active` style no longer needs an unrelated click handler.

   A `<text>` with an opaque `backgroundColor` now takes mouse hits, like an HTML element with a background. The wheel still reaches a scroller behind it. Set `pointerEvents: 'none'` when the label must stay transparent to pointer input.

   Each native renderer now accepts one mounted React root. A second `createRoot(renderer)` throws instead of taking over the same window and event map. Removed text nodes are also freed instead of accumulating for the process lifetime.

2. **Background window launch and live keyboard automation.** `render()` adds `focus` and `show` options. `focus: false` opens behind the current app. `show: false` creates the live React tree without showing a window. `activateWindow()` later reveals and focuses it.

   ```tsx
   render(<App />, {
     title: 'Notes',
     focus: process.env.GPUIX_BACKGROUND !== '1',
   })
   ```

   Agent-driven checks can keep the user's keyboard while still using real GPU paint:

   ```ts
   const app = await launch({
     command: 'bun',
     args: ['app.tsx'],
     env: { GPUIX_BACKGROUND: '1' },
   })

   await app.getByTestId('composer').fill('hello gpuix')
   await app.getByTestId('composer').press('enter')
   ```

   `fill()` and `press()` now use GPUI's live desktop input pipeline. Automation does not need window activation. Linux currently ignores `focus` and `show` because GPUI does not support those flags there.

3. **Pixel-stable bidirectional history with `<virtual-list>`.** `scrollToItem` accepts an offset inside the row, and `getListScrollTop` returns the logical item anchor plus viewport height. Infinite histories can prepend or append a page without moving the message the reader is looking at.

   ```tsx
   renderer.scrollToItem(listId, index, offsetInItem)
   const top = renderer.getListScrollTop(listId)
   // [itemIndex, offsetInItemPx, viewportHeightPx] or null
   ```

    Scroll requests are applied after the next render's child splice. A negative offset can anchor the viewport above the named row, and GPUI resolves it against the newly measured row heights.

    ```text
    before page                            after page arrives
    ┌──────────────────┐                   ┌──────────────────┐
    │ loading row      │                   │ new message 7    │
    ├──────────────────┤                   ├──────────────────┤
    │ message A        │ <--> same pixel   │ message A        │
    │ message B        │                   │ message B        │
    └──────────────────┘                   └──────────────────┘
    ```

   `PublicInstance` is now exported for typed host refs. The new `examples/infinite-chat.tsx` shows delayed cursor pagination in both directions, variable-height Safe MDX rows, bounded page retention, loading voids, stable anchors, and links to messages outside the loaded page. Run it on desktop or in the browser:

   ```bash
   cd examples && bun run infinite-chat
   bun run web # open /infinite
   ```

4. **Custom renderers now implement one atomic `applyBatch(json)` transport.** The separate native mutation methods are removed from `NativeRenderer`. React validates and sends one batch per commit on desktop, web, and in tests.

   ```ts
   const renderer: NativeRenderer = {
     applyBatch(json) {
       return nativeTransport.applyBatch(json)
     },
   }
   ```

   Style and custom-prop payloads are JSON values inside that batch, not nested JSON strings.

5. **The WebGPU examples are available online.** https://gpuix.dev/chat-example/ runs the GPUIX chat example in the browser and shows a lightweight loading state while the Wasm renderer starts. The example now uses GPUIX branding and describes the React-to-GPUI architecture accurately. The bidirectional history example is also available from the browser development server at `/infinite`.

6. **Native releases now ship one prebuilt target per OS.** The package supports the architecture most commonly used on each platform:

   | OS | Target | Renderer |
   | --- | --- | --- |
   | macOS | `aarch64-apple-darwin` | Metal |
   | Linux | `x86_64-unknown-linux-gnu` | Vulkan / wgpu |
   | Windows | `x86_64-pc-windows-msvc` | Direct3D |

   Intel macOS, arm64 Linux, and arm64 Windows no longer have prebuilt packages. The standalone macOS and Linux chat examples now ship as `.tar.gz` archives, which preserve the executable name and mode and reduce the download size. Windows continues to ship an `.exe`.

   ```bash
   tar -xzf example-chat-aarch64-apple-darwin.tar.gz
   ./example-chat-aarch64-apple-darwin
   ```

7. **Text selection and native editing behave consistently across long content.** Selection washes now include the first glyph of each soft-wrapped row. A drag in a virtual list survives anchor-row unmounts, scrolls near the list edge, and stops its timer when the list cannot move further.

   Double-click selects the word under the pointer in `<input>` and `<textarea>`. Triple-click selects the full value. A selected word no longer collapses when the pointer moves, textarea selection autoscrolls past the visible box, and adjacent typing or deletion groups into one undo step for 700 ms with a 200-step history cap.

8. **CRLF source no longer leaves carriage returns in `<code>`.** Rendering, syntax highlighting, selection, and copied text now use normalized LF rows. A trailing newline still produces its final empty row.

   Fixes https://github.com/remorses/gpuix/issues/25

## 0.5.1

1. **Fixed `@gpuix/react/testing` reporting no native renderer when installed from npm.** `hasNativeTestRenderer` was always `false`, so every suite that guards on it skipped silently:

   ```
   Test Files  1 skipped (1)
        Tests  6 skipped (6)
   ```

   `testing.js` ships as ESM and loaded the addon with a bare `require("@gpuix/native")`. Node has no `require` in ESM. Inside this repository vitest inlines the workspace package and provides one, so the suite here always passed; installed from npm the package is externalized and run by Node, the call threw, and the `catch` reported native as missing. It now uses `createRequire(import.meta.url)`.

## 0.5.0

1. **GPUIX apps now run in the browser.** The same React tree renders through GPUI's browser platform on WebGPU, with a WebGL2 fallback. `RetainedTree`, `GpuixView`, styles, and text painting are shared with desktop, so events, selects, comboboxes, inputs, motion, and GPUI scroll gestures all work through a Wasm-to-JavaScript callback bridge. napi-rs stays the desktop bridge; wasm-bindgen starts `gpui_web` in the page.

   ```sh
   bun run web       # build the Wasm if it is missing, then serve with HMR
   bun run web:wasm  # only cargo + wasm-bindgen
   ```

   `bun run web` serves through Bun's frontend dev server, so an edit to a component module is a **React Fast Refresh** update instead of a page reload. `useState` survives, the GPUI canvas is never re-created, and the ~19 MB Wasm module is never re-fetched.

   Browser apps always expose the automation API on `globalThis`, so Playwright or Playwriter can drive them by evaluating in the page:

   ```ts
   await globalThis.gpuix.getByTestId('send').click()
   await globalThis.gpuix.getByTestId('composer').fill('hello')
   await globalThis.gpuix.clock.fastForward(200)
   ```

   Two rules for a browser entry, both learned the hard way: never call `import.meta.hot.accept("./your-app", ...)` in the entry file, because Bun runs the dependency-accept callback even when the module already self-accepted for Fast Refresh and the remount wipes every hook; and keep the `@gpuix/native` import out of any Refresh boundary, because the Wasm half is a singleton and `WebGpuixRenderer::init` fails with `GPUIX web is already running`.

   Browser-specific fixes that landed with it: the debug frame overlay works on Wasm (`render(<App />, { debugFrameOverlay: 'full' })`), macOS browsers get Option+Left / Option+Right word navigation in inputs, diagonal resize cursors point the right way, and GPUI Web's IME bridge is fully hidden so host `input` CSS can no longer unhide a stray text field at the top of the page.

   On iOS, touch pans emit **scroll wheel** events instead of mouse drags, so a swipe scrolls instead of selecting text. A tap does not start a selection, a long press followed by a drag still does, and a text input requests the software keyboard inside its tap handler so the keyboard opens on the composer and closes elsewhere.

2. **New `highlight` prop** — paint a background wash behind matched or explicitly given text ranges. This is what you need for Ctrl+F, agent citations, or LSP diagnostic tints. Put it on any element and it applies to that subtree, so the root searches the window and a container searches only that container.

   ```tsx
   <div highlight={{ query: 'fox' }}>
     <text>the quick brown fox</text>
   </div>
   ```

   It reaches `<text>`, `<code>`, `<markdown>` and `<diff>` with no extra props, because every string GPUIX paints goes through the same funnel.

   `useTextSearch` owns the cursor and the count, so a find bar needs no effects:

   ```tsx
   import { useTextSearch } from '@gpuix/react'

   const search = useTextSearch({ query })

   <text>{search.total === 0 ? 'No results' : `${search.active + 1}/${search.total}`}</text>
   <div onClick={search.previous}><text>↑</text></div>
   <div onClick={search.next}><text>↓</text></div>

   <div {...search.props} style={{ flex: 1 }}>
     <Transcript />
   </div>
   ```

   | field | meaning |
   |---|---|
   | `query` | substring to match, case-insensitive by default |
   | `caseSensitive` | exact case only |
   | `wholeWord` | neither neighbour may be alphanumeric or `_` |
   | `ranges` | explicit `[start, end)` UTF-16 pairs |
   | `color` / `activeColor` | any CSS colour; defaults come from the theme |
   | `activeIndex` | which match gets `activeColor`, for a find cursor |
   | `matchIndexOffset` | matches before this subtree; only for virtualized content |
   | `radius` | corner radius of the wash, default 2 |

   Matches are non-overlapping and leftmost-first, and never cross a line, exactly like browser find. They do cross the several host nodes React creates for one interpolated line: `<text>Hello {name}!</text>` is three host text nodes and `Hello Tommy` still matches. `activeIndex` counts matches in paint order, so it means the same thing whether a match sits in a `<text>` or inside a `<code>` block.

   A `<virtual-list>` never builds off-screen rows, so the app supplies both numbers with the new `findRanges` export, which runs the same algorithm as the native matcher on a string you give it:

   ```tsx
   import { findRanges, useTextSearch } from '@gpuix/react'

   const perRow = useMemo(
     () => rows.map((row) => findRanges({ text: row.text, query }).length),
     [rows, query],
   )

   const search = useTextSearch({
     query,
     matches: {
       total: perRow.reduce((n, count) => n + count, 0),
       indexOffset: perRow.slice(0, windowStart).reduce((n, count) => n + count, 0),
     },
   })
   ```

   A highlight is a quad, so `getPaintedText()` cannot see it. `renderer.getPaintedHighlights()` reports the matched range in UTF-16 units plus the boxes it drew, one per visual row.

   Nothing resolves and nothing paints unless an element declares a `highlight`. A root-scoped query over a 1000-turn chat costs about 2ms per keystroke, and moving the find cursor only re-colours matches it already found.

3. **`<virtual-list>` can mount a window of rows** instead of all of them. The children form retains every child, so the first mount of a long transcript used to pay for every row. Pass `itemCount` with `estimatedItemHeight` and `windowStart`, then render only that slice; native keeps the full logical length for the scrollbar.

   ```tsx
   const WINDOW = 40

   function Transcript({ turns }: { turns: Turn[] }) {
     const [start, setStart] = useState(0)
     const end = Math.min(turns.length, start + WINDOW)
     return (
       <virtual-list
         itemCount={turns.length}
         windowStart={start}
         estimatedItemHeight={220}
         onVisibleRange={(event) =>
           setStart(Math.max(0, Math.floor(event.startIndex ?? 0) - WINDOW / 4))
         }
       >
         {turns.slice(start, end).map((turn) => (
           <ChatTurn key={turn.id} turn={turn} />
         ))}
       </virtual-list>
     )
   }
   ```

   `onVisibleRange` reports `startIndex` and `endIndex` after a scroll. TypeScript now **requires** `estimatedItemHeight` next to `itemCount`, and native ignores `itemCount` without it, because a row React has not mounted would otherwise measure as height 0 and collapse the scrollbar on a jump.

   There is deliberately **no `VirtualList` wrapper component**. The window is application state. A generic wrapper cannot know when to widen its own window, so it silently dropped rows whenever `itemCount` grew without a scroll, which is exactly what a filter does.

4. **A prepended row is visible again.** A list is anchored on a row, not a pixel offset, so inserting rows above the viewport used to slide the viewport down by the height of the new rows. A todo list or a feed that prepends the newest item never showed it.

   ```text
   scrolled down                          pinned to the top
   ┌──────────────────┐                   ┌──────────────────┐
   │ new row  (above) │  ◄── inserted     │ new row          │  ◄── inserted, visible
   ├──────────────────┤                   ├──────────────────┤
   │ ░░ viewport ░░░░ │  stays put        │ ░░ viewport ░░░░ │  follows the insert
   │ ░░░░░░░░░░░░░░░░ │                   │ ░░░░░░░░░░░░░░░░ │
   └──────────────────┘                   └──────────────────┘
   ```

   A browser anchors the same way and suppresses it at `scrollTop: 0`. A top-aligned list that is scrolled to the very top now stays at the top across a mutation. Scrolled anywhere else, the rows under the pointer still do not move. A history pane that loads older pages while the user reads should keep using `alignment="bottom"`.

5. **Automation can drag, hover, wheel, and hold modifiers.** The protocol already carried the events, but nothing exposed them, so a drag or a pan could not be driven from a test.

   ```ts
   await app.getByTestId('clip-7').dragBy(120, 0, { steps: 6 })
   await app.getByTestId('clip-7-trim-end').dragTo(app.getByTestId('clip-8'))
   await app.getByTestId('canvas').wheel(0, 120, { modifiers: 'cmd' })
   await app.getByTestId('row-3').hover()

   await app.mouse.drag({ x: 240, y: 500 }, { x: 700, y: 620 })
   await app.mouse.wheel({ x: 700, y: 600 }, -140, 0)
   await app.mouse.down({ x: 100, y: 100 }, { button: 2 })
   ```

   | Call | What it does |
   |---|---|
   | `locator.hover()` | Moves the pointer to the center, so hover styles and tooltips fire |
   | `locator.wheel(dx, dy)` | One wheel event over the center |
   | `locator.dragBy(dx, dy)` / `locator.dragTo(target)` | Press, travel, release |
   | `locator.center()` | The center of the last painted bounds |
   | `app.mouse.move / down / up / click / wheel / drag` | Raw pointer input in window coordinates |

   A drag sends **interpolated moves**, not one jump, because snapping, live previews, and per-move commits only appear when the pointer travels. Every mouse call takes `modifiers` in the same hyphenated syntax as `press('cmd-a')`, so cmd-wheel zoom, shift-click range selection, and alt-drag duplication are testable. `launch()` can now scroll a live app, `textContent()` concatenates descendants like DOM `textContent`, and `click({ button })` really sends that button. Mouse input, locator bounds, and clock controls also work against a live app on Windows, Linux, and FreeBSD.

6. **`<input>` and `<textarea>` are reachable from the locator API.**

   ```ts
   await app.getByTestId('composer').click()
   await app.getByTestId('composer').fill('hello gpuix')
   ```

   `bounds()` and `click()` threw `Element has no painted bounds`, because a custom element paints itself and the editor never attached the automation bounds tracker; the only workaround was a hard-coded pixel coordinate. In the browser, `fill()` and `press()` threw `GPUI browser input is unavailable`: the client looked for `input[data-gpui-input]`, and [zed-industries/zed#63201](https://github.com/zed-industries/zed/pull/63201) replaced that element with a `<textarea>`. It now matches the attribute alone, exported as `IME_MIRROR_SELECTOR`.

   `<img>`, `<svg>`, `<anchored>`, `<diff>` and `<markdown>` do register painted bounds now too, so a `testId` on `<markdown>` no longer returns null, and `TestRenderer.findByTestId()` resolves it from the retained tree.

7. **macOS apps have a menu bar**, so `⌘Q`, `⌘H`, `⌥⌘H`, `⌘M` and `⌘W` work. GPUI never calls `NSApplication.setMainMenu:`, so `NSApp.mainMenu` stayed nil, macOS painted nothing next to the Apple menu, and there was no way to quit a GPUIX app from the keyboard.

   ```
   Apple    <executable>             Window
            ├ Services               ├ (AppKit window tiling)
            ├ Hide <appName>   ⌘H    ├ Minimize          ⌘M
            ├ Hide Others     ⌥⌘H    ├ Zoom
            ├ Show All               ├ Close Window      ⌘W
            └ Quit <appName>   ⌘Q    └ (open windows)
   ```

   New `appName` window option for the name inside `Hide X` and `Quit X`. It defaults to `title`.

   ```tsx
   render(<App />, { title: 'Todo', appName: 'Todo' })
   ```

   `appName` does **not** set the title of the application menu: macOS takes that from the executable, so `bun app.tsx` shows `bun`. Only a real `.app` bundle changes it. There is no Edit menu on purpose, because AppKit consumes a menu key equivalent before the window sees it and `⌘C` would be taken away from text selection and from `<input>`.

8. **Pointer capture, like HTML.** `onMouseMove` and `onMouseUp` continue after the pointer leaves the element that received `onMouseDown`, matching [`setPointerCapture`](https://developer.mozilla.org/en-US/docs/Web/API/Element/setPointerCapture). A clip, resizer, or slider keeps receiving events without a full-window overlay.

   ```tsx
   <div
     onMouseDown={(e) => startDrag(e)}
     onMouseMove={(e) => moveDrag(e)}
     onMouseUp={() => endDrag()}
   />
   ```

   Capture is armed only when the same node listens for down and move. A node with only `onMouseDown` / `onMouseUp` does not capture, and a release outside still cancels the click, as in the DOM.

9. **The wheel reaches an ancestor scroller from under an absolutely positioned child**, like a browser. Absolute and fixed boxes used **BlockMouse**, which ended the hit test, so a timeline clip or a graph node swallowed a pan gesture. Every filled or positioned `div` now uses **BlockMouseExceptScroll**: clicks and hovers stop, the wheel passes through.

   ```tsx
   <div style={{ position: 'relative' }} onScroll={pan}>
     {/* the wheel over this clip now pans the surface behind it */}
     <div style={{ position: 'absolute', left: 240, width: 120, backgroundColor: '#38455C' }} />
   </div>
   ```

   Set `pointerEvents: "auto"` on the rare element that must swallow the wheel too, such as a modal backdrop. `<anchored>` still occludes by default. An absolutely positioned box still takes clicks with no background, exactly like an empty positioned `div` in a browser, so a wrapper that only carries a scroll offset should set `pointerEvents: "none"`.

10. **`<code>` is a bare surface.** It paints glyphs only: no fill, no border, no radius, no padding, no language header. `style` is the surface, exactly like a `<div>`, so the card look belongs to your app instead of to the element.

    ```tsx
    <code
      code={source}
      language="typescript"
      showLineNumbers
      style={{
        padding: 12,
        borderRadius: 10,
        borderWidth: 1,
        borderColor: '#ffffff1f',
        backgroundColor: '#ffffff09',
      }}
    />
    ```

    `fontFamily`, `fontSize`, `fontWeight`, `lineHeight` and `color` in `style` now beat the theme, and one resolver feeds the div text style, every `TextRun`, and the fixed row height. `style.lineHeight` used to be dropped and clip tall glyphs; it re-sizes the rows instead.

    **Migration:** `showHeader` is gone. Render your own header in a wrapper. Five `theme.metrics` fields only ever styled that card and moved to the `mdCode*` group, where they still tune the `<markdown>` fenced block:

    | Before | After |
    |---|---|
    | `codePaddingX` / `codePaddingY` | `mdCodePaddingX` / `mdCodePaddingY` |
    | `codeRadius` | `mdCodeRadius` |
    | `codeHeaderPaddingY` | `mdCodeHeaderPaddingY` |
    | `codeHeaderTextSize` | `mdCodeHeaderTextSize` |

    `<markdown>` keeps its card: a document renderer owns its layout, a primitive does not.

11. **Syntax highlighting moved from Tree-sitter to Syntect**, with **Oniguruma** on native. There is no Tree-sitter runtime and no per-language C grammar in the binary. Language detection is unchanged (fence tag, then path, then shebang), and token classes stay `HighlightKind` values rather than baked-in colours, so a theme change recolours existing spans without a reparse.

    Syntect compiles every TextMate regex of a grammar the first time that grammar is used, on the frame thread, inside a paint. The engine decides how expensive that is:

    | grammar | fancy-regex, first use | **Oniguruma, first use** |
    | --- | ---: | ---: |
    | TypeScript | ~133ms | **~12ms** |
    | Markdown | ~39ms | **~1.7ms** |
    | Rust | ~17ms | **~1.6ms** |

    The chat example mount for 1000 turns goes from about **240ms to about 130ms**, and the worst scroll frame from about **17ms to about 6ms**. The browser Wasm build keeps the pure-Rust fancy-regex engine, because Oniguruma is a C library. Token colours can shift a little versus Tree-sitter, because Syntect scopes are not the old capture names.

12. **A large mount is 4x faster and the retained tree is 5x smaller.** `applyBatch` used to build a `serde_json::Value` tree, deep-clone every style payload out of it, and parse the clone a second time, so each style was allocated three times. The batch now deserializes straight from its JSON bytes into typed ops, and styles are shared by content: a 10,000-turn chat sends 59,320 `setStyle` ops carrying 90 distinct styles, and every element gets the same `Arc`.

    Measured on a 10,000-turn chat, 221,764 ops:

    | | before | after |
    |---|---:|---:|
    | parse and apply | 127.1 ms | 30.1 ms |
    | heap churn | 900.5 MB | 104.0 MB |
    | allocations | 1,476,196 | 186,090 |
    | retained tree | 224.5 MB | 42.6 MB |
    | bytes per element | 3116 B | 592 B |

    A 5,000-row chat also stops rebuilding virtual-list focus maps on every GPUI frame, so sidebar motion and caret blink no longer pay that cost per tick, and custom-element props are only re-parsed when a retained value actually changes. `getAutomationTree()` stops serializing style, events, and custom props, which took a 5k-row tree from about 110ms to about 22ms, so `getByTestId().click()` is no longer dominated by encoding unused style maps.

13. **The install is about 8x smaller.** `@gpuix/native` packed every platform binary into the main tarball through a `*.node` glob, on top of the six per-platform packages that `optionalDependencies` already resolves. A hello-world install paid for all of it:

    ```
    node_modules                       254M
    ├── @gpuix/native                  185M  ◄── all six binaries, unused
    ├── @gpuix/native-darwin-arm64      23M  ◄── the one that loads
    └── @gpuix/react                   544K
    ```

    Only the loader, the types, the browser entry, and the Wasm build ship in the main package now. Nothing changes at runtime.

14. **Fixed Windows x64 native binding failing to load with `ERR_DLOPEN_FAILED`.** The published `.node` statically imported `TaskDialogIndirect` from comctl32 v6 and `u_strlen` from `icuuc.dll`. Node and Bun do not activate comctl32 v6, so Windows resolved the old comctl32 and `LoadLibrary` failed before any JS ran.

    ```bash
    bun -e "require('@gpuix/native'); console.log('OK')"
    ```

15. **Every CSS `cursor` keyword GPUI can paint is supported**, not just `pointer` and `default`. Resize and drag cursors are what tell a user that an edge can be trimmed or a clip can be grabbed; until now `col-resize` was silently dropped.

    ```tsx
    <div style={{ cursor: 'grab', active: { cursor: 'grabbing' } }} />
    <div style={{ cursor: 'col-resize' }} />
    ```

    | Group | Keywords |
    |---|---|
    | Pointing | `default`, `auto`, `pointer`, `context-menu`, `not-allowed`, `no-drop` |
    | Text | `text`, `vertical-text`, `crosshair` |
    | Dragging | `grab`, `grabbing`, `move`, `all-scroll`, `alias`, `copy` |
    | Resizing | `col-resize`, `row-resize`, `ew-resize`, `ns-resize`, `nwse-resize`, `nesw-resize`, `n-resize`, `e-resize`, `s-resize`, `w-resize`, `ne-resize`, `nw-resize`, `se-resize`, `sw-resize` |

    `cursor` is a typed union, so an editor completes the list. An unlisted keyword is ignored, like any other invalid style value.

16. **Colour strings accept the full csscolorparser grammar** across styles, themes, pseudo-states, selection colours, SVG tint, borders, and shadows: modern RGB/HSL/HWB, HSV, LAB/LCH, OKLab/OKLCH, named colours, `transparent`, alpha, `none`, and limited relative-colour forms. TypeScript types are unchanged.

17. **More style properties reach the GPU.** Per-side border widths and one structured `boxShadow` with offset, blur, spread, and colour are new. Per-corner radii, `flexBasis`, and `alignContent` were already declared in the public style type but never applied; they work now.

18. **New `onAuxClick` for the non-primary mouse buttons.** `onClick` never fired for a right or middle click, so the `isRightClick` field it documents could never be `true` and a context menu had no event to hang on. `onClick` stays primary-only, like the DOM.

    ```tsx
    <div
      onClick={() => select(item)}
      onAuxClick={(event) => {
        if (event.isRightClick) openContextMenu(event.x, event.y)
      }}
    />
    ```

    `onMouseDown` and `onMouseUp` still see every button through `event.button`: `0` left, `1` middle, `2` right.

19. **Window geometry hooks are pull-based.** `useWindowSize()` seeded state with a hardcoded `800x600` and read the renderer once from an effect, so a first read before the platform window had a size kept `800x600` forever, and a resize was never observed at all. It samples every **100 ms** now and only rerenders when the numbers change.

    New `getWindowInsets()` and `useWindowInsets()` report system and software-keyboard geometry, so a composer can stay above the iOS keyboard instead of hiding behind it:

    ```tsx
    const { keyboardTop, keyboardVisible, ime } = useWindowInsets()

    return (
      <div style={{ paddingBottom: ime.bottom }}>
        {keyboardVisible ? `Keyboard starts at ${keyboardTop}px` : 'Keyboard closed'}
      </div>
    )
    ```

    | Field | Meaning |
    | --- | --- |
    | `ime` | Edges covered by the software keyboard |
    | `safeArea` | Edges covered by notches, status bars, home indicators |
    | `effective` | Per-edge max of the two, the region content should avoid |
    | `keyboardTop` | Y coordinate where the keyboard starts |
    | `keyboardVisible` | `ime.bottom > 0` |
    | `visibleHeight` | Window height minus the effective top and bottom |

    Both hooks take the same option, because Safari fires `visualViewport` events in bursts while the keyboard animates and iOS reports stale values on some of them:

    ```tsx
    useWindowInsets()                      // 100ms, the default
    useWindowInsets({ intervalMs: 250 })   // slower
    useWindowInsets({ intervalMs: false }) // read once, never poll
    ```

20. **One diagonal gesture scrolls both axes**, and `position: "fixed"` lays out. `overflow: "scroll"` moved only one axis per wheel event, because GPUI zeroes the smaller of the two deltas by default.

    ```tsx
    <div style={{ width: 260, height: 220, overflow: 'scroll' }}>
      {/* one diagonal swipe now pans on X and Y together */}
    </div>
    ```

    A flex column stretches its children to the cross axis, so rows in a two-axis container still need to state a width, or there is nothing to pan on X. `position: "fixed"` blocked hits like `absolute` but stayed in normal flow, so a box drifted when its siblings changed; it now lays out like `absolute`.

21. **Text selection starts from the empty space before the glyphs.** A press in parent padding, a code gutter, or the empty start of a line clamps to the nearest text on that row, instead of requiring the mouse-down to land inside the tight text box.

    ```
      [padding] hello world
          ^
          press here, drag right  →  "hello world"
    ```

    A press above or below every line still does not start a selection, so a composer or titlebar cannot claim the nearest paragraph, and `userSelect: "none"` now also blocks the start.

    Copying across interpolated text is fixed too. `<text>Hello {name}!</text>` is three painted runs of one line, and selecting across them used to copy them joined with newlines. Runs now carry the parent host element they belong to, so the same selection yields `Hello Tommy!`, while `<code>`, `<diff>` and `<markdown>` keep one line per line.

22. **Fixed `key` on every GPUIX element.** A list built with `.map()` failed to typecheck, so any real app broke on the first `tsc` run:

    ```
    error TS2322: Type '{ key: string; ... }' is not assignable to type 'Props'.
      Property 'key' does not exist on type 'Props'.
    ```

    `key` lives on `Props` now, next to `ref`. It cannot live on `JSX.IntrinsicAttributes`, because TypeScript 5 ignores that member for intrinsic elements. Every element prop type extends `Props`, so `<div>`, `<text>`, `<img>`, `<svg>`, `<canvas>`, `<input>`, `<textarea>`, `<anchored>`, `<code>`, `<diff>`, `<markdown>` and `<virtual-list>` accept `key` again, and so do `motion.div`, Select, Combobox and Tooltip. `@gpuix/react/jsx-dev-runtime` types also match the runtime file now: they re-exported `jsx` and `jsxs` from `react/jsx-dev-runtime`, which exports only `jsxDEV`.

23. **Abandoned concurrent renders stay out of the native mutation queue.** React may throw away a Suspense render. GPUIX waits until commit before it creates native elements, so fallback text paints and abandoned text does not. Unchanged click handlers also stay registered across rerenders, because the whole handler map is no longer cleared before every update.

24. **Each React root owns its event handler map.** Two `createTestRoot()` trees can both start at id `1` without overwriting each other's handlers, and a remount on the same native renderer keeps allocating new ids, so a late event from the old tree cannot hit a new handler that reused id `1`.

    **Migration:** `resetIdCounter()` is gone, and `handleGpuixEvent` needs the renderer that produced the event:

    ```ts
    handleGpuixEvent(event, renderer)
    ```

25. **Native `<markdown>` wraps in flex columns.** A markdown node in a flex row kept its max-content width, so a long paragraph or list item blew past the parent. The root and each text block shrink with `min-width: 0` now, and a fenced block inside `<markdown>` matches `<code>`: long lines scroll on X and leave the vertical wheel on the parent.

    ```tsx
    <div style={{ display: 'flex', flexDirection: 'row', width: 280 }}>
      <div style={{ width: 40, flexShrink: 0 }} />
      <markdown
        source="- a long sentence that must wrap in the remaining column"
        style={{ flexGrow: 1 }}
      />
    </div>
    ```

26. **The test renderer runs on Windows** through GPUI's DirectX renderer: `TestGpuixRenderer`, `createTestRoot()`, native input simulation, and PNG screenshot capture. A live window can call `captureScreenshot()` there too. Linux stays unavailable until GPUI ships its pending wgpu headless renderer.

    The test app also releases its custom elements before the GPUI app goes away. `<input>` keeps a GPUI entity handle, and GPUI's leak detector panics if one outlives the app, which killed the whole vitest worker on Windows after every test in the file had already passed. `createTestRoot({ width, height })` does not size the window on Windows yet: it opens at the display size. Tracked in [#21](https://github.com/remorses/gpuix/issues/21).

    `createTestRoot()` can also size the offscreen window, which was always **1280x800**. That is wide enough to keep a centered `maxWidth` column at its cap, so any layout that only changes below a breakpoint was invisible to the suite.

    ```tsx
    const narrow = createTestRoot({ width: 640, height: 480 })
    createTestRoot({ width: 640 })  // 640 x 800
    createTestRoot({ width: 0 })    // throws: must be a positive, finite number
    ```

27. **New `getDebugFrameOverlayStats()`** so tests and apps can read the same draw times the on-screen overlay shows.

    ```ts
    renderer.resetDebugFrameOverlayStats()
    // ... scroll or click ...
    const stats = renderer.getDebugFrameOverlayStats()
    // stats.currentMs, stats.p90Ms, stats.p99Ms, stats.maxMs, stats.frames, stats.samples
    ```

    `p90Ms` is the overlay **10%** line and `p99Ms` is the **1%** line: the slow tail, not the fast frames.

    On macOS, `THROTTLE=utility` restarts a run under `taskpolicy -c utility`, which pins work to E-cores as an M1/M2 Air CPU proxy. `background` and `maintenance` are slower. GPU and RAM stay on the host machine, so this is not Chrome 6x, and it should not be set in CI.

    ```bash
    THROTTLE=utility bun run test chat.perf.test.tsx
    THROTTLE=utility bun --hot chat.tsx
    ```

28. **A Quickstart and a todo starter app.** The README described the architecture and the mutation protocol before it ever said how to install the packages, and never mentioned `jsxImportSource`, which is required: without it TypeScript falls back to DOM types and `<virtual-list>`, `<markdown>`, `<code>` and `style.hover` all fail.

    ```bash
    bun add @gpuix/react react
    bun add -d @types/react typescript
    ```

    ```json
    { "compilerOptions": { "jsx": "react-jsx", "jsxImportSource": "@gpuix/react" } }
    ```

    `example-app/` is a complete todo app in one file, with scripts already wired:

    | Script | What it does |
    |---|---|
    | `bun run dev` | Desktop app with hot remount |
    | `bun run build` | Standalone binary in `dist/todo` |
    | `bun run web:dev` | Browser build served with isolation headers |
    | `bun run screenshot` | Drives the app through the automation client |
    | `bun run test` | Vitest against the GPU test renderer |
    | `bun run typecheck` | `tsc --noEmit` |

    It shows `<virtual-list>`, a native `<input>`, `motion.div`, tinted `<svg>` icons, native `hover` and `active`, and `testId` automation hooks. Copy the folder, change `@gpuix/react` from `workspace:^` to a version range, and run `bun install`.

29. **A video-editor timeline example**, to answer whether GPUIX can carry a real editing surface. It drags clips between tracks, trims both edges with snapping, scrubs a playhead, marquee-selects, zooms under the pointer, and pans on both axes with a frozen ruler and a frozen track column.

    ```bash
    cd examples && bun --hot timeline.tsx
    ```

    Two patterns in it are worth copying. **React owns the scroll offset**: a native `overflow: "scroll"` grid cannot drive a frozen header, because GPUI moves the grid on the wheel frame and the `onScroll` callback arrives a frame later, so the two tear apart during a fast pan. **A drag needs no overlay**: each clip and trim handle listens for `onMouseDown`, `onMouseMove` and `onMouseUp`, which arms pointer capture, so a release past the window edge still ends the gesture, while an overlay mounted on the press cannot arm anything.

    A pannable surface also has to cull. On 3,259 clips across 26 tracks, one wheel frame costs **7.7ms** culled and **92ms** with `memo` alone.

30. **GitHub releases include a standalone chat example executable for each platform.** No Node, Bun, or Rust install is required.

    ```bash
    chmod +x example-chat-aarch64-apple-darwin
    ./example-chat-aarch64-apple-darwin
    ```

    macOS may block the unsigned binary the first time: right-click the file, choose Open, and confirm. On Windows, download `example-chat-x86_64-pc-windows-msvc.exe` and double-click it.

31. **`@gpuix/native` and `@gpuix/react` are published as Apache-2.0.** Both packages declare `license: Apache-2.0` and ship the license text in the npm tarball. GPUI itself is Apache-2.0, so this matches the native dependency.

32. Smaller fixes: `destroyElement` no longer leaves a dangling child id on the parent or skips invalidating the parent chain, so a cache keyed on the subtree revision cannot serve text that left the tree; automation calls after `close()` are rejected and shutdown is idempotent across the in-process and SSE backends.

## 0.4.0

1. **Native `motion.div` animations** — animate from an initial style to a target style. React sends the targets once. Rust interpolates the presentation style and requests GPUI frames. The React tree is not reconciled on each frame.

   ```tsx
   import { motion } from '@gpuix/react'

   <motion.div
     initial={{ width: 0, opacity: 0 }}
     animate={{ width: 260, opacity: 1 }}
     transition={{ duration: 0.2, ease: 'easeOut' }}
   >
     Sidebar content
   </motion.div>
   ```

   Numeric targets: `width`, `height`, `top`, `right`, `bottom`, `left`, `opacity`, `borderRadius`. Timing uses seconds. `ease` is `"linear"`, `"ease"`, `"easeIn"`, `"easeOut"`, `"easeInOut"`, or a cubic-bezier `[x1, y1, x2, y2]`.

   Set `initial={false}` to mount at the first `animate` target. A running animation can reverse or change target without a jump, because the next transition starts from the current visible value.

   Springs, keyframes, variants, exit transitions, and shared layout animations are not available yet.

2. **Playwright-like automation API** — mark elements with `testId`, then drive them from tests or from another process. Ordinary log lines are ignored.

   ```ts
   import { connectTest } from '@gpuix/react/automation'

   const app = await connectTest(renderer)
   await app.getByTestId('inc').click()
   await app.getByText('Count: 1').waitFor()
   await app.getByTestId('composer').fill('hello gpuix')
   await app.getByTestId('composer').press('enter')
   await app.captureFrames('review/sidebar', [0, 150, 300])
   ```

   Locators: `getByTestId`, `getByText`, `getByType`. `click()` hits the center of the last painted bounds. `fill(text)` replaces the focused editor. `press('enter')` sends one key. `waitFor()` polls until exactly one match exists.

   `app.clock.pause()`, `set(ms)`, and `fastForward(ms)` freeze native motion time so CI can capture the same frames every run.

   A live app listens on stdin when stdin is a pipe, not a TTY. A terminal run is unchanged. `launch({ command, args })` pipes stdin and speaks SSE `data:` lines:

   ```ts
   import { launch } from '@gpuix/react/automation'

   const app = await launch({ command: 'bun', args: ['examples/chat.tsx'] })
   await app.getByTestId('composer').fill('hello')
   await app.screenshot({ path: 'live.png' })
   await app.close()
   ```

## 0.3.0

1. **CSS grid on `div`** — `display: "grid"` plus `gridTemplateColumns` maps to GPUI's Taffy grid. Use `gridColumnMin: "max-content"` for tables so each column is as wide as its widest cell.

   ```tsx
   <div
     style={{
       display: 'grid',
       gridTemplateColumns: 3,
       gridColumnMin: 'max-content',
       rowGap: 1,
       columnGap: 1,
     }}
   >
     {cells}
   </div>
   ```

   `gridTemplateRows` and `gridRowMin` work the same on the other axis.

2. **Window chrome at open time** — `render()` now honors a transparent titlebar, traffic-light position, and a blurred or transparent window background. Traffic lights can sit in a sidebar. The native titlebar does not take a strip above the app.

   ```tsx
   import { render } from '@gpuix/react'

   render(<App />, {
     title: 'Waku',
     width: 1180,
     height: 820,
     titlebarTransparent: true,
     windowBackground: 'blurred',
     trafficLightX: 16,
     trafficLightY: 17,
   })
   ```

   `windowBackground` is `"opaque"` (default), `"transparent"`, or `"blurred"`. The older `transparent: true` flag still maps to a transparent background when `windowBackground` is unset.

3. **`<diff>` flows with its parent** — it no longer owns a scroller unless you pass `scroll`. Nested scrolling is not supported in GPUI. A parent that already scrolls used to fight the inner `list()`. The default is now a column of rows, same as `<code>`.

   Use `maxLines` to keep a long patch short. Show more fires `onShowMore` with the hidden line count. Clear `maxLines` in that handler to reveal the rest.

   ```tsx
   const [open, setOpen] = useState(false)

   <diff
     patch={unifiedPatch}
     wordDiff
     maxLines={open ? undefined : 24}
     onShowMore={() => setOpen(true)}
   />
   ```

   Pass `scroll` and a bounded height only for a dedicated full-window viewer. That path still virtualizes with GPUI's `list()`.

4. **Debug frame overlay** — see draw time on a live window. The overlay paints after layout. It is not a React element.

   ```tsx
   import { render } from '@gpuix/react'

   render(<App />, { title: 'My App', debugFrameOverlay: 'full' })
   ```

   Or call the renderer:

   ```ts
   renderer.setDebugFrameOverlay('full')
   renderer.cycleDebugFrameOverlay()
   renderer.resetDebugFrameOverlayStats()
   renderer.getDebugFrameOverlay()
   ```

   Modes are `hidden` (default), `minimal` (last draw time), and `full` (`CUR`, `1%`, `10%`, `MAX`, `FRAMES`). The readout is **draw time**, not FPS. `8.3 MS` is about 120 Hz.

5. **Quit when the last window closes** — on macOS the red traffic-light button used to destroy the window and leave the bun/Node process running. Closing the last window now quits AppKit. The next `tick()` returns `false`. `render()` exits the process, so the Dock icon goes away.

6. **Overlays block hits, and `pointerEvents` works** — a filled or absolutely positioned `div` now inserts a blocking hitbox. Clicks, hovers, and scroll no longer reach controls under a Select, Combobox, or any other card.

   Set `pointerEvents: "none"` to opt out. Set `pointerEvents: "auto"` to block even when the element has no fill.

7. **Opaque Select, Combobox, and Tooltip surfaces** — `FloatingLayer` now defaults to `backgroundColor: "#1A1A1A"` so window blur and page content do not show through the card. Pass your own `style.backgroundColor` to override.

8. **`<svg>` icons paint on the first frame** — file paths are read from disk. `data:image/svg+xml` URLs from Bun/Vitest `import … with { type: 'file' }` are percent-decoded. The icon paints with `svg().data(...)`.

9. **`bun --hot` remounts no longer paint a black window** — `render()` now unmounts the previous React root with `flushSync`, so the old tree is gone before the new one is created.

10. **Cmd+Delete and Cmd+Backspace in `<input>` and `<textarea>`** — on macOS these match the system text field. Cmd+Backspace deletes to the start of the line. Cmd+Delete deletes to the end of the line.

11. **Vertical wheel over `overflowX: "scroll"` stays on the parent** — GPUI remaps mouse-wheel Y onto overflow-x unless `restrict_scroll_to_axis` is set. A parent that contains `<code>` or a markdown table then used to jump on both axes. Trackpad X still pans the wide child.

    ```tsx
    <div style={{ overflowY: 'scroll' }}>
      <code code={wideSource} language="ts" />
    </div>
    ```

12. **Parent scroller takes the wheel over a filled in-flow `div`** — a `backgroundColor` used to insert `occlude()` (BlockMouse), so `<virtual-list>` never saw the wheel over text or a card. In-flow fills now use `block_mouse_except_scroll()`. Absolute, fixed, and `pointerEvents: "auto"` still steal the wheel.

13. **macOS scroll stays at the display rate on expensive frames** — `tick()` used to sleep a fixed 8ms after every pump. A 10ms scroll frame plus that sleep ran at about 55fps on a 120Hz display. The next pump now waits only the leftover budget.

14. **Faster first React mount** — `applyBatch` sends styles and custom props as JSON values instead of double-encoded strings. A 10,000-row list spent most of its mount time parsing escaped strings twice. Legacy string payloads still decode.

15. **Raw custom-prop values stay intact** — `setCustomProp` still treats the payload as a JSON string. After the batch started carrying objects, a raw `"top"` or `"true"` was parsed again and threw. `<anchored side="top">` never committed. The queue now uses `setCustomPropValue` for a raw JSON value.

## 0.2.0

1. **Selectable text everywhere, plus `<code>`, `<diff>` and `<markdown>`** — every string GPUIX paints can be selected with a drag and copied with Cmd+C. A drag can start in a plain `<text>` and end inside a code block; the selection spans both.

   ```tsx
   <div style={{ display: 'flex', flexDirection: 'column' }}>
     <text>drag from here</text>
     <code code={'and into this code block'} language="ts" />
   </div>
   ```

   Chrome opts out the same way CSS does, and it inherits:

   ```tsx
   <div style={{ userSelect: 'none' }}>
     <text>toolbar label, never selected</text>
   </div>
   ```

   Read it from the renderer with `renderer.getSelectedText()` and clear it with `renderer.clearSelection()`.

   **`<code>`** is a syntax-highlighted block. One row per line at an exact line height, so its height is known before highlighting runs and a late highlight never reflows it.

   ```tsx
   <code code={source} language="typescript" showLineNumbers />
   <code code={source} path="src/app.ts" />   {/* detect from the extension */}
   ```

   **`<diff>`** is a unified diff viewer virtualized with GPUI's `list()`, so a 2000-line patch paints only the rows on screen. Collapsing a file removes its rows rather than hiding them.

   ```tsx
   <diff
     patch={unifiedPatch}
     wordDiff
     collapsedPaths={['pnpm-lock.yaml']}
     onToggleFile={(e) => toggle(e.value)}
     onLineClick={(e) => console.log(e.oldLine, e.newLine, e.value)}
   />
   ```

   `wordDiff` highlights only the tokens that changed inside paired `+`/`-` lines.

   **`<markdown>`** is GitHub-flavoured markdown: headings, lists, tables, block quotes, fenced code, strikethrough, task lists, and autolinked bare URLs.

   ```tsx
   <markdown source={readme} onLinkClick={(e) => open(e.value)} />
   ```

   All three take the same `theme` prop. Fields layer on top of the built-in dark theme:

   ```tsx
   <code
     code={source}
     language="rust"
     theme={{
       appearance: 'light',
       accent: '#7c86ff',
       syntax: { keyword: '#f38ba8', string: '#a6e3a1' },
     }}
   />
   ```

   Bundled languages: Rust, TypeScript, TSX, JavaScript, JSX, Python, Go, JSON, Bash, TOML, YAML, Markdown, HTML, CSS, C.

   Row heights, gutter widths, paddings and the heading scale live in `theme.metrics`, so tuning the design is a React re-render and never a native rebuild.

   ```tsx
   <diff
     patch={patch}
     theme={{
       metrics: {
         diffLineHeight: 26,
         diffGutterWidth: 48,
         mdHeadingSizes: [24, 19, 16, 14],
       },
     }}
   />
   ```

   New style props: `userSelect` (`"text"` | `"none"`, inherited), `selectionColor`, and `lineHeight` is now applied.

   New test helpers: `renderer.getPaintedText()`, `renderer.dragSelect(x1, y1, x2, y2)`, and `renderer.getSyntaxCacheStats()`.

   Ported from [Comet](https://github.com/zeronsh/comet) (MIT). See `THIRD_PARTY_NOTICES.md`.

2. **Native `<input>` and `<textarea>`** — single-line and multiline editors backed by GPUI's platform input handler.

   ```tsx
   <textarea
     value={draft}
     minRows={1}
     maxRows={8}
     onChange={(event) => setDraft(event.value ?? '')}
     onSubmit={send}
   />
   ```

   Both support a native caret, mouse selection, IME composition, clipboard actions, undo/redo, caret movement and grapheme-safe deletion. `Enter` submits and `Shift+Enter` inserts a newline in a textarea.

3. **`render()` remounts React on the same native window** — a `bun --hot` save remounts the tree without creating a second window.

   ```tsx
   import { render } from '@gpuix/react'

   function App() {
     return <div style={{ padding: 16 }}>hello</div>
   }

   render(<App />, { title: 'My App', width: 800, height: 600 })
   ```

   ```bash
   bun --hot app.tsx
   ```

   The first call creates the GPUI renderer, window, React root, and frame loop. Later calls reuse that host and remount the tree. `useState` resets. The native `.node` addon stays loaded.

   `createRoot`, `createRenderer`, and `startFrameLoop` still exist for tests and custom hosts. Pass `{ renderer }` into `render()` to drive the test renderer.

   React Refresh (keep hook state across saves) is not included.

4. **Headless Select, Combobox, and Tooltip** — unstyled primitives with the same compound composition used by shadcn. Import a namespace, wrap it in a local `components/ui/*.tsx`, and use those styled components in the app.

   ```tsx
   import * as SelectPrimitive from '@gpuix/react/select'

   <SelectPrimitive.Root value={model} onValueChange={setModel}>
     <SelectPrimitive.Trigger>
       <SelectPrimitive.Value placeholder="Select a model" />
     </SelectPrimitive.Trigger>
     <SelectPrimitive.Content>
       <SelectPrimitive.Item value="sonnet">Sonnet</SelectPrimitive.Item>
     </SelectPrimitive.Content>
   </SelectPrimitive.Root>
   ```

   Dedicated entry points:

   | Import | Main parts |
   |---|---|
   | `@gpuix/react/select` | `Root`, `Trigger`, `Value`, `Content`, `Item` |
   | `@gpuix/react/combobox` | `Root`, `Input`, `Content`, `List`, `Item`, `Empty` |
   | `@gpuix/react/tooltip` | `Provider`, `Root`, `Trigger`, `Content` |

   The barrel `@gpuix/react` still exports the prefixed names (`Select`, `SelectTrigger`, and the rest).

   Each part accepts GPUIX styles, including state-based item style functions. Menus support native focus, keyboard navigation, outside-click dismissal, window-edge snapping, and click occlusion. Comboboxes use the native text input and rank prefix matches before substring matches.

5. **`<virtual-list>`** — long, variable-height React collections. GPUI builds and lays out only rows near the viewport while React and the native retained tree keep the complete collection.

   ```tsx
   <virtual-list
     alignment="bottom"
     followTail
     estimatedItemHeight={180}
     style={{ flexGrow: 1, minHeight: 0 }}
   >
     {messages.map((message) => (
       <Message key={message.id} message={message} />
     ))}
   </virtual-list>
   ```

   Rows can contain any GPUIX host or custom element. Appended rows preserve list measurements, changed rows are remeasured, and existing `scrollTo`, `scrollToItem`, and `getScrollOffset` methods work with virtual lists.

6. **Tintable local SVG icons** — `<svg>` uses GPUI's monochrome SVG renderer.

   ```tsx
   <svg
     src="/absolute/path/to/search.svg"
     style={{ width: 16, height: 16, color: '#b4b4b4' }}
   />
   ```

   `width` and `height` control layout. `color` controls the icon tint.

7. **`startFrameLoop()`** — stop burning CPU on idle apps. The old `setImmediate` loop spun at roughly 27,000 ticks per second and measured **73.5% CPU** on an idle counter. `startFrameLoop` paces at ~125fps (~1% CPU).

   ```tsx
   import { startFrameLoop } from '@gpuix/react'

   startFrameLoop(renderer)
   ```

   ```tsx
   const loop = startFrameLoop(renderer, { frameMs: 16 })
   loop.stop()
   ```

   Each frame is scheduled only after the previous one finishes. Rendering is unchanged: one draw per React commit, and no draws at all while idle.

8. **Native GPUI platform** — Node applications use GPUI's native platform, window, renderer, and event pipeline on macOS, Windows, and Linux.

   On macOS, Node drives an embedded AppKit event pump from the pinned GPUIX fork on the process main thread. On Windows and Linux, GPUI runs its normal blocking event loop on a dedicated Rust UI thread while Node sends in-process render and window commands. Windows runtime validation is still pending.

9. **GPUI upgrade to zed `d5dc01f2`** — picks up several months of GPUI work, including `Application::run_embedded()`. GPUIX now holds the returned `ApplicationHandle` for the lifetime of the process.

   Scroll events can now report a cancelled phase. Previously a cancelled scroll gesture was reported to JS as `"ended"`.

   ```tsx
   <div
     style={{ overflow: 'scroll' }}
     onScroll={(e) => {
       if (e.touchPhase === 'cancelled') return
     }}
   />
   ```

   Building from source now requires **Rust 1.97.1**, pinned in `rust-toolchain.toml`. On macOS you also need the Metal compiler:

   ```bash
   xcodebuild -downloadComponent MetalToolchain
   ```

   Prebuilt binaries from npm are unaffected.

10. **Style props that were declared and dropped now work** — `<text>` takes the full style set (padding, width, backgroundColor, borderRadius, flex). `fontSize` works on `<div>` and custom elements. `textAlign`, `rowGap`, `columnGap`, and `lineHeight` are applied. `borderWidth: 0` can clear a border.

    ```tsx
    <text style={{ paddingLeft: 40, width: 300, backgroundColor: '#7c86ff', borderRadius: 12 }}>
      now works
    </text>
    ```

11. **`autoFocus` works and `<input>` is unstyled** — `autoFocus` was declared and dropped by the reconciler, so an `<input>` never held keyboard focus unless the user clicked it. It now works on every element type.

    ```tsx
    <input value={text} autoFocus onKeyDown={(e) => e.keyChar && setText(t => t + e.keyChar)} />
    ```

    `<input>` no longer hardcodes a background, border, or radius. Only the placeholder dims. Style the element or its wrapper:

    ```tsx
    <input
      value={text}
      style={{ backgroundColor: '#00000000', borderWidth: 0, color: '#ececec', fontSize: 15 }}
    />
    ```

    `<input>` is **controlled**: it paints `value` and reports keystrokes.

12. **Blinking caret** — the native input and textarea caret blinks every 500ms while focused and idle. Editing or moving the caret makes it immediately solid. Blurring the field stops its repaint timer.

    ```tsx
    <input theme={{ caret: '#22c55e' }} />
    ```

13. **Clipboard and natural scroll** — `Cmd+C` writes to the system clipboard via `arboard`. Wheel deltas keep the sign the OS already applied, so natural scrolling matches System Settings.

14. **React 19 JSX components** — the GPUIX JSX runtime accepts any valid `ReactNode` return type, so libraries such as `safe-mdx` can render parsed content into GPUIX host elements.

## 2026-03-02 23:30 UTC

- **Add hover/active pseudo-selector style support** — styles applied natively by GPUI with zero JS round-trips.
  - New `hover` and `active` keys in `StyleDesc` accept nested style objects: `style={{ backgroundColor: '#313244', hover: { backgroundColor: '#45475a' }, active: { backgroundColor: '#585b70' } }}`.
  - Rust `StyleDesc` (style.rs): added `hover: Option<Box<StyleDesc>>` and `active: Option<Box<StyleDesc>>` fields with serde support.
  - Renderer (renderer.rs): `build_div()` calls GPUI's native `.hover()` and `.active()` methods, passing the sub-styles through `apply_styles()` which works on `StyleRefinement` via the `Styled` trait.
  - TypeScript types (host.ts): `hover?` and `active?` typed as `Omit<StyleDesc, 'hover' | 'active'>` to prevent infinite nesting.
  - Added 7 tests validating hover-only, active-only, combined hover+active, empty hover, color-only hover, and hover alongside event handlers.

## 2026-03-02 16:50 UTC

- **Add GitHub Actions CI/CD pipeline** (`.github/workflows/ci.yml`) — builds native binaries for 4 targets (macOS arm64/x64, Linux x64/arm64), runs tests on macOS, and publishes to npm.
- Publish is version-gated: skips if the package.json version is already on npm. Bump version + push to main to release.
- Two packages published: `@gpuix/native` (per-platform binaries via napi pre-publish) and `@gpuix/react` (pure TypeScript).
- Generate `packages/native/npm/` per-platform package scaffolding (darwin-arm64, darwin-x64, linux-x64-gnu, linux-arm64-gnu).
- Add `build:release` script for Linux CI builds without test-support (gpui_macos is macOS-only).
- macOS builds include test-support by default so published binaries ship `TestGpuixRenderer` for user testing.
- Update `@gpuix/react` dependency on `@gpuix/native` from `workspace:*` to `workspace:^` for publishing.
- Add `publishConfig` to `@gpuix/react` package.json.
- Document Cargo feature gate in Cargo.toml comments.

## 2026-03-02 16:32 UTC

- **Migrate `packages/native` from napi-rs v2 to v3** — prerequisite for CI/CD and per-platform npm publishing.
  - Bump `napi` crate from `2` to `3` and `napi-derive` from `2` to `3` in `Cargo.toml` (`napi-build` stays at `2`).
  - Bump `@napi-rs/cli` from `^2.18.0` to `^3.1.3` in `package.json`.
  - Switch napi config from v2 `triples` format (`name` + `triples.additional`) to v3 `targets` format (`binaryName` + `targets` array). Add `x86_64-unknown-linux-gnu` and `aarch64-unknown-linux-gnu` targets.
  - Change `prepublishOnly` script from `napi prepublish` to `napi pre-publish` (v3 hyphenated command).
  - Add `publishConfig` with `registry` and `access: "public"`.
  - Wrap `ThreadsafeFunction` in `Arc` in `GpuixRenderer` — napi v3 `ThreadsafeFunction` is `!Clone`, so `Arc` allows sharing it into the `GpuixView` closure from `&self` methods.
  - Generated `index.js` now uses v3's `requireNative()` function pattern (replaces v2's switch/case loader).
  - Generated `index.d.ts` now includes JSDoc comments from Rust `///` doc comments.
- All 105 tests pass.

## 2026-03-02 17:05 UTC

- Fix `fontWeight` to accept both string and number values — previously `fontWeight: 700` (number) would reject the entire mutation batch because the Rust deserializer only accepted strings. Now uses a `FontWeightValue` enum with `#[serde(untagged)]` that deserializes both `"bold"` (string) and `700` (number). Numeric values are clamped to 1–1000.
- All 105 tests pass.

## 2026-03-02 16:55 UTC

- Add `whiteSpace` support — `"nowrap"` prevents text wrapping (single line), `"normal"` enables wrapping (default). Applied in both `apply_styles()` and `build_text()` via GPUI's `.whitespace_nowrap()` / `.whitespace_normal()`.
- Add `textOverflow` support — `"ellipsis"` truncates long text with "..." at end, `"ellipsis-start"` truncates from the start. Applied in both `apply_styles()` and `build_text()` via GPUI's `.text_ellipsis()` / `.text_ellipsis_start()`.
- Add `lineClamp` support — limits text to N visible lines. Applied in both `apply_styles()` and `build_text()` via GPUI's `.line_clamp(n)`. Values < 1 are ignored.
- Update README: document all text style properties, add note about `white-space: pre` not being supported in GPUI with a workaround pattern (split `\n` + flex column + nowrap per line).
- Add 13 new tests in styles.test.tsx: whiteSpace nowrap/normal with visual comparison, textOverflow ellipsis/ellipsis-start with comparison, lineClamp at 1/2/3 lines with comparison, edge case lineClamp: 0, div-level inheritance for nowrap and lineClamp, pre-like behavior composite test, and short text no-truncation edge case.
- All 104 tests pass (82 existing + 22 in styles.test.tsx).

## 2026-03-02 16:30 UTC

- Wire up `alignSelf` in `apply_styles()` — field existed in StyleDesc but was never applied. Uses direct `el.style().align_self` field access since GPUI has no convenience methods. Supports center, start, end, stretch, baseline.
- Fix `flexGrow` and `flexShrink` to respect actual numeric values — previously `flexGrow: 0` and `flexGrow: 1` both produced the same result (hardcoded 1.0). Now sets `el.style().flex_grow = Some(value)` directly.
- Add `fontFamily` support — new field in `StyleDesc` (Rust + TS), applied in both `apply_styles()` and `build_text()` via GPUI's `.font_family()` method. Enables monospace fonts for code rendering.
- Wire up `fontWeight` in `apply_styles()` and `build_text()` — field existed in StyleDesc but was never applied. Parses CSS weight strings (named keywords like "bold"/"semibold" and numeric like "700") to `gpui::FontWeight`. Case-insensitive with hyphenated variants (extra-bold, semi-bold).
- Add `backgroundColor` support to `build_text()` — text elements can now have background colors via `.bg()` on the wrapping div. Enables word-level diff highlighting.
- Wire up `flexWrap` in `apply_styles()` — field existed in StyleDesc but was never applied. Maps "wrap" → `flex_wrap()`, "wrap-reverse" → `flex_wrap_reverse()`, "nowrap" → `flex_nowrap()`.
- Extract `parse_font_weight()` helper function to deduplicate font-weight parsing between `apply_styles()` and `build_text()`.
- Add `styles.test.tsx` with 9 end-to-end tests covering all new features with Metal GPU screenshots: alignSelf stretch, flexShrink 0, flexGrow values, fontFamily (Menlo/Courier vs default), fontWeight (bold/light/normal), text backgroundColor, flexWrap, and a composite diff-viewer row test.
- All 91 tests pass.

## 2026-03-02 14:54 UTC

- Add test proving React refs expose the element's numeric ID (`ref.current.id`) for use with programmatic scroll API
- Remove dead `id?: string` prop from `Props` type — it was never wired to anything
- Add scroll usage docs to README: `overflow: "scroll"` example, per-axis scrolling, and programmatic scroll via refs
- Comment Props type to document that element IDs come from refs, not a user prop

## 2026-03-02 14:42 UTC

- Add scrollable container support — `overflow: "scroll"`, `overflowX: "scroll"`, `overflowY: "scroll"` now create native GPUI scrollable divs
- GPUI handles scroll physics automatically: scroll wheel events update a persistent `ScrollHandle` offset, content is clipped and translated, offset is clamped to valid bounds
- `ScrollHandle` persists across frames in `GpuixView::scroll_handles` (keyed by element ID), same lifecycle pattern as `focus_handles`
- Add per-axis overflow hidden support: `overflowX: "hidden"` and `overflowY: "hidden"` now map to `overflow_x_hidden()` / `overflow_y_hidden()`
- Add programmatic scroll API via napi: `scrollTo(elementId, x, y)`, `scrollToItem(elementId, index)`, `getScrollOffset(elementId)` on both `GpuixRenderer` and `TestGpuixRenderer`
- Production renderer syncs scroll handles to a thread_local (`SCROLL_HANDLES`) after each render so napi methods can access them without an App context
- TestRenderer exposes `scrollTo()`, `scrollToItem()`, `getScrollOffset()` wrapper methods
- NativeRenderer interface updated with optional scroll methods
- Add 6 new end-to-end scroll tests: basic scroll, overflow-y only, programmatic scrollTo, scrollToItem, screenshot regression (before/after scroll), and onScroll event + overflow scroll combo
- All 80 tests pass

## 2026-03-01 20:45 UTC

- Remove JS shadow tree from TestRenderer — all element state now lives exclusively in Rust's RetainedTree, queried via napi
- TestRenderer inspection methods (findByType, getAllText, toJSON, getRoot, getElement, findByText) now query the native TestGpuixRenderer instead of maintaining a parallel JS element map
- Add `getRootId()` napi method to TestGpuixRenderer for root element queries
- Add `customProps` to `getTreeJson()` output so test inspection can see custom element props (used by img/input tests)
- TestRenderer constructor now requires native renderer (throws if not available); tests already skip via `hasNativeTestRenderer`
- Net ~220 lines of redundant JS state management code removed
- All 68 tests pass — zero test file changes needed

## 2026-03-01 20:30 UTC

- Add FFI mutation batching — all React reconciler mutations per commit are now buffered JS-side and sent to Rust in a single `applyBatch()` napi call instead of N individual FFI calls
- Add `apply_batch(json)` to both `GpuixRenderer` and `TestGpuixRenderer` (Rust) — parses a JSON array of string-named mutation tuples `["methodName", ...args]` and applies them under a single mutex lock
- Atomic two-phase Rust processing: `parse_batch_ops()` validates all ops into typed `BatchOp` enum before any tree mutation; malformed batch → error with tree unchanged
- Add Proxy-based `wrapWithBatching()` (`batch-renderer.ts`) — auto-captures any NativeRenderer method call as `[name, ...args]`; adding new methods requires zero changes to the batching layer
- TestRenderer uses `_skipNative` flag + dynamic dispatch for `applyBatch()` replay — also zero changes needed when adding new methods
- Wire `wrapWithBatching()` into both `createRoot()` and `createTestRoot()` — batching is automatic when the renderer supports `applyBatch()`
- Backward compatible: individual mutation methods remain available; batching is opt-in via `applyBatch` presence
- All 68 existing tests pass through the batched path

## 2026-03-01 19:07 UTC

- Add native `<img>` custom element backed by `gpui::img(PathBuf)` with `src` and `objectFit` custom props and fallback rendering states for missing/failed sources
- Register image factory in the custom element registry and expose `ImgProps` in React JSX runtime/dev-runtime type surfaces
- Add new end-to-end `img.test.tsx` suite including screenshot regression that captures before/after PNGs when image `src` is set

## 2026-03-01 18:52 UTC

- Add new `<anchored>` custom element with GPUI `anchored()` positioning props (`x`/`y`, `position`, `anchor`, `snapToWindow`, `snapMargin`) and optional deferred overlay rendering (`deferred`, `priority`)
- Extend custom element render context to pass built child elements so custom primitives can wrap and position nested React content
- Register `anchored` in the default custom element registry and expose it in React intrinsic types/component map
- Add end-to-end anchored deferred dialog overlay test (open, inside click stays open, outside click closes)

## 2026-03-01 18:47 UTC

- Add dialog overlay screenshot regression test that captures before/after PNGs and asserts visual output changes when opening the dialog

## 2026-03-01 18:45 UTC

- Add absolute positioning support in native style mapping (`position`, `top`, `right`, `bottom`, `left`) so React styles place elements out of flow like dialogs/tooltips
- Add end-to-end dialog overlay test: click button opens tooltip-like dialog content, inside click keeps it open, outside click closes via `onMouseDownOutside`

## 2026-03-01 18:35 UTC

- Add polymorphic custom element trait infrastructure (`CustomElement`, `CustomElementFactory`, `CustomElementRegistry`)
- Implement `<input>` as first custom element with value/placeholder/readOnly props and keyboard event handling
- Add `custom_props` field to `RetainedElement` for storing non-style/non-event props on custom elements
- Add `setCustomProp`/`getCustomProp` napi methods on both `GpuixRenderer` and `TestGpuixRenderer`
- Add custom prop forwarding in React reconciler (`host-config.ts`) — automatically syncs non-reserved props for non-div/text elements
- Add `InputProps` type and `input` to JSX IntrinsicElements
- Add 6 end-to-end tests: input rendering, keyboard typing (controlled component), backspace, screenshot before/after, tree structure
- Fix jsx-dev-runtime.js to export `jsxDEV` for React 19 compatibility with vitest (was breaking all tests)
- All 27 tests pass (6 new input + 21 existing events)

## 2026-03-01 17:42 UTC

- Fix custom element lifecycle cleanup by pruning/destroying stale trait instances when IDs disappear from the retained tree
- Fix stale custom prop state by resetting missing known props to `null` each frame via `supported_props()` synchronization
- Apply retained `style` to custom elements through `CustomRenderContext` so `<input style={...}>` affects native layout/hit-testing
- Filter custom element event wiring to declared `supported_events()` only
- Harden React custom prop forwarding with safe JSON serialization fallback (`null` on unsupported/circular values)
- Expand input end-to-end coverage with `readOnly` removal regression test and style-based click hit-test assertion

## 2026-03-01 17:15 UTC

- Rewrite README to reflect current mutation-based architecture (was describing old JSON tree approach)
- Replace "description-based renderer" language with "mutation-based protocol over napi-rs FFI"
- Add architecture diagram showing individual napi calls (createElement, appendChild, setStyle, commitMutations)
- Add Mutation API section documenting the full NativeRenderer interface
- Add Event Flow section with pipeline diagram (GPUI → Rust closure → ThreadsafeFunction → JS event registry → React handler)
- Add detailed events table with payload fields for each event type
- Add Testing section covering TestGpuixRenderer (GPU-backed Metal tests, screenshot capture, native event simulation)
- Update status checklist: mark keyboard events, focus/blur, scroll, click-outside, and test renderer as completed
- Update usage example to use createRenderer() instead of raw GpuixRenderer constructor

## 2026-03-01 16:48 UTC

- Center screenshot probe cards in the visual renderer tests so captured frames represent realistic composition instead of top-left anchored blocks
- Improve screenshot test visuals with richer card styling (rounded surfaces, palette contrast, readable text hierarchy)
- Keep visual assertions unchanged (before/after PNG difference) while moving click/hover simulation coordinates to centered card hit zones

## 2026-03-01 16:35 UTC

- Expand visual screenshot coverage with additional end-to-end tests for `click`, `keyDown`, and `mouseEnter`-driven hover state changes
- Add shared screenshot assertion helper in `events.test.tsx` to enforce non-empty PNG output and before/after image differences

## 2026-03-01 16:20 UTC

- Fix `build_text` to render child text elements recursively instead of dropping nested text nodes
- Improve screenshot reliability by forcing `window.refresh()` before `capture_screenshot()` in the native test renderer
- Strengthen screenshot integration test to assert visual output changes (compare PNG bytes before vs after interaction)
- Update screenshot test fixture to use a high-contrast background toggle so black-frame regressions are obvious

## 2026-03-01 15:40 UTC

- Switch TestGpuixRenderer from `TestAppContext` (no GPU) to `VisualTestAppContext` (real Metal rendering on macOS)
- Add `gpui_macos` dependency for `MacPlatform` — provides real Metal GPU rendering in test windows
- Replace raw `VisualTestContext` pointer with `VisualTestAppContext` + `AnyWindowHandle` in thread_local storage
- Add `capture_screenshot(path)` napi method — renders via Metal, reads back pixels, saves as PNG
- Add `captureScreenshot(path)` JS wrapper to `TestRenderer`
- Add screenshot integration test (renders counter, clicks, captures before/after PNGs)
- Gate `test_renderer` module on `#[cfg(all(feature = "test-support", target_os = "macos"))]`
- All 19 tests pass (18 existing event/tree tests + 1 new screenshot test)

## 2026-03-01 15:24 UTC

- Fix missing text in macOS visual screenshots by enabling `gpui_macos/font-kit` under `test-support`
- Keep `VisualTestAppContext` on real `MacTextSystem` instead of fallback `NoopTextSystem`, restoring glyph rasterization in `capture_screenshot()`
- Validate with an example-like counter render: text labels (`0/1`, `+`, `-`, `Reset`) now appear correctly in captured PNGs

## 2026-03-01 12:50 UTC

- Add plan for GPU-backed test renderer with screenshot support (`docs/visual-screenshot-plan.md`)
- Plan uses GPUI's `VisualTestAppContext` + Metal rendering on macOS (Oracle-reviewed, original headless wgpu approach rejected due to `WgpuRenderer` being surface-bound)

## 2026-03-01 12:25 UTC

- Add changelog requirement to AGENTS.md
- Document auto-generated napi-rs files in AGENTS.md (`index.d.ts`, `index.js`, `*.node`)

## 2026-03-01 12:00 UTC

- Add `simulate_key_down(keystroke, is_held?)` and `simulate_key_up(keystroke)` to Rust TestGpuixRenderer for fine-grained key event testing
- Extend `simulate_mouse_move(x, y, pressed_button?)` to accept optional pressed button for drag simulation
- Add `nativeSimulateKeyDown`, `nativeSimulateKeyUp` JS wrappers to TestRenderer
- Update `nativeSimulateMouseMove` to pass pressed button through to native
- Restore dropped tests: keyUp state update, keyDown+keyUp sequence, mouse button mapping (left/right/middle), drag pressedButton
- Tighten weak assertions: scroll checks exact deltaX/deltaY/touchPhase, mouseMove checks exact x/y
- Fix stale "mock-only mode" comment in testing.ts

## 2026-03-01 11:45 UTC

- Migrate all event tests from JS-only simulation to native GPUI end-to-end simulation
- Add `simulate_mouse_down(x, y, button)` and `simulate_mouse_up(x, y, button)` to Rust TestGpuixRenderer
- Add `nativeSimulateMouseDown` and `nativeSimulateMouseUp` JS wrappers to TestRenderer
- Remove all 10 JS-only simulation methods from TestRenderer (`simulateEvent`, `simulateClick`, `simulateKeyDown`, `simulateKeyUp`, `simulateMouseEnter`, `simulateMouseLeave`, `simulateMouseDown`, `simulateMouseUp`, `simulateMouseMove`, `simulateScroll`)
- Rewrite all tests to use coordinate-based native GPUI simulation with explicit element sizes
- Change key names from `"arrowDown"`/`"arrowUp"` to GPUI names `"down"`/`"up"`
