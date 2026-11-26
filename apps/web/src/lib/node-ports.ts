import type { WorkflowNode } from '../../../../packages/contracts/src/index'
export function ports(type: string, config: WorkflowNode['config'] = {}): string[] {
  if (type === 'Fail Workflow') return []
  if (type === 'IF / Condition') return ['true', 'false', 'error']
  if (type === 'Switch / Router') {
    const rules = Array.isArray(config.rules) ? config.rules : []
    const named = rules.flatMap(rule => rule && typeof rule === 'object' && !Array.isArray(rule) && typeof rule.port === 'string' ? [rule.port] : [])
    return [...new Set(named), 'fallback', 'error']
  }
  return ['success', 'error']
}
