---
'@gpuix/native': patch
'@gpuix/react': patch
---

Make `fill()` and `press()` work against desktop apps started with `launch()`.

```ts
const app = await launch({ command: 'bun', args: ['app.tsx'] })
await app.getByTestId('composer').fill('hello gpuix')
await app.getByTestId('composer').press('enter')
```

Live keyboard automation now uses GPUI's real window input pipeline, including native input and textarea editing.
