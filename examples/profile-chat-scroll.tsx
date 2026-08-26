// Profile ChatApp wheel frames. Run with bun --cpu-prof.
import React from 'react'
import { connectTest } from '@gpuix/react/automation'
import { applyMacCpuThrottleFromEnv, createTestRoot } from '@gpuix/react/testing'
import { ChatApp } from './chat'

applyMacCpuThrottleFromEnv()

const root = createTestRoot()

const mountStart = performance.now()
const count = Number(process.env.TURNS ?? 10_000)
const safe = process.env.SAFE_MDX === '1'
root.render(<ChatApp turnCount={count} includeSafeMdx={safe} />)
console.log(`mount ${(performance.now() - mountStart).toFixed(1)}ms`)

if (process.env.INTERACT === '1') {
  const treeStart = performance.now()
  root.renderer.getAutomationTree()
  console.log(`getTree ${(performance.now() - treeStart).toFixed(1)}ms`)

  const idle: number[] = []
  for (let i = 0; i < 20; i++) {
    const start = performance.now()
    root.renderer.flush()
    idle.push(performance.now() - start)
  }
  idle.sort((a, b) => a - b)
  console.log(
    `idle flush mean=${(idle.reduce((a, b) => a + b, 0) / idle.length).toFixed(2)}ms max=${idle[19]!.toFixed(2)}ms`,
  )

  const app = await connectTest(root.renderer)
  const collapseStart = performance.now()
  await app.getByTestId('sidebar-collapse').click()
  console.log(`collapse ${(performance.now() - collapseStart).toFixed(1)}ms`)
  const expandStart = performance.now()
  await app.getByTestId('sidebar-expand').click()
  console.log(`expand ${(performance.now() - expandStart).toFixed(1)}ms`)
  process.exit(0)
}

if (process.env.MOUNT_ONLY === '1') process.exit(0)

const samples: number[] = []
for (let i = 0; i < 40; i++) {
  const start = performance.now()
  root.renderer.dispatchScrollWheel(700, 400, 0, i % 2 === 0 ? -160 : 160)
  samples.push(performance.now() - start)
}
samples.sort((a, b) => a - b)
const mean = samples.reduce((a, b) => a + b, 0) / samples.length
console.log(
  `wheel n=${samples.length} mean=${mean.toFixed(2)}ms p50=${samples[20]!.toFixed(2)}ms p95=${samples[38]!.toFixed(2)}ms max=${samples[39]!.toFixed(2)}ms`,
)
