/// Buffers React mutations into one applyBatch() FFI call per commit.
///
/// Queue raw objects for setStyle / setCustomProp. Do not JSON.stringify them
/// first. The outer applyBatch stringify would escape that string again, and
/// Rust would parse twice. A 10k-row mount spent 626ms in applyBatch that way.
///
/// ## Batch timing
///
/// The batch boundary is React's commit phase (synchronous):
///
///   setState() → React render → reconciler mutations → resetAfterCommit()
///                                ↓ queue ops          ↓ applyBatch(json)
///
/// Multiple setState calls batched by React into one render = one batch.
/// Multiple separate commits in the same event loop tick = multiple batches.
///
/// ## Render-phase isolation
///
/// React's createInstance / createTextInstance / appendInitialChild callbacks
/// only build lightweight JS host nodes. A placement callback materializes the
/// accepted subtree during commit, so abandoned concurrent renders never enter
/// this queue.

import type { MutationRenderer, NativeRenderer } from "../types/host.js"
import { containerForRenderer, unregisterEventHandlers } from "./event-registry.js"

export type MutationTuple = (number | string | boolean | object | null)[]

/**
 * Wrap a NativeRenderer with batching support.
 *
 * The returned facade exists only during React's commit phase. Application
 * commands use the original NativeRenderer.
 */
export function wrapWithBatching(inner: NativeRenderer): MutationRenderer {
  let queue: MutationTuple[] = []

  return {
    createElement(id, elementType) {
      queue.push(["createElement", id, elementType])
    },
    destroyElement(id) {
      queue.push(["destroyElement", id])
      return []
    },
    appendChild(parentId, childId) {
      queue.push(["appendChild", parentId, childId])
    },
    insertBefore(parentId, childId, beforeId) {
      queue.push(["insertBefore", parentId, childId, beforeId])
    },
    setStyle(id, style) {
      queue.push(["setStyle", id, style])
    },
    setText(id, content) {
      queue.push(["setText", id, content])
    },
    setEventListener(id, eventType, hasHandler) {
      queue.push(["setEventListener", id, eventType, hasHandler])
    },
    setRoot(id) {
      queue.push(["setRoot", id])
    },
    setCustomProp(id, key, value) {
      queue.push(["setCustomProp", id, key, value])
    },
    flushMutations() {
      if (queue.length === 0) return

      // Preserve the queue on failure so JS and Rust cannot desync.
      const destroyedIds = inner.applyBatch(JSON.stringify(queue))
      const container = containerForRenderer(inner)
      if (container) {
        for (const id of destroyedIds) {
          unregisterEventHandlers(container.eventHandlers, id)
        }
      }

      queue = []
    },
  }
}
