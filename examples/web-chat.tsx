/** Browser entry for the full GPUIX chat example. */

import React from 'react'
import { render } from '@gpuix/react'
import { ChatApp } from './chat'

// Do NOT add `import.meta.hot.accept("./chat", ...)` here. Bun runs an
// importer's dependency-accept callback even when the imported module already
// self-accepted for React Fast Refresh, so the callback would remount the tree
// on top of a successful refresh and throw away every `useState`. When an edit
// is not a refresh boundary, Bun reloads the page, which is the right fallback.
render(<ChatApp includeSafeMdx />, {
  title: 'GPUIX Chat',
  width: 1180,
  height: 820,
  debugFrameOverlay: 'full',
})

document.getElementById('gpuix-loader')?.remove()
