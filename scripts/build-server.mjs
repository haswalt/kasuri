import { build } from 'esbuild'
import { mkdirSync, copyFileSync, cpSync } from 'node:fs'
mkdirSync('dist/server', { recursive:true })
await build({ entryPoints:['apps/server/src/main.ts'], outfile:'dist/server/main.mjs', bundle:true, platform:'node', target:'node24', format:'esm', sourcemap:true, banner:{ js:"import { createRequire } from 'node:module'; const require = createRequire(import.meta.url);" } })
copyFileSync('apps/api/generated/engine_bg.wasm', 'dist/server/engine_bg.wasm')

cpSync('apps/web/dist', 'dist/public', { recursive:true })
