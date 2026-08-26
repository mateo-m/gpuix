---
"@gpuix/react": patch
---

Keep events alive across a `bun --hot` remount.

The map from renderer to container lived in the module, and the native
event callback keeps the module instance that created it. A hot reload
evaluates the module again, so the new tree registered its handlers in a
new map while native events searched the old one, and every click died.
The map now lives on `globalThis`, so both module instances share it. The
`onEvent` option also follows the latest `render()` call instead of the
first one.
