import type { EventPayload } from "@gpuix/native"
import type { Container, EventHandlerMap, NativeRenderer } from "../types/host.js"

/** One renderer, one root. This map is also the ownership guard: a renderer
 *  owns one window, one native root id, and one event handler map, so a second
 *  root would replace all three without the first root ever knowing. */
const containersByRenderer = new WeakMap<NativeRenderer, Container>()

export function attachRoot(renderer: NativeRenderer, container: Container): void {
  const owner = containersByRenderer.get(renderer)
  if (owner && owner !== container) {
    throw new Error(
      "This renderer already drives a mounted GPUIX root. One renderer owns one window, one native root id, and one event map, so a second root would silently take both over. Unmount the first root first."
    )
  }
  containersByRenderer.set(renderer, container)
}

/** Only the owner may detach. Otherwise unmounting a rejected or stale root
 *  would delete the live root's event mapping and every handler would go dead. */
export function detachRoot(renderer: NativeRenderer, container: Container): void {
  if (containersByRenderer.get(renderer) === container) {
    containersByRenderer.delete(renderer)
  }
}

export function containerForRenderer(renderer: NativeRenderer): Container | undefined {
  return containersByRenderer.get(renderer)
}

export function handleGpuixEvent(payload: EventPayload, renderer: NativeRenderer): void {
  const container = containersByRenderer.get(renderer)
  if (!container) return
  const elementHandlers = container.eventHandlers.get(payload.elementId)
  if (!elementHandlers) return
  const handler = elementHandlers.get(payload.eventType)
  if (handler) handler(payload)
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
