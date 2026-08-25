import type { EventPayload } from "@gpuix/native"

/**
 * A value `width`, `height` and the four min and max forms take.
 *
 * A bare number is pixels. A string is read by the same length parser every
 * other length property uses, so `"6rem"`, `"1in"`, `"calc(100px + 2rem)"` and
 * `"var(--size)"` all work. On top of those it takes a percentage of the
 * parent, such as `"50%"`, and `"auto"` for the size the content takes.
 *
 * A value the parser cannot read drops the one declaration and leaves the rest
 * of the style alone, the way a browser drops a declaration it cannot parse.
 */
export type DimensionValue = number | string

export interface MotionStyle {
  width?: number
  /**
   * A length in pixels, or `"auto"` for the height the content takes.
   *
   * `"auto"` is measured at the width the element really gets, whether that
   * comes from a declared length, from `flex`, from a percentage or from a
   * stretched cross axis, so text wraps the way it will on screen. The
   * measurement repeats every frame, so the animation follows content that
   * changes while it runs.
   */
  height?: number | "auto"
  opacity?: Numeric
  top?: Numeric
  right?: Numeric
  bottom?: Numeric
  left?: Numeric
  borderRadius?: Numeric
}

export type MotionEase =
  | "linear"
  | "ease"
  | "easeIn"
  | "easeOut"
  | "easeInOut"
  | [number, number, number, number]

export interface MotionTransition {
  /** Duration in seconds. */
  duration?: number
  /** Delay in seconds. */
  delay?: number
  ease?: MotionEase
}

export interface MotionProps {
  initial?: MotionStyle | false
  animate: MotionStyle
  transition?: MotionTransition
}

export interface BoxShadow {
  offsetX: number
  offsetY: number
  blurRadius: number
  spreadRadius: number
  color: string
}

/**
 * A style value that resolves to a number.
 *
 * A bare number is pixels, which is what the `style` prop has always taken.
 * A string is there for `var()` and for writing the unit, so `8`, `"8px"` and
 * `"var(--pad)"` all mean the same padding. Any other unit drops the
 * declaration rather than paint the wrong size, so `"2rem"` does nothing.
 */
export type Numeric = number | string

export interface StyleDesc {
  display?: string
  visibility?: string
  flexDirection?: string
  flexWrap?: string
  flexGrow?: Numeric
  flexShrink?: Numeric
  flexBasis?: Numeric
  alignItems?: string
  alignSelf?: string
  alignContent?: string
  justifyContent?: string
  gap?: Numeric
  rowGap?: Numeric
  columnGap?: Numeric
  gridTemplateColumns?: Numeric
  gridTemplateRows?: Numeric
  gridColumnMin?: "zero" | "min-content" | "max-content"
  gridRowMin?: "zero" | "min-content" | "max-content"

  width?: DimensionValue
  height?: DimensionValue
  minWidth?: DimensionValue
  minHeight?: DimensionValue
  maxWidth?: DimensionValue
  maxHeight?: DimensionValue

  padding?: Numeric
  paddingTop?: Numeric
  paddingRight?: Numeric
  paddingBottom?: Numeric
  paddingLeft?: Numeric

  margin?: Numeric
  marginTop?: Numeric
  marginRight?: Numeric
  marginBottom?: Numeric
  marginLeft?: Numeric

  position?: string
  top?: number
  right?: number
  bottom?: number
  left?: number

  background?: string
  backgroundColor?: string
  color?: string
  opacity?: number

  borderWidth?: Numeric
  borderTopWidth?: Numeric
  borderRightWidth?: Numeric
  borderBottomWidth?: Numeric
  borderLeftWidth?: Numeric
  borderColor?: string
  borderRadius?: number
  borderTopLeftRadius?: Numeric
  borderTopRightRadius?: Numeric
  borderBottomLeftRadius?: Numeric
  borderBottomRightRadius?: Numeric
  boxShadow?: BoxShadow

  fontSize?: Numeric
  fontFamily?: string
  fontWeight?: string | number
  textAlign?: string
  lineHeight?: Numeric
  whiteSpace?: "normal" | "nowrap"
  textOverflow?: "ellipsis" | "ellipsis-start"
  lineClamp?: Numeric

  overflow?: string
  overflowX?: string
  overflowY?: string

  cursor?: string
  /** `"auto"` blocks hits behind this element. `"none"` never does. Unset blocks when the element paints a fill or is absolutely positioned. */
  pointerEvents?: "auto" | "none"

  /** "none" opts this element and its subtree out of text selection.
   *  Inherited like the CSS property, so a toolbar can disable it once. */
  userSelect?: "text" | "none" | "auto"
  /** Selection wash colour for this subtree. Defaults to the theme accent at 35%. */
  selectionColor?: string

