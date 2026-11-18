import type { Json, NodeManifest } from '../../../packages/contracts/src/index'
export interface EngineBindings {
  manifests(): string
  validate(workflow: string): void
  start(id: string, workflow: string, input: string, now: number): string
  advance(checkpoint: string, now: number): string
  control(checkpoint: string, action: string): string
}
export function createEngine(bindings: EngineBindings) {
  return {
    connectors: JSON.parse(bindings.manifests()) as NodeManifest[],
    validateGraph(workflow: unknown) { bindings.validate(JSON.stringify(workflow)) },
    startRun(id: string, workflow: unknown, input: Json, now: number) { return bindings.start(id, JSON.stringify(workflow), JSON.stringify(input), now) },
    advanceRun(checkpoint: string, now: number): { checkpoint: string; wakeAt: number | null } { return JSON.parse(bindings.advance(checkpoint, now)) },
    controlRun(checkpoint: string, action: string) { return bindings.control(checkpoint, action) },
  }
}
export type Engine = ReturnType<typeof createEngine>
