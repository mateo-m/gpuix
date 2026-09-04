/** Browser entry for the bidirectional infinite history example. */

import React from 'react'
import { render } from '@gpuix/react'
import { InfiniteChatApp } from './infinite-chat'

// Do NOT add `import.meta.hot.accept("./infinite-chat", ...)` here. See the note
// in `web-chat.tsx`: Bun runs an importer's dependency-accept callback even when
// the imported module already self-accepted for Fast Refresh, so it would remount
// on top of a successful refresh and throw away every `useState`.
render(<InfiniteChatApp />, {
  title: 'GPUIX Infinite History',
  width: 920,
  height: 760,
  debugFrameOverlay: 'full',
})

document.getElementById('gpuix-loader')?.remove()