  // Pseudo-selector styles, applied by GPUI natively (no JS round-trip).
  // Nesting is one level deep: hover/active cannot contain hover/active.
  //
  // These two are the only conditions `style` carries, and they are here for
  // history. A CSS `style` attribute holds declarations, not selectors. Any
  // further condition belongs in a class, not here.
  hover?: StyleDeclarations
  active?: StyleDeclarations

  // Custom properties. A declaration here is in scope for `var()` on this
  // element and on everything below it, the same as in CSS.
  //
  // A number declares its own plain text, so `{ "--pad": 8 }` declares `8`.
  // The name needs both dashes: `"-pad"` is a type error rather than a
  // variable that silently never resolves.
  [name: `--${string}`]: string | number | undefined
}

/**
 * What `hover` and `active` may hold.
 *
 * No nesting, and no custom properties. A declaration inside a state has
 * nothing to apply to, because the cascade reads variables from the element
 * itself, not from one of its states.
 */
export type StyleDeclarations = Omit<
  StyleDesc,
  "hover" | "active" | `--${string}`
>

// Element types supported by GPUIX
export type ElementType =
  | "div"
  | "text"
  | "img"
  | "svg"
  | "canvas"
  | "input"
  | "textarea"
  | "anchored"
  | "code"
  | "diff"
  | "markdown"
  | "virtual-list"

// ── Theme ────────────────────────────────────────────────────────────

/** Colours for one syntax capture class each. Every field is a CSS colour. */
export interface SyntaxTheme {
  comment?: string
  keyword?: string
  string?: string
  stringSpecial?: string
  escape?: string
  number?: string
  boolean?: string
  typeName?: string
  typeBuiltin?: string
  constructor?: string
  function?: string
  functionBuiltin?: string
  macroName?: string
  property?: string
  constant?: string
  variable?: string
  variableSpecial?: string
  parameter?: string
  operator?: string
  punctuation?: string
  tag?: string
  attribute?: string
  label?: string
  invalid?: string
}

/**
 * Every number that decides layout in the native text components.
 *
 * These live in the theme, not in Rust constants, so tuning a row height or a
 * heading scale is a React re-render and needs no native rebuild.
 */
export interface GpuixMetrics {
  // Code blocks
  codeTextSize?: number
  codeLineHeight?: number
  codePaddingX?: number
  codePaddingY?: number
  codeRadius?: number
  codeHeaderPaddingY?: number
  codeHeaderTextSize?: number
  codeGutterDigitWidth?: number
  codeGutterPaddingRight?: number
  codeGutterMinWidth?: number

  // Diffs
  diffTextSize?: number
  diffLineHeight?: number
  diffFileHeaderHeight?: number
  diffHunkHeaderHeight?: number
  diffNoticeHeight?: number
  diffBodyBottomPad?: number
  diffGutterWidth?: number
  diffMarkerWidth?: number
  diffAccentBarWidth?: number
  diffRowPaddingX?: number

  // Markdown
  mdTextSize?: number
  mdLineHeight?: number
  mdBlockGap?: number
  /** `[h1, h2, h3, h4to6]`. A shorter array leaves the rest at their defaults. */
  mdHeadingSizes?: number[]
  mdHeadingLineHeights?: number[]
  mdTableCellPadding?: number
  mdTableMinColumnWidth?: number
  mdTableMinColumnContent?: number
  mdInlineCodeRadius?: number
}

/**
 * Theme tokens for the native text components. Every field is optional and
 * layers on top of the built-in dark theme (or light, via `appearance`).
 */
export interface GpuixTheme {
  appearance?: "dark" | "light"
  bg?: string
  border?: string
  text?: string
  textMuted?: string
  textFaint?: string
  textDim?: string
  accent?: string
  caret?: string
  codeText?: string
  codeWash?: string
  diffAdd?: string
  diffDel?: string
  diffHunkBg?: string
  fontSans?: string
  fontMono?: string
  syntax?: SyntaxTheme
  metrics?: GpuixMetrics
}

// Props passed to elements.
// Element IDs are auto-generated numeric IDs (not user-settable).
// Use React refs to get an element's ID: ref.current.id
export interface Props {
  style?: StyleDesc
  /**
   * Class tokens, separated by spaces, read by the root's resolver.
   *
   * `string | undefined` is the whole type, so `clsx` and `cn` need no special
   * handling. Without a resolver on the root this does nothing and warns once.
   * A declaration in `style` beats one from a class in every state.
   */
  className?: string
  children?: React.ReactNode
  ref?: React.Ref<PublicInstance>

  // ── Mouse events ───────────────────────────────────────────────
  onClick?: (event: EventPayload) => void
  onMouseDown?: (event: EventPayload) => void
  onMouseUp?: (event: EventPayload) => void
  onMouseEnter?: (event: EventPayload) => void
  onMouseLeave?: (event: EventPayload) => void
  onMouseMove?: (event: EventPayload) => void
  /** Fires when user clicks OUTSIDE this element. Use for "click outside to close". */
  onMouseDownOutside?: (event: EventPayload) => void

