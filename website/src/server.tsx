// Entry point for the gpuix docs website.
// Mounts holocron docs, the browser chat example, and a /gh redirect.

import { Spiceflow } from 'spiceflow'
import { app as holocronApp } from '@holocron.so/vite/app'
import chatExampleHtml from '../../examples/web.html?raw'

const chatHtml = chatExampleHtml.replace('./web-chat.tsx', '/chat-example/chat.js')

export const app = new Spiceflow()
  .get('/gh', () => {
    return Response.redirect('https://github.com/remorses/gpuix', 302)
  })
  .get('/chat-example', ({ request }) => {
    if (!new URL(request.url).pathname.endsWith('/')) {
      return Response.redirect(new URL('/chat-example/', request.url), 308)
    }
    return new Response(chatHtml, {
      headers: {
        'Content-Type': 'text/html; charset=utf-8',
        'Cross-Origin-Embedder-Policy': 'require-corp',
        'Cross-Origin-Opener-Policy': 'same-origin',
      },
    })
  })
  .use(holocronApp)

export default {
  fetch: app.handle.bind(app),
}
