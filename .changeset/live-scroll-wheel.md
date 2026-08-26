---
"@gpuix/native": patch
"@gpuix/react": patch
---

Make the automation `scrollWheel` method work on the live app.

The live renderer threw "scrollWheel is not live yet". It now dispatches a
real `ScrollWheelEvent` with a pixel delta through the window, the same
path a physical wheel takes.
