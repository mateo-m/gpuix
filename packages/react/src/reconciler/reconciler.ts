import React from "react"
import type { ReactNode } from "react"
import ReactReconciler from "react-reconciler"
import type { OpaqueRoot } from "react-reconciler"
import { ConcurrentRoot } from "react-reconciler/constants.js"
import { GpuixContext } from "../hooks/use-gpuix.js"
import type {
  Container,
  ElementIdAllocator,
  NativeRenderer,
  RootOptions,
} from "../types/host.js"
import { wrapWithBatching } from "./batch-renderer.js"
import { createClassNameCache } from "./class-names.js"
import { attachRoot, detachRoot } from "./event-registry.js"
import { hostConfig } from "./host-config.js"

// Cast to any because @types/react-reconciler is out of date with react-reconciler 0.31.0
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export const reconciler = ReactReconciler(hostConfig as any)

/**
 * Register with the React DevTools global hook.
 *
 * This is not only for DevTools, and it is not optional. React Fast Refresh
 * reaches a renderer through the same hook: `react-refresh` patches
 * `hook.inject`, keeps the `scheduleRefresh` and `setRefreshHandler` helpers
 * this call passes in, and drives hot updates through them.
 *
 * Drop this call and there is **no error and no page reload**. Bun still marks
 * the edited module self-accepting and still calls `performReactRefresh()`,
 * which iterates zero mounted roots and schedules nothing. The bundle updates
 * and the painted UI silently stays stale.
 *
 * The hook has to already exist when this module evaluates. Bun's HMR runtime
 * calls `injectIntoGlobalHook(window)` during bundle init, so it does in the
 * dev server, and `injectIntoDevTools()` is a no-op returning `false` in plain
 * Node. Do not test that return value: it ends in `hook.checkDCE ? true : false`
 * and `react-refresh` installs no `checkDCE`, so a working injection still
 * reports `false`. `fast-refresh.test.tsx` asserts the observable behaviour.
 */
try {
  // @ts-expect-error the types for `react-reconciler` are not up to date with the library
  reconciler.injectIntoDevTools()
} catch {
  // No DevTools hook in this process.
}

const _r = reconciler as typeof reconciler & {
  flushSyncFromReconciler?: typeof reconciler.flushSync
}
export const flushSync = _r.flushSyncFromReconciler ?? _r.flushSync

export interface Root {
  render: (node: ReactNode) => void
  unmount: () => void
  /** The renderer this root draws on, which is what `render()` opened. */
  renderer: NativeRenderer
}

const idAllocators = new WeakMap<NativeRenderer, ElementIdAllocator>()

function idAllocatorFor(renderer: NativeRenderer): ElementIdAllocator {
  let alloc = idAllocators.get(renderer)
  if (!alloc) {
    alloc = { nextElementId: 0 }
    idAllocators.set(renderer, alloc)
  }
  return alloc
}

export function createRoot(renderer: NativeRenderer, options: RootOptions = {}): Root {
  let container: OpaqueRoot | null = null
  const batchedRenderer = wrapWithBatching(renderer)
  const gpuixContainer: Container = {
    renderer: batchedRenderer,
    ids: idAllocatorFor(renderer),
    eventHandlers: new Map(),
    classNames: options.resolveClassName
      ? createClassNameCache(options.resolveClassName)
      : null,
    warnedAboutClassName: false,
  }
  attachRoot(renderer, gpuixContainer)
  attachRoot(batchedRenderer, gpuixContainer)

  const cleanup = (): void => {
    if (container) {
      // Must be sync. A late unmount destroy()s remounted ids and the window goes black.
      flushSync(() => {
        reconciler.updateContainer(null, container, null, () => {})
      })
      container = null
    }
    detachRoot(renderer)
    detachRoot(batchedRenderer)
  }

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  container = (reconciler.createContainer as any)(
    gpuixContainer,
    ConcurrentRoot,
    null,
    false,
    null,
    "",
    console.error,
    console.error,
    console.error,
    null
  )

  return {
    render: (node): void => {
      const activeContainer = container
      if (!activeContainer) {
        throw new Error("Cannot render an unmounted GPUIX root")
      }
      reconciler.updateContainer(
        React.createElement(
          GpuixContext.Provider,
          { value: { renderer: batchedRenderer } },
          node
        ),
        activeContainer,
        null,
        () => {}
      )
    },

    unmount: cleanup,
    renderer,
  }
}
