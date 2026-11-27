// Copies the built WebAssembly module into the page's static files; a script so the build runs the same in every shell.
import { copyFileSync, mkdirSync, statSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const here = dirname(fileURLToPath(import.meta.url))
const from = resolve(here, '../../target/wasm32-unknown-unknown/release/breach_wasm.wasm')
const to = resolve(here, '../public/breach_wasm.wasm')
mkdirSync(dirname(to), { recursive: true })
copyFileSync(from, to)
console.log(`breach_wasm.wasm: ${statSync(to).size} bytes`)
