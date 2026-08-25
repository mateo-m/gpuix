---
"@gpuix/native": patch
"@gpuix/react": patch
---

Animate `height` to `auto`

A motion `height` now takes `"auto"` at either end of the animation. `auto` is
the height the content takes, and only layout knows that number, so the element
measures its content every frame and interpolates against the measurement. An
animation that opens a panel follows content that changes while it runs.

The measurement happens before the element knows its own width, so declare a
pixel `width` to make it exact. Without one the content measures unwrapped,
which reads short for text that would have wrapped.
