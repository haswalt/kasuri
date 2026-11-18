import { cpSync, mkdirSync, copyFileSync } from 'node:fs'
mkdirSync('dist/worker',{recursive:true})
cpSync('crates/workflow-worker/build/index.js','dist/worker/index.js')
cpSync('crates/workflow-worker/build/index_bg.wasm','dist/worker/index_bg.wasm')
cpSync('apps/web/dist','dist/public',{recursive:true})
copyFileSync('deploy/workerd/workerd.capnp','dist/workerd.capnp')
mkdirSync('data',{recursive:true})
console.log('Self-host package: dist/workerd.capnp, worker Wasm and public assets. Start with pnpm start.')
