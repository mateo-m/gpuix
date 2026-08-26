---
"@gpuix/native": patch
"@gpuix/react": patch
---

Make cmd-c copy the selected text.

The copy handler was a key listener on a hidden element. A key event only
visits the elements between the window root and the focused element, so the
handler never ran. The handler is now a keystroke observer on the window. A
focused input still copies its own text first.
