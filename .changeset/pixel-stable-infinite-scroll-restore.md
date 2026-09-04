---
'@gpuix/native': minor
'@gpuix/react': minor
---

Add a pixel-preserving scroll anchor API for `<virtual-list>`, so an infinite-scroll history can prepend a page without moving the message the reader is looking at.

```tsx
renderer.scrollToItem(listId, index, offsetInItem)  // offset in px, may be negative
renderer.getListScrollTop(listId)  // [itemIndex, offsetInItemPx, viewportHeightPx] or null
```

Two problems made the old `scrollToItem(listId, index)` restore jump:

- it snapped the anchor row to the viewport top with offset `0`, so the row moved by up to the height of the loading row the reader was waiting in
- it applied `scroll_to` immediately, while the just-committed prepend only reaches `gpui::ListState` on the next render. The splice then shifted the freshly restored anchor a second time, landing it on the wrong row

Virtual-list `scrollToItem` calls are now queued and applied on the next render, **after** that frame's child splice. A **negative** `offsetInItem` anchors the viewport top above the row; gpui resolves it at layout time by measuring the freshly inserted rows above it, so the restore is exact rather than estimate-based:

```
while the reader waits in the loading row          after the page lands
┌────────────────────┐                             ┌────────────────────┐
│ ░ loading row ░░░░ │ ◄─ gpui's anchor            │ new message 7      │ ◄─ measured above
├────────────────────┤                             ├────────────────────┤
│ message A          │ ◄─ what the reader reads    │ message A          │ ◄─ same pixel
│ message B          │                             │ message B          │
└────────────────────┘                             └────────────────────┘
        scrollToItem(indexOfA, offsetInVoid - EDGE_HEIGHT)
```

`getListScrollTop` reports the logical anchor `[itemIndex, offsetInItemPx, viewportHeightPx]`, which is exact even while row heights are still estimates, unlike the pixel-space `getScrollOffset`. An `itemIndex` equal to the item count is gpui's at-end sentinel (a bottom-aligned list resting at its very end); the viewport height converts it into a position relative to the trailing rows.

`examples/infinite-chat.tsx` uses the new API for both directions: prepended pages keep the message under the loading row pixel-stable, and appended pages fill the blank space below the text the reader is on instead of pushing it up (or, at the at-end sentinel, snapping the view to the bottom of the new loading row).
