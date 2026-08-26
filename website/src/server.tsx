// Entry point for the gpuix docs website.
// Mounts holocron docs and adds a /gh redirect.

import { Spiceflow } from 'spiceflow'
import { app as holocronApp } from '@holocron.so/vite/app'

export const app = new Spiceflow()
  .get('/gh', () => {
    return Response.redirect('https://github.com/remorses/gpuix', 302)
  })
  .use(holocronApp)

export default {
  async fetch(request: Request): Promise<Response> {
    return app.handle(request)
  },
}
