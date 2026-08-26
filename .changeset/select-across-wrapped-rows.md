---
"@gpuix/native": patch
---

Cover the first glyph of a wrapped row in the selection wash.

The wash walks the visual rows of a paragraph with `position_for_index`.
The index at a soft-wrap boundary reports its position on the earlier row,
so each walk started one glyph into the next row and the wash missed that
glyph. A continuation row now stretches back to the leading edge of the
layout.
