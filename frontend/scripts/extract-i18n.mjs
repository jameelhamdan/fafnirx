// Collects every t('…') and msg('…') call under src/ into translations/en.json and lists the strings other
// languages lack. Usage: npm run i18n   (add --check to fail when en.json is out of date or a language is incomplete)
import { existsSync, readdirSync, readFileSync, writeFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { join } from 'node:path'

const root = fileURLToPath(new URL('..', import.meta.url))
const walk = (dir) =>
  readdirSync(dir, { withFileTypes: true }).flatMap((e) =>
    e.isDirectory() ? walk(join(dir, e.name)) : /\.(tsx?|jsx?)$/.test(e.name) ? [join(dir, e.name)] : [],
  )

const call = /\b(?:t|tr|msg)\(\s*(?:'((?:[^'\\]|\\.)*)'|"((?:[^"\\]|\\.)*)"|`((?:[^`\\$]|\\.)*)`)/g
const unescape = (s) => s.replace(/\\(['"`\\])/g, '$1').replace(/\\n/g, '\n')

const keys = new Set()
for (const file of walk(join(root, 'src'))) {
  if (file.endsWith('i18n.ts')) continue // its comments show examples
  for (const m of readFileSync(file, 'utf8').matchAll(call)) keys.add(unescape(m[1] ?? m[2] ?? m[3]))
}
const sorted = [...keys].sort((a, b) => a.localeCompare(b, 'en'))
const next = JSON.stringify(Object.fromEntries(sorted.map((k) => [k, k])), null, 2) + '\n'
const enPath = join(root, 'translations', 'en.json')
const stale = !existsSync(enPath) || readFileSync(enPath, 'utf8') !== next
if (stale) writeFileSync(enPath, next)
console.log(`${sorted.length} strings in translations/en.json${stale ? ' (updated)' : ''}`)

let missing = 0
for (const name of readdirSync(join(root, 'translations'))) {
  if (name === 'en.json' || !name.endsWith('.json')) continue
  const have = JSON.parse(readFileSync(join(root, 'translations', name), 'utf8'))
  const lacking = sorted.filter((k) => !have[k])
  const unused = Object.keys(have).filter((k) => !keys.has(k))
  missing += lacking.length
  console.log(`${name}: ${lacking.length} missing, ${unused.length} unused`)
  for (const k of lacking.slice(0, 20)) console.log('  missing:', JSON.stringify(k))
}
if (process.argv.includes('--check') && (stale || missing)) process.exit(1)
