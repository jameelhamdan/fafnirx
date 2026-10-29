// Builds the web app's part of one module (or of every module with no argument) into dist-modules/<id>/, where the module's
// service serves it from: `npm run build:module -- video`. See vite.module.config.ts.
import { spawnSync } from 'node:child_process'
import { existsSync, readdirSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { join } from 'node:path'

const root = fileURLToPath(new URL('..', import.meta.url))
const ids = process.argv[2]
  ? [process.argv[2]]
  : readdirSync(join(root, 'src', 'modules'), { withFileTypes: true }).filter((e) => e.isDirectory()).map((e) => e.name)

for (const id of ids) {
  if (!existsSync(join(root, 'src', 'modules', id, 'index.ts'))) throw new Error(`src/modules/${id}/index.ts does not exist`)
  const run = spawnSync('npx', ['vite', 'build', '--config', 'vite.module.config.ts'], {
    cwd: root,
    env: { ...process.env, MODULE: id },
    stdio: 'inherit',
    shell: true,
  })
  if (run.status !== 0) process.exit(run.status ?? 1)
}
