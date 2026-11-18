import type { Workflow } from '../../../packages/contracts/src/index'
import branch from '../../../examples/basic-workflow.json'
export function demoWorkflows(now: number): Workflow[] {
  const base = { schemaVersion: 1 as const, status: 'draft' as const, revision: 0, updatedAt: new Date(now).toISOString() }
  return [{ ...base, id: 'core-data-demo', name: 'Core data demo', description: 'Map fields, wait, and round-trip JSON through the Rust engine.', nodes: [
    { id: 'start', type: 'Manual Trigger', label: 'Start with sample data', config: {} },
    { id: 'map', type: 'Map / Transform', label: 'Select fields', config: { mapping: '{"name":{"$input":"/name"},"source":"kasuri"}' } },
    { id: 'wait', type: 'Wait / Delay', label: 'Wait 1.5 seconds', config: { delayMs: '1500' } },
    { id: 'stringify', type: 'JSON Utilities', label: 'Encode JSON', config: { operation: 'stringify' } },
    { id: 'parse', type: 'JSON Utilities', label: 'Decode JSON', config: { operation: 'parse' } },
  ], edges: [{ id: 'e1', source: 'start', target: 'map' }, { id: 'e2', source: 'map', target: 'wait' }, { id: 'e3', source: 'wait', target: 'stringify' }, { id: 'e4', source: 'stringify', target: 'parse' }] },
  { ...branch, ...base, name: 'Conditional routing demo' } as Workflow]
}
