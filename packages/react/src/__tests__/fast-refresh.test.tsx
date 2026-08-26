/// React Fast Refresh over the GPUIX reconciler.
///
/// This is the regression guard for browser HMR. `bun scripts/web.ts` gets its
/// hot updates from `react-refresh`, which never talks to a renderer directly:
/// it patches the React DevTools global hook, keeps the `scheduleRefresh` and
/// `setRefreshHandler` helpers that `reconciler.injectIntoDevTools()` passes in,
/// and drives updates through them.
///
/// If that injection ever stops working there is no error and no page reload.
/// Bun still accepts the update and still calls `performReactRefresh()`, which
/// finds zero mounted roots and schedules nothing, so the painted UI silently
/// stays stale. Only a test can catch that.
///
/// `injectIntoGlobalHook` has to run before `reconciler.ts` evaluates, so
/// `@gpuix/react` is loaded with a dynamic import after the hook is installed.
/// A static import would evaluate first and the injection would find no hook.

import { describe, expect, it } from "vitest"
import React, { useState } from "react"
import * as RefreshRuntime from "react-refresh/runtime"

RefreshRuntime.injectIntoGlobalHook(globalThis)

const { createTestRoot, hasNativeTestRenderer } = await import("../testing.js")

const describeNative = hasNativeTestRenderer ? describe : describe.skip

/** One React Refresh family. Both versions of a component register under it. */
const FAMILY = "test/Counter"

describeNative("react fast refresh", () => {
  it("swaps a component and keeps its useState", () => {
    // The `label` closure stands in for an edited string literal. Only the
    // rendered text differs between the two versions; the hooks are identical,
    // so React Refresh must update in place instead of remounting.
    const makeCounter = (label: string) =>
      function Counter() {
        const [count, setCount] = useState(0)
        return (
          <div style={{ width: 200, height: 50 }} onClick={() => setCount((value) => value + 1)}>
            <text>{`${label} ${count}`}</text>
          </div>
        )
      }

    const before = makeCounter("before")
    RefreshRuntime.register(before, FAMILY)

    const testRoot = createTestRoot()
    testRoot.render(React.createElement(before))
    expect(testRoot.renderer.getAllText()).toEqual(["before 0"])

    // The hook saw our commit, so `react-refresh` tracks this root. Without the
    // `injectIntoDevTools()` call in `reconciler.ts` this stays 0 and every
    // refresh below is a silent no-op. Underscore-prefixed, but React's source
    // labels it "Exposed for testing".
    expect(RefreshRuntime._getMountedRootCount()).toBe(1)

    // Drive the state through a real click so the value cannot come from the
    // initial render.
    testRoot.renderer.nativeSimulateClick(10, 10)
    expect(testRoot.renderer.getAllText()).toEqual(["before 1"])

    const after = makeCounter("after")
    RefreshRuntime.register(after, FAMILY)
    RefreshRuntime.performReactRefresh()
    testRoot.renderer.flush()

    // New render output, same state. A remount would read "after 0".
    expect(testRoot.renderer.getAllText()).toEqual(["after 1"])

    testRoot.unmount()
  })
})
