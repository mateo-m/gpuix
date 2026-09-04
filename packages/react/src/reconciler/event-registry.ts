import type { EventPayload } from "@gpuix/native"
import type {
  Container,
  ElementIdAllocator,
  EventHandlerMap,
  NativeRenderer,
} from "../types/host.js"

interface RendererState {
  container?: Container
  ids: ElementIdAllocator
  windowKeyEventId: number
}

// bun --hot preserves the renderer, so its React state must survive module reloads too.
const RENDERER_STATES_KEY = Symbol.for("@gpuix/react/renderer-states")
const rendererStates = (() => {
  const existing = Reflect.get(globalThis, RENDERER_STATES_KEY) as
    | WeakMap<NativeRenderer, RendererState>
    | undefined
  if (existing) return existing
  const created = new WeakMap<NativeRenderer, RendererState>()
  Reflect.set(globalThis, RENDERER_STATES_KEY, created)
  return created
})()

function stateFor(renderer: NativeRenderer): RendererState {
  let state = rendererStates.get(renderer)
  if (!state) {
    state = {
      ids: { nextElementId: 0 },
      windowKeyEventId: 0,
    }
    rendererStates.set(renderer, state)
  }
  return state
}

export function idAllocatorFor(renderer: NativeRenderer): ElementIdAllocator {
  return stateFor(renderer).ids
}

export function nextWindowKeyEventId(renderer: NativeRenderer): number {
  const state = stateFor(renderer)
  state.windowKeyEventId += 1
  return state.windowKeyEventId
}

export function attachRoot(renderer: NativeRenderer, container: Container): void {
  const state = stateFor(renderer)
  const owner = state.container
  if (owner && owner !== container) {
    throw new Error(
      "This renderer already drives a mounted GPUIX root. One renderer owns one window, one native root id, and one event map, so a second root would silently take both over. Unmount the first root first."
    )
  }
  state.container = container
}

/** Only the owner may detach. Otherwise unmounting a rejected or stale root
 *  would delete the live root's event mapping and every handler would go dead. */
export function detachRoot(renderer: NativeRenderer, container: Container): boolean {
  const state = stateFor(renderer)
  if (state.container === container) {
    state.container = undefined
    return true
  }
  return false
}

export function containerForRenderer(renderer: NativeRenderer): Container | undefined {
  return rendererStates.get(renderer)?.container
}

export function handleGpuixEvent(payload: EventPayload, renderer: NativeRenderer): boolean {
  const container = containerForRenderer(renderer)
  if (!container) return false
  const onEvent = container.onEvent
  if (payload.eventType === "windowKeyDown" || payload.eventType === "windowKeyUp") {
    if (payload.elementId !== container.windowKeyEventId) return false
    const handler =
      payload.eventType === "windowKeyDown"
        ? container.windowKeyEventHandlers.onKeyDown
        : container.windowKeyEventHandlers.onKeyUp
    if (!handler) return false
    handler(
      {
        ...payload,
        elementId: 0,
        eventType: payload.eventType === "windowKeyDown" ? "keyDown" : "keyUp",
      },
      renderer
    )
    onEvent?.(payload)
    return true
  }
  const elementHandlers = container.eventHandlers.get(payload.elementId)
  if (!elementHandlers) return false
  const handler = elementHandlers.get(payload.eventType)
  if (!handler) return false
  handler(payload)
  onEvent?.(payload)
  return true
}

export function registerEventHandler(
  eventHandlers: EventHandlerMap,
  elementId: number,
  eventType: string,
  handler: (event: EventPayload) => void
): void {
  let elementHandlers = eventHandlers.get(elementId)
  if (!elementHandlers) {
    elementHandlers = new Map()
    eventHandlers.set(elementId, elementHandlers)
  }
  elementHandlers.set(eventType, handler)
}

export function unregisterEventHandler(
  eventHandlers: EventHandlerMap,
  elementId: number,
  eventType: string
): void {
  const m = eventHandlers.get(elementId)
  if (!m) return
  m.delete(eventType)
  if (m.size === 0) eventHandlers.delete(elementId)
}

export function unregisterEventHandlers(
  eventHandlers: EventHandlerMap,
  elementId: number
): void {
  eventHandlers.delete(elementId)
}
