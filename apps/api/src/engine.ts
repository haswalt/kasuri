import { initSync, start, advance, control, manifests, validate } from '../generated/engine.js'
import wasm from '../generated/engine_bg.wasm'
import type { Json, NodeManifest, Run, RunMode, RunStatus } from '../../../packages/contracts/src/index'
// Only immutable module initialization is shared; every run is restored from its own checkpoint.
initSync({ module: wasm })
export const connectors: NodeManifest[] = JSON.parse(manifests())
export function validateGraph(workflow: unknown) { validate(JSON.stringify(workflow)) }
export function startRun(id: string, workflow: unknown, input: Json, now: number) { return start(id, JSON.stringify(workflow), JSON.stringify(input), now) }
export function advanceRun(checkpoint: string, now: number): { checkpoint: string; wakeAt: number | null } { return JSON.parse(advance(checkpoint, now)) }
export function controlRun(checkpoint: string, action: string) { return control(checkpoint, action) }
interface Checkpoint {
  id: string; status: RunStatus; failure: string | null; input: Json
  workflow: { id: string; name: string; revision: number; nodes: { id: string; label: string; type: string }[] }
  nodes: Record<string, { status: string; attempts?: number; output?: Json; failure?: { code: string }; until_ms?: number; at_ms?: number; task?: { attempt: number } }>
}
export function runView(checkpoint: string, mode: RunMode, createdAt: number, wakeAt: number | null, hostError?: string | null): Run {
  const state: Checkpoint = JSON.parse(checkpoint)
  return { id: state.id, workflowId: state.workflow.id, name: state.workflow.name, revision: state.workflow.revision, createdAt: new Date(createdAt).toISOString(), mode, status: hostError ? 'failed' : state.status, input: state.input, error: hostError ?? state.failure ?? undefined, wakeAt, nodes: state.workflow.nodes.map(n => {
    const s = state.nodes[n.id]
    return { ...n, status: s.status, attempts: s.attempts ?? s.task?.attempt ?? 0, ...(s.output !== undefined ? { output: s.output } : {}), error: s.failure?.code, untilMs: s.until_ms ?? s.at_ms }
  }) }
}