  // ── Keyboard events (need focus: autoFocus, or a click on the element) ──
  onKeyDown?: (event: EventPayload) => void
  onKeyUp?: (event: EventPayload) => void

  // ── Focus events ───────────────────────────────────────────────
  onFocus?: (event: EventPayload) => void
  onBlur?: (event: EventPayload) => void

  // ── Scroll events ──────────────────────────────────────────────
  onScroll?: (event: EventPayload) => void

  // ── Text editor events ─────────────────────────────────────────
  onChange?: (event: EventPayload) => void
  onSubmit?: (event: EventPayload) => void

  // ── Native component events ─────────────────────────────────────
  onToggleFile?: (event: EventPayload) => void
  onShowMore?: (event: EventPayload) => void
  onLineClick?: (event: EventPayload) => void
  onLinkClick?: (event: EventPayload) => void
  onVisibleRange?: (event: EventPayload) => void

  // ── Focus props ────────────────────────────────────────────────
  /** Take keyboard focus when the element first mounts. Required for `<input>`:
   *  without it, or a click, the field never receives key events. */
  autoFocus?: boolean
  /** Native GPUI tab order. Use 0 for normal keyboard focus. */
  tabIndex?: number
  /** Stable locator id for automation. */
  testId?: string
  /** Internal native animation description used by motion components. */
  motion?: MotionProps
}

// Props for native text editor elements.
export interface InputProps extends Props {
  /** External editor value. Native edits apply immediately and report through onChange. */
  value?: string
  placeholder?: string
  readOnly?: boolean
  theme?: GpuixTheme
}

export interface TextareaProps extends InputProps {
  minRows?: number
  maxRows?: number
}

/** A variable-height list that builds only rows near its viewport. */
export interface VirtualListProps {
  style?: StyleDesc
  children?: React.ReactNode
  ref?: React.Ref<PublicInstance>
  alignment?: "top" | "bottom"
  followTail?: boolean
  overdraw?: number
  estimatedItemHeight?: number
  /** Logical row count. When set, `children` is only the mounted window. */
  itemCount?: number
  /** Logical index of `children[0]`. Ignored when `itemCount` is unset. */
  windowStart?: number
  onVisibleRange?: (event: EventPayload) => void
}

// Props for native <img> rendering.
export interface ImgProps extends Props {
  src?: string
  objectFit?: "fill" | "contain" | "cover" | "scaleDown" | "none"
  alt?: string
}

// Props for monochrome SVGs loaded from local files and tinted by style.color.
export interface SvgProps extends Props {
  src?: string
}

// Props for the <code> custom element — a syntax-highlighted code block.
export interface CodeProps extends Props {
  /** The source to display. Rendered one div per line at an exact line height. */
  code?: string
  /** Language alias such as "ts", "rust", "bash". Beats `path` for detection. */
  language?: string
  /** File path, used for extension-based language detection. */
  path?: string
  showLineNumbers?: boolean
  /** Header strip with the language tag. Defaults to true when `language` is set. */
  showHeader?: boolean
  theme?: GpuixTheme
}

// Props for the <diff> custom element — a unified diff viewer.
export interface DiffProps extends Props {
  /** A unified git patch (the output of `git diff`). */
  patch?: string
  /** Highlight the words that changed inside paired +/- lines. */
  wordDiff?: boolean
  /** File paths rendered as a header only. Collapsed bodies cost one row. */
  collapsedPaths?: string[]
  /**
    * Use the virtualized `list()` scroller. Off by default so a parent
    * list can be the only scroll container. Requires a bounded height.
   */
  scroll?: boolean
  /** Paint this many line rows, then a Show more row. */
  maxLines?: number
  theme?: GpuixTheme
  /** Fires when a file header is clicked. `event.value` is the file path. */
  onToggleFile?: (event: EventPayload) => void
  /** Fires when Show more is clicked. `event.value` is the hidden line count. */
  onShowMore?: (event: EventPayload) => void
  /** Fires when a diff line is clicked. `event.value` is the line text,
   *  `event.oldLine` / `event.newLine` are its line numbers. */
  onLineClick?: (event: EventPayload) => void
}

// Props for the <markdown> custom element.
export interface MarkdownProps extends Props {
  /** GitHub-flavoured markdown. Tables, strikethrough and task lists are on. */
  source?: string
  theme?: GpuixTheme
  /** Fires when a block containing links is clicked. `event.value` is the URL. */
  onLinkClick?: (event: EventPayload) => void
}

