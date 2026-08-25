// GPUIX React - React bindings for GPUI
export { createRoot, flushSync } from "./reconciler/index.js"
export type { ClassNameResolver, RootOptions } from "./types/host.js"
export {
  createRenderer,
  enableAutomation,
  render,
  resetRender,
  startFrameLoop,
} from "./reconciler/renderer.js"
export { GpuixContext, useGpuix, useGpuixRequired } from "./hooks/use-gpuix.js"
export { useWindowSize } from "./hooks/use-window-size.js"
export {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectScrollDownButton,
  SelectScrollUpButton,
  SelectSeparator,
  SelectTrigger,
  SelectValue,
} from "./components/select.js"
export type {
  SelectContentProps,
  SelectItemProps,
  SelectItemState,
  SelectProps,
  SelectTriggerProps,
  SelectTriggerState,
  SelectValueProps,
} from "./components/select.js"
export {
  Combobox,
  ComboboxContent,
  ComboboxEmpty,
  ComboboxGroup,
  ComboboxInput,
  ComboboxItem,
  ComboboxLabel,
  ComboboxList,
  ComboboxSeparator,
  ComboboxTrigger,
  ComboboxValue,
} from "./components/combobox.js"
export type {
  ComboboxInputProps,
  ComboboxItemProps,
  ComboboxItemState,
  ComboboxListProps,
  ComboboxProps,
  ComboboxTriggerProps,
  ComboboxValueProps,
} from "./components/combobox.js"
export { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "./components/tooltip.js"
export type {
  TooltipContentProps,
  TooltipProps,
  TooltipProviderProps,
  TooltipTriggerProps,
} from "./components/tooltip.js"
export { motion, VirtualList } from "./components/index.js"
export type { Root, FrameLoop, RenderOptions } from "./reconciler/renderer.js"
export type { WindowSize } from "./hooks/use-window-size.js"

// Re-export types
export type { MotionDivProps, WindowedVirtualListProps } from "./components/index.js"
export type {
  DebugFrameOverlayMode,
  DebugFrameOverlayStats,
  MotionEase,
  MotionProps,
  MotionStyle,
  MotionTransition,
  NativeRenderer,
  StyleDesc,
} from "./types/host.js"
export { handleGpuixEvent } from "./reconciler/event-registry.js"

// Testing utilities
export {
  TestRenderer,
  applyMacCpuThrottleFromEnv,
  createTestRoot,
  hasNativeTestRenderer,
  readMacCpuThrottle,
} from "./testing.js"
export { connectTest, launch } from "./automation/index.js"
export type { MacCpuThrottle, TestRoot, TestElement } from "./testing.js"
export type {
  EventPayload,
  EventModifiers,
  WindowOptions,
  WindowSize as NativeWindowSize,
} from "@gpuix/native"

export { GpuixRenderer } from "@gpuix/native"
