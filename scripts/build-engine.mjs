import { spawnSync } from 'node:child_process'
import { mkdirSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
const root = fileURLToPath(new URL('../', import.meta.url))
function run(command, args) {
  const result = spawnSync(command, args, { cwd: root, stdio: 'inherit' })
  if (result.error) throw result.error
  if (result.status !== 0) process.exit(result.status ?? 1)
}
run('cargo', ['build', '-p', 'workflow-wasm', '--target', 'wasm32-unknown-unknown', '--release', '--locked'])
mkdirSync(new URL('../apps/api/generated/', import.meta.url), { recursive: true })
run('wasm-bindgen', ['target/wasm32-unknown-unknown/release/workflow_wasm.wasm', '--target', 'web', '--out-dir', 'apps/api/generated', '--out-name', 'engine'])
