/// Find-bar state for the `highlight` prop, plus the same matcher in JS.
///
/// `findRanges` mirrors `matches_in` in `packages/native/src/text/search.rs`.
/// It exists because a `<virtual-list>` never builds off-screen rows, so native
/// cannot count or scroll to a match that was never painted. The app owns the
/// row data and must do that itself; it should not have to write a second,
/// subtly different matcher to do it.

import { useCallback, useMemo, useState } from "react"
import type { EventPayload } from "@gpuix/native"
import type { HighlightSpec, Props } from "../types/host.js"

export interface TextSearchOptions {
  query: string
  caseSensitive?: boolean
  wholeWord?: boolean
  color?: string
  activeColor?: string
  radius?: number
  /**
   * Match bookkeeping you did yourself, for content native cannot see whole.
   *
   * A `<virtual-list>` mounts a window of its rows, so native both counts and
   * numbers that window only. Supplying one number without the other is always
   * wrong, which is why they travel together: `total` replaces the count native
   * reports, and `indexOffset` is how many matches sit above the window, so
   * `next()` still lands on the right row.
   *
   * Both are MATCH counts, not row indices. Sum `findRanges` over your rows.
   */
  matches?: { total: number; indexOffset: number }
}

export interface TextSearch {
  /** Spread onto the container to search: `<div {...search.props}>`. */
  props: Pick<Props, "highlight" | "onHighlight">
  /** Matches in retained text, or the `total` you supplied. */
  total: number
  /** Index of the active match, clamped into `[0, total)`. */
  active: number
  next(): void
  previous(): void
  /** Jump to a specific match. Out-of-range values are ignored. */
  goTo(index: number): void
}

/**
 * Drive a find bar.
 *
 * `next` and `previous` are plain event handlers, so nothing here needs an
 * effect. The count arrives through `onHighlight` after the build that resolved
 * it, never during, so a `setState` in that handler cannot re-enter the build.
 */
export function useTextSearch(options: TextSearchOptions): TextSearch {
  const {
    query,
    caseSensitive,
    wholeWord,
    color,
    activeColor,
    radius,
    matches: supplied,
  } = options
  const [reported, setReported] = useState(0)
  const [requested, setRequested] = useState(0)

  // An empty query paints nothing, so it reports nothing, so the last count
  // would survive and the bar would still read "1/2" on an empty input.
  const total = query.length === 0 ? 0 : (supplied?.total ?? reported)
  // Clamped on read rather than in an effect: the count and the cursor change
  // in the same render, and a stale cursor must never paint a wrong match.
  const active = total === 0 ? 0 : Math.min(requested, total - 1)

  const highlight = useMemo<HighlightSpec | null>(() => {
    if (query.length === 0) return null
    return {
      query,
      caseSensitive,
      wholeWord,
      color,
      activeColor,
      radius,
      activeIndex: active,
      matchIndexOffset: supplied?.indexOffset,
    }
  }, [
    query,
    caseSensitive,
    wholeWord,
    color,
    activeColor,
    radius,
    active,
    supplied?.indexOffset,
  ])

  const onHighlight = useCallback((event: EventPayload) => {
    setReported(event.matchCount ?? 0)
  }, [])

  const goTo = useCallback(
    (index: number) => {
      if (index < 0 || index >= total) return
      setRequested(index)
    },
    [total]
  )
  const next = useCallback(() => {
    if (total === 0) return
    setRequested((current) => (Math.min(current, total - 1) + 1) % total)
  }, [total])
  const previous = useCallback(() => {
    if (total === 0) return
    setRequested((current) => (Math.min(current, total - 1) + total - 1) % total)
  }, [total])

  return useMemo(
    () => ({
      props: { highlight, onHighlight },
      total,
      active,
      next,
      previous,
      goTo,
    }),
    [highlight, onHighlight, total, active, next, previous, goTo]
  )
}

export interface FindRangesOptions {
  text: string
  query: string
  caseSensitive?: boolean
  wholeWord?: boolean
}

// `Alphabetic`, not `L`: Rust's `char::is_alphanumeric` uses the Unicode
// Alphabetic property, which also covers combining marks such as U+0345. With
// `\p{L}` the two matchers disagree on `wholeWord` and a virtual-list count
// stops matching what native actually paints.
const WORD_CHAR = /[\p{Alphabetic}\p{N}_]/u

/** True when the code point ending at `end` is a word character. */
function wordCharBefore(text: string, end: number): boolean {
  if (end <= 0) return false
  // Read a whole code point: a lone surrogate is never a letter, so checking
  // one UTF-16 unit would call an astral letter a word boundary and disagree
  // with the native matcher, which reads scalars.
  const low = text.charCodeAt(end - 1)
  const start = low >= 0xdc00 && low <= 0xdfff && end >= 2 ? end - 2 : end - 1
  return WORD_CHAR.test(text.slice(start, end))
}

/** True when the code point starting at `start` is a word character. */
function wordCharAt(text: string, start: number): boolean {
  if (start >= text.length) return false
  const codePoint = text.codePointAt(start)
  if (codePoint === undefined) return false
  return WORD_CHAR.test(String.fromCodePoint(codePoint))
}

/**
 * Lowercase `text` and record, for every folded unit, the index of the original
 * character that produced it.
 *
 * Lowercasing alone is not enough: `İ`.toLowerCase() is two units, so every
 * index after it shifts. The map converts a hit in folded space back exactly,
 * the same way `fold` does in `packages/native/src/text/search.rs`.
 */
function fold(text: string): { folded: string; map: number[] } {
  let folded = ""
  const map: number[] = []
  for (let index = 0; index < text.length; ) {
    const codePoint = text.codePointAt(index)
    const char = String.fromCodePoint(codePoint ?? 0)
    const lower = char.toLowerCase()
    for (let unit = 0; unit < lower.length; unit++) map.push(index)
    folded += lower
    index += char.length
  }
  map.push(text.length)
  return { folded, map }
}

/**
 * Non-overlapping `[start, end)` matches of `query` in `text`, in UTF-16 code
 * units. Same contract as the native matcher: leftmost first, non-overlapping,
 * Unicode **lowercasing** (not full case folding, so `ﬀ` does not match `ff`),
 * and a word boundary is any code point that is not Unicode Alphabetic, a
 * digit, or `_`.
 */
export function findRanges(options: FindRangesOptions): Array<[number, number]> {
  const { text, query, caseSensitive = false, wholeWord = false } = options
  if (query.length === 0) return []

  // Identity map in the case-sensitive path, so there is one loop below.
  const { folded, map } = caseSensitive ? { folded: text, map: null } : fold(text)
  const needle = caseSensitive ? query : query.toLowerCase()

  const out: Array<[number, number]> = []
  let from = 0
  for (;;) {
    const at = folded.indexOf(needle, from)
    if (at === -1) break
    from = at + needle.length
    const start = map ? map[at]! : at
    const end = map ? map[from]! : from
    if (start >= end) continue
    if (wholeWord && (wordCharBefore(text, start) || wordCharAt(text, end))) continue
    out.push([start, end])
  }
  return out
}
