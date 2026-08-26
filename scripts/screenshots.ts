/**
 * Regenerate the screenshots the README links to.
 *
 * Test output lands in gitignored `screenshots/` folders and churns on every
 * run. Only the curated set below is committed, so the README never shows a
 * stale frame and the repo never carries a binary diff per test run.
 *
 *   bun scripts/screenshots.ts
 */

import { spawnSync } from 'node:child_process'
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const OUT = path.join(ROOT, 'docs', 'images')

/** `[source screenshot, committed name]`, relative to the repo root. */
const CURATED: [string, string][] = [
  ['example-app/screenshots/todo.png', 'todo-app.png'],
  ['examples/screenshots/chat-top.png', 'chat-app.png'],
  ['packages/react/screenshots/showcase.png', 'showcase.png'],
  ['packages/react/screenshots/markdown-document.png', 'markdown.png'],
  ['packages/react/screenshots/code-typescript.png', 'code.png'],
  ['packages/react/screenshots/diff-multi-file.png', 'diff.png'],
  ['packages/react/screenshots/selection-after.png', 'selection.png'],
  ['packages/react/screenshots/metrics-roomy.png', 'metrics.png'],
]

function run(command: string, args: string[], cwd: string) {
  console.log(`[screenshots] ${command} ${args.join(' ')}`)
  const result = spawnSync(command, args, { cwd, stdio: 'inherit' })
  if (result.status !== 0) {
    console.error(`[screenshots] failed: ${command} ${args.join(' ')}`)
    process.exit(1)
  }
}

console.log('[screenshots] rendering visual tests')
run('bun', ['run', 'test', 'showcase'], path.join(ROOT, 'packages', 'react'))
run('bun', ['run', 'test', 'markdown'], path.join(ROOT, 'packages', 'react'))
run('bun', ['run', 'test', 'code'], path.join(ROOT, 'packages', 'react'))
run('bun', ['run', 'test', 'diff-native'], path.join(ROOT, 'packages', 'react'))
run('bun', ['run', 'test', 'chat'], path.join(ROOT, 'examples'))
// The todo example has no test suite. It captures itself through the
// automation client, with the motion clock paused.
run('bun', ['run', 'screenshot'], path.join(ROOT, 'example-app'))

fs.mkdirSync(OUT, { recursive: true })
for (const [from, name] of CURATED) {
  const source = path.join(ROOT, from)
  if (!fs.existsSync(source)) {
    console.error(`[screenshots] missing ${from} — did its test run?`)
    process.exit(1)
  }
  const target = path.join(OUT, name)
  // A 256-colour palette is lossless in practice for flat dark UI (no
  // gradients to band) and cuts the committed size by about two thirds.
  const shrunk = spawnSync(
    'magick',
    [source, '-strip', '-colors', '256', '-define', 'png:compression-level=9', target],
    { stdio: 'inherit' }
  )
  if (shrunk.status !== 0) {
    console.warn('[screenshots] magick unavailable, copying uncompressed')
    fs.copyFileSync(source, target)
  }
  const kb = Math.round(fs.statSync(target).size / 1024)
  console.log(`[screenshots] ${name} (${kb} KB)`)
}
console.log(`[screenshots] wrote ${CURATED.length} files to docs/images`)
