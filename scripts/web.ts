/**
 * Build the browser Wasm target and serve the example with HMR.
 *
 * Bun's frontend dev server bundles `examples/web.html`, watches its module
 * graph, and runs the React Fast Refresh transform. An edit to
 * `examples/chat.tsx` keeps `useState`, so the GPUI canvas, the wasm module,
 * and the scroll position all survive the update.
 *
 * The wasm half must never re-evaluate. `WebGpuixRenderer::init` fails with
 * "GPUIX web is already running" once `WEB_APP` is set, and `gpui_web` appends
 * its own canvas to `<body>`. What protects it is not that it lives in
 * `node_modules` — Bun bundles it into the same client registry as the app.
 * It is that Bun re-runs only the changed module and then walks *upward*
 * through its importers, so an unchanged dependency stays evaluated and cached.
 * Keep Wasm init in a module that can never become an HMR boundary and is never
 * explicitly accepted. Only a full page reload re-creates it, which is fine.
 *
 *   bun scripts/web.ts               # build the Wasm if it is missing, then serve
 *   bun scripts/web.ts --rebuild     # force the cargo + wasm-bindgen step first
 *   bun scripts/web.ts --build-only  # only cargo + wasm-bindgen, do not serve
 */

import { spawn } from "node:child_process"
import fs from "node:fs"
import path from "node:path"
import { fileURLToPath } from "node:url"
import page from "../examples/web.html"

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..")
const NATIVE = path.join(ROOT, "packages", "native")
const PACKAGE_OUTPUT = path.join(NATIVE, "wasm")
const WASM = path.join(NATIVE, "target", "wasm32-unknown-unknown", "release", "gpuix_native.wasm")

/**
 * `packages/native/.cargo/config.toml` links the Wasm with `--shared-memory`,
 * so its `WebAssembly.Memory` is `shared: true`. That needs SharedArrayBuffer,
 * which only exists in a cross-origin isolated document.
 */
const ISOLATION_HEADERS = {
  "Cross-Origin-Opener-Policy": "same-origin",
  "Cross-Origin-Embedder-Policy": "require-corp",
}

/**
 * `Bun.serve` has no way to add headers to an HTML route, so the bundled
 * document is registered on a private path and re-sent from `/` with the two
 * isolation headers. Only the top-level document needs them: `require-corp`
 * constrains cross-origin subresources, and every asset the dev server emits is
 * same-origin. Tracking: https://github.com/oven-sh/bun/issues/16873
 */
const DOCUMENT_PATH = "/__gpuix-document"

function run({ command, args, cwd }: { command: string; args: string[]; cwd: string }): Promise<void> {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, { cwd, stdio: "inherit" })
    child.on("error", reject)
    child.on("exit", (code) => {
      if (code === 0) resolve()
      else reject(new Error(`${command} exited with code ${code ?? 1}`))
    })
  })
}

async function buildWasm(): Promise<void> {
  console.log("web: building gpuix-native for wasm32-unknown-unknown")
  await run({
    command: "cargo",
    args: ["+nightly", "build", "--target", "wasm32-unknown-unknown", "--no-default-features", "--release", "--lib"],
    cwd: NATIVE,
  })

  fs.mkdirSync(PACKAGE_OUTPUT, { recursive: true })
  console.log("web: generating the @gpuix/native browser loader")
  await run({
    command: "wasm-bindgen",
    args: [WASM, "--target", "web", "--out-dir", PACKAGE_OUTPUT, "--out-name", "gpuix-web"],
    cwd: NATIVE,
  })
}

async function main() {
  const buildOnly = process.argv.includes("--build-only")
  // `browser.mjs` needs both halves of the wasm-bindgen output. Checking only
  // the `.wasm` lets a half-finished run skip the build and then fail at import.
  const missing = ["gpuix-web.js", "gpuix-web_bg.wasm"].some(
    (file) => !fs.existsSync(path.join(PACKAGE_OUTPUT, file)),
  )
  if (buildOnly || missing || process.argv.includes("--rebuild")) {
    await buildWasm()
  }
  if (buildOnly) {
    console.log(`web: generated ${path.relative(ROOT, PACKAGE_OUTPUT)}`)
    return
  }

  // `examples/` imports `@gpuix/react` through its `main`, so `dist` has to
  // exist. Run `bun run dev` in `packages/react` to keep it fresh while editing
  // the library itself.
  console.log("web: building @gpuix/react")
  await run({ command: "bun", args: ["run", "build"], cwd: path.join(ROOT, "packages", "react") })

  const server = Bun.serve({
    port: Number(process.env.PORT || 4173),
    routes: {
      [DOCUMENT_PATH]: page,
      "/": async (_request, self) => {
        const bundled = await fetch(new URL(DOCUMENT_PATH, self.url))
        const headers = new Headers(bundled.headers)
        for (const [key, value] of Object.entries(ISOLATION_HEADERS)) {
          headers.set(key, value)
        }
        return new Response(bundled.body, { status: bundled.status, headers })
      },
    },
    development: { hmr: true, console: true },
  })
  console.log(`web: ${server.url}`)
}

main().catch((error) => {
  console.error(`web: ${error instanceof Error ? error.message : String(error)}`)
  process.exit(1)
})
