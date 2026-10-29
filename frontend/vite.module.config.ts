import tailwindcss from '@tailwindcss/vite'
import reactPlugin from '@vitejs/plugin-react'
import fs from 'node:fs'
import { createRequire } from 'node:module'
import path from 'node:path'
import { defineConfig, type Plugin } from 'vite'

// Builds one module of the web app, apart from the app: `MODULE=video vite build --config vite.module.config.ts` (or
// `npm run build:module -- video`) makes dist-modules/video/index.js, its chunks and style.css, which the module's
// service serves (FAFNIRX_MODULE_UI). The app hands the module React, its own SDK and its ui kit when it loads it
// (src/sdk/runtime.ts), so those are not part of the build; whatever else the module imports (hls.js, icons) is.
const id = process.env.MODULE
if (!id || !/^[a-z0-9]+$/.test(id)) throw new Error('Name the module: MODULE=<id> (a folder of src/modules)')
const root = import.meta.dirname
const source = path.join(root, 'src', 'modules', id)
if (!fs.existsSync(path.join(source, 'index.ts'))) throw new Error(`src/modules/${id}/index.ts does not exist`)

/** Imports the app provides at run time. */
const provided = (name: string) => name === 'react' || name === 'react/jsx-runtime' || name === '@/sdk' || name.startsWith('@/components/ui/')

const files = (dir: string): string[] =>
  fs.readdirSync(dir, { withFileTypes: true }).flatMap((e) => (e.isDirectory() ? files(path.join(dir, e.name)) : /\.tsx?$/.test(e.name) ? [path.join(dir, e.name)] : []))

/** Everything React exports: the libraries a module brings (icons) import from it too, not only the module's own files. */
const react = createRequire(import.meta.url)
const everything = (name: string) => Object.keys(react(name)).filter((key) => /^[A-Za-z_$][\w$]*$/.test(key))

/** What the module's files import from each provided module, so that the stand-in for it exports exactly that. */
function importsOf(dir: string) {
  const found = new Map<string, Set<string>>([
    ['react', new Set(everything('react'))],
    ['react/jsx-runtime', new Set(everything('react/jsx-runtime'))],
  ])
  const statement = /import\s+(type\s+)?([\w$]+)?\s*,?\s*(?:\{([^}]*)\})?\s*from\s*['"]([^'"]+)['"]/g
  for (const file of files(dir)) {
    for (const [, typeOnly, standalone, braces, from] of fs.readFileSync(file, 'utf8').matchAll(statement)) {
      if (typeOnly || !provided(from)) continue
      const names = found.get(from) ?? new Set<string>()
      if (standalone) names.add('default')
      for (const part of (braces ?? '').split(',')) {
        const name = part.trim()
        if (name && !name.startsWith('type ')) names.add(name.split(/\s+as\s+/)[0])
      }
      found.set(from, names)
    }
  }
  return found
}

/** Replaces the provided imports with reads from `window.__fafnirx.shared`. */
function providedByApp(): Plugin {
  const wanted = importsOf(source)
  return {
    name: 'provided-by-the-app',
    enforce: 'pre',
    resolveId: (name) => (provided(name) ? `\0provided:${name}` : null),
    load(resolved) {
      if (!resolved.startsWith('\0provided:')) return null
      const name = resolved.slice('\0provided:'.length)
      const lines = [`const m = window.__fafnirx.shared[${JSON.stringify(name)}]`]
      for (const exported of wanted.get(name) ?? []) {
        lines.push(exported === 'default' ? 'export default m.default ?? m' : `export const ${exported} = m.${exported}`)
      }
      return lines.join('\n')
    },
  }
}

export default defineConfig({
  plugins: [providedByApp(), reactPlugin(), tailwindcss()],
  publicDir: false,
  build: {
    outDir: path.join(root, 'dist-modules', id),
    emptyOutDir: true,
    lib: { entry: path.join(source, 'index.ts'), formats: ['es'], fileName: () => 'index.js', cssFileName: 'style' },
    cssCodeSplit: false,
  },
})
