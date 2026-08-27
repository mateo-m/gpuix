---
"@gpuix/native": minor
"@gpuix/react": minor
---

Add `scroll-behavior`, `scroll-snap-*`, `scroll-initial-target` and the
logical `scroll-margin` and `scroll-padding` variants.

`scroll-behavior: smooth` turns a programmatic scroll into a glide on the
offset. `scrollTo` and `scrollIntoView` also take a `behavior` argument,
`auto`, `instant` or `smooth`, like the web option. A wheel move that takes
the box away from the glide cancels it.

`scroll-snap-type` on a box and `scroll-snap-align` on its descendants snap
the box when a scroll comes to rest. `mandatory` always snaps to the nearest
position. `proximity`, the default, snaps within half a viewport.
`scroll-snap-stop: always` on an area stops a long scroll that would pass
over it. The snap area grows by the `scroll-margin` of the element, and the
viewport shrinks by the `scroll-padding` of the box, as in CSS.

`scroll-initial-target: nearest` scrolls the ancestors of an element to it
once, on the first frame after it paints.

`scroll-margin` and `scroll-padding` take the logical variants: `-block`,
`-block-start`, `-block-end`, `-inline`, `-inline-start` and `-inline-end`.
GPUIX lays text out horizontally, left to right, so block is vertical and
inline is horizontal.

Out of scope: `scroll-timeline-*` needs CSS animations and
`scroll-marker-group` needs the `::scroll-marker` pseudo-element. GPUIX has
neither, so these properties stay unread.
