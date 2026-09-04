#!/usr/bin/env node

// Downloads the example app and exposes the `gpuix new` command.

import { spawn } from 'node:child_process'
import fs from 'node:fs/promises'
import path from 'node:path'
import { createRequire } from 'node:module'
import { pathToFileURL } from 'node:url'
import { goke } from 'goke'
import JSZip from 'jszip'

const require = createRequire(import.meta.url)
const packageJson = require('../package.json') as { version: string }
const archiveUrl = 'https://github.com/remorses/gpuix/archive/refs/heads/main.zip'
const templatePrefix = 'gpuix-main/example-app/'
const reactPackageUrl = 'https://registry.npmjs.org/@gpuix%2Freact/latest'

export async function createGpuixApp({
  targetDirectory,
  githubToken,
}: {
  targetDirectory: string
  githubToken?: string
}) {
  try {
    await fs.stat(targetDirectory)
    throw new Error(`Directory "${targetDirectory}" already exists`)
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error
  }

  const headers = githubToken
    ? { Authorization: `Bearer ${githubToken}` }
    : undefined
  const [archiveResponse, reactPackageResponse] = await Promise.all([
    fetch(archiveUrl, { headers }),
    fetch(reactPackageUrl),
  ])

  if (!archiveResponse.ok) {
    throw new Error(`Failed to download template: ${archiveResponse.status}`)
  }
  if (!reactPackageResponse.ok) {
    throw new Error(
      `Failed to resolve @gpuix/react: ${reactPackageResponse.status}`,
    )
  }

  const reactPackage = (await reactPackageResponse.json()) as { version: string }
  const zip = await JSZip.loadAsync(await archiveResponse.arrayBuffer())
  const templateFiles = Object.values(zip.files).filter(
    (entry) => entry.name.startsWith(templatePrefix) && !entry.dir,
  )

  if (templateFiles.length === 0) {
    throw new Error('example-app was not found in the downloaded archive')
  }

  await fs.mkdir(targetDirectory, { recursive: true })
  try {
    await Promise.all(
      templateFiles.map(async (entry) => {
        const relativePath = entry.name.slice(templatePrefix.length)
        const targetPath = path.join(targetDirectory, relativePath)
        await fs.mkdir(path.dirname(targetPath), { recursive: true })
        await fs.writeFile(targetPath, await entry.async('uint8array'))
      }),
    )

    const targetPackagePath = path.join(targetDirectory, 'package.json')
    const targetPackage = JSON.parse(
      await fs.readFile(targetPackagePath, 'utf8'),
    )
    targetPackage.name = path.basename(targetDirectory)
    targetPackage.dependencies['@gpuix/react'] = `^${reactPackage.version}`
    await fs.writeFile(
      targetPackagePath,
      `${JSON.stringify(targetPackage, null, 2)}\n`,
    )
  } catch (error) {
    await fs.rm(targetDirectory, { recursive: true, force: true })
    throw error
  }
}

async function installDependencies(targetDirectory: string) {
  await new Promise<void>((resolve, reject) => {
    const child = spawn('bun', ['install'], {
      cwd: targetDirectory,
      stdio: 'inherit',
    })
    child.on('error', reject)
    child.on('exit', (code) => {
      if (code === 0) resolve()
      else reject(new Error(`bun install exited with code ${code}`))
    })
  })
}

export const cli = goke('gpuix')

cli
  .command('new <name>', 'Create a GPUIX app from the official example')
  .option('--no-install', 'Skip `bun install`')
  .example('gpuix new my-app')
  .action(async (name, options, { console, process }) => {
    const targetDirectory = path.resolve(process.cwd, name)
    console.log(`Creating ${name}...`)
    await createGpuixApp({
      targetDirectory,
      githubToken: process.env.GITHUB_TOKEN,
    })

    if (!options.noInstall) {
      console.log('Installing dependencies...')
      await installDependencies(targetDirectory)
    }

    console.log(`Created ${name}`)
    console.log(`\nNext steps:\n  cd ${name}\n  bun run dev`)
  })

cli.help()
cli.completions()
cli.version(packageJson.version)

export async function isEntryPoint(entryPath?: string) {
  if (!entryPath) return false
  const realEntryPath = await fs.realpath(entryPath).catch(() => entryPath)
  return pathToFileURL(realEntryPath).href === import.meta.url
}

if (await isEntryPoint(process.argv[1])) {
  await cli.parse()
}