// Props for the <anchored> custom element.
export interface AnchoredProps extends Props {
  position?: { x: number; y: number }
  side?: "top" | "right" | "bottom" | "left"
  align?: "start" | "center" | "end"
  gap?: number
  anchor?:
    | "topLeft"
    | "topCenter"
    | "topRight"
    | "rightCenter"
    | "bottomRight"
    | "bottomCenter"
    | "bottomLeft"
    | "leftCenter"
  offset?: { x: number; y: number }
  fit?: "switch" | "snap"
  snapMargin?: number
  deferred?: boolean
  priority?: number
  occlude?: boolean
}

/// Interface for the renderer that receives mutations from the reconciler.
/// Implemented by the real napi GpuixRenderer and by TestRenderer (which
/// delegates to native TestGpuixRenderer for tests).
export interface NativeRenderer {
  createElement(id: number, elementType: string): void
  destroyElement(id: number): Array<number>
  appendChild(parentId: number, childId: number): void
  removeChild(parentId: number, childId: number): void
  insertBefore(parentId: number, childId: number, beforeId: number): void
  setStyle(id: number, styleJson: string | object): void
  setText(id: number, content: string): void
  setEventListener(id: number, eventType: string, hasHandler: boolean): void
  setRoot(id: number): void
  commitMutations(): void
  setCustomProp(id: number, key: string, valueJson: string | object | number | boolean | null): void
  /** Apply a batch of mutations in a single FFI call. Returns destroyed IDs. */
  applyBatch?(json: string): Array<number>

  // ── Focus API ──────────────────────────────────────────────────
  focusElement?(elementId: number): void
  blur?(): void

  // ── Scroll API ─────────────────────────────────────────────────
  /** Set the scroll offset of a scrollable element (overflow: "scroll").
   *  x and y are negative pixel values (scroll down = more negative y). */
  scrollTo?(elementId: number, x: number, y: number): void
  /** Scroll a child into view by its index in the children list. */
  scrollToItem?(elementId: number, index: number): void
  /** Get the current scroll offset [x, y] or null if element is not scrollable. */
  getScrollOffset?(elementId: number): Array<number> | null

  // ── Selection API ──────────────────────────────────────────────
  /** The current text selection joined in document order, or null. */
  getSelectedText?(): string | null
  /** Drop the current selection. */
  clearSelection?(): void

  // ── Window API ─────────────────────────────────────────────────
  getWindowSize?(): { width: number; height: number }
  setWindowTitle?(title: string): void
  setDebugFrameOverlay?(mode: DebugFrameOverlayMode): string
  getDebugFrameOverlay?(): string
  cycleDebugFrameOverlay?(): string
  resetDebugFrameOverlayStats?(): void
  getDebugFrameOverlayStats?(): DebugFrameOverlayStats
}

export type DebugFrameOverlayMode = "hidden" | "minimal" | "full"

export interface DebugFrameOverlayStats {
  currentMs?: number
  p90Ms?: number
  p99Ms?: number
  maxMs?: number
  frames: number
  samples: number
}

export type EventHandlerMap = Map<
  number,
  Map<string, (event: EventPayload) => void>
>

export interface ElementIdAllocator {
  nextElementId: number
}

// One React root. Event handlers stay on this object so two live roots
// can both use id 1. Ids come from an allocator that lives with the
// NativeRenderer, so a remount on the same renderer cannot reuse them.
export interface Container {
  renderer: NativeRenderer
  ids: ElementIdAllocator
  eventHandlers: EventHandlerMap
  /** How this root reads `className`, or `null` when nothing registered one. */
  classNames: ClassNameCache | null
  /** Whether this root has already warned that it has no resolver. */
  warnedAboutClassName: boolean
}

/**
 * Reads one class token, such as `p-4`, into the style it declares.
 *
 * Returns `null` for a token it does not know. The token never holds a space,
 * because the root splits the string before it calls this.
 */
export type ClassNameResolver = (token: string) => StyleDesc | null

/** A resolver with what a root has already asked it. */
export interface ClassNameCache {
  resolve: ClassNameResolver
  /** One entry per token, holding `null` for a token the resolver rejected. */
  tokens: Map<string, StyleDesc | null>
  /** Whole strings, bounded, least recently used out first. */
  strings: Map<string, StyleDesc>
}

/** Options for a root. */
export interface RootOptions {
  /**
   * How to read `className` on this root's elements.
   *
   * This is an option on the root rather than a global, so two roots can hold
   * different resolvers and two test files can run at once.
   */
  resolveClassName?: ClassNameResolver
}

// Instance — minimal handle for React's reconciler.
// The real element state lives in Rust's RetainedTree.
export interface Instance {
  id: number
  type: ElementType
  props: Props
}

// Text instance for raw text nodes
export interface TextInstance {
  id: number
  text: string
  parentId: number | null
}

// Public instance exposed via refs
export type PublicInstance = Instance

// Host context passed down the tree
export interface HostContext {
  isInsideText: boolean
}
