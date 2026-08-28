---
"@gpuix/native": patch
---

Keep the scroll offset of a scrolled screen in its frozen view
transition copy. A frame could build between the capture and the start
call, after the update already removed the old screen from the tree.
That frame dropped the scroll handle of the screen, so its frozen copy
painted from a fresh handle at offset zero, and the list flashed back
to the top for the length of the transition. The cleanup now also keeps
the state of every id inside a pending capture.
