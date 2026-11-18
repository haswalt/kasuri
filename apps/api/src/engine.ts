import { initSync, start, advance, control, manifests, validate } from '../generated/engine.js'
import wasm from '../generated/engine_bg.wasm'
import { createEngine } from './engine-bridge'
initSync({ module: wasm })
export const engine = createEngine({ start, advance, control, manifests, validate })
