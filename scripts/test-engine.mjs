import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { initSync, start, advance, manifests, validate } from '../apps/api/generated/engine.js'
initSync({ module: new WebAssembly.Module(readFileSync(new URL('../apps/api/generated/engine_bg.wasm', import.meta.url))) })
const graph = readFileSync(new URL('../examples/basic-workflow.json', import.meta.url), 'utf8')
validate(graph)
let checkpoint = start('wasm-test', graph, '{"priority":"urgent","title":"Wasm works"}', 0)
let result = JSON.parse(advance(checkpoint, 0))
assert.equal(result.wakeAt, 1000)
assert.equal(JSON.parse(result.checkpoint).nodes.wait.status, 'waiting')
result = JSON.parse(advance(result.checkpoint, 1000))
assert.equal(JSON.parse(result.checkpoint).status, 'succeeded')
assert.equal(JSON.parse(result.checkpoint).nodes.done.output.wait.title, 'Wasm works')
assert.equal(JSON.parse(manifests()).length, 9)
assert.throws(() => validate('{}'))
console.log('Rust/Wasm integration passed: graph validation, mapping, branching, wait, recovery and manifests.')
