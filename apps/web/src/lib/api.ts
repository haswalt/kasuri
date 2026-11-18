import type { Json, Run, RunMode, Workflow, Workspace } from '../../../../packages/contracts/src/index'
async function request<T>(path: string, method = 'GET', body?: unknown): Promise<T> {
  const response = await fetch(`/api/${path}`, { method, credentials: 'same-origin', headers: body === undefined ? {} : { 'Content-Type': 'application/json' }, ...(body === undefined ? {} : { body: JSON.stringify(body) }) })
  if (!response.headers.get('content-type')?.includes('application/json')) throw new Error('Backend unavailable. Start the full app with pnpm dev.')
  const result = await response.json()
  if (!response.ok) throw new Error(result.error || `Request failed (${response.status}).`)
  return result as T
}
export const api = {
  workspace: () => request<Workspace>('workspace'),
  save: (workflow: Workflow) => request<Workflow>(`workflows/${workflow.id}`, 'PUT', workflow),
  workflowAction: (id: string, action: string) => request<Workflow>(`workflows/${id}/${action}`, 'POST'),
  run: (workflowId: string, mode: RunMode, input: Json) => request<Run>('runs', 'POST', { workflowId, mode, input }),
  getRun: (id: string) => request<Run>(`runs/${id}`),
  runAction: (id: string, action: string) => request<Run>(`runs/${id}/${action}`, 'POST'),
}
