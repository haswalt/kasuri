import { assert, HttpError, json, readJson } from './http'
import type { Json, RunMode } from '../../../packages/contracts/src/index'
const cookieName = 'kasuri_demo'
async function invoke(stub: WorkspaceStub, operation: string, payload: unknown = {}) {
  const result: { status: number; value?: unknown; error?: string } = JSON.parse(await stub.api(operation, JSON.stringify(payload)))
  if (result.status !== 200) throw new HttpError(result.status, result.error ?? 'Workspace request failed.')
  return result.value
}
export interface WorkspaceStub { api(operation: string, payloadJson: string): Promise<string> }
export interface AppHost {
  WORKSPACES: { getByName(id: string): WorkspaceStub }
  ASSETS: { fetch(request: Request): Promise<Response> }
  sessionTtlMs?: number
}
export async function handleRequest(request: Request, env: AppHost): Promise<Response> {
    const url = new URL(request.url)
    if (!url.pathname.startsWith('/api/')) return env.ASSETS.fetch(request)
    try {
      if (url.pathname === '/api/health' && request.method === 'GET') return json({ ok: true, engine: 'rust-wasm', version: 1 })
      if (!['GET', 'HEAD'].includes(request.method)) {
        assert(!request.headers.get('origin') || request.headers.get('origin') === url.origin, 'Cross-origin requests are not allowed.', 403)
        assert(request.headers.get('sec-fetch-site') !== 'cross-site', 'Cross-site requests are not allowed.', 403)
      }
      const match = request.headers.get('cookie')?.match(/(?:^|;\s*)kasuri_demo=([a-f0-9-]{36})(?:;|$)/)
      let session = match?.[1]; let fresh = !session
      if (!session) { assert(request.method === 'GET' && url.pathname === '/api/workspace', 'Open the workspace to start a demo session.', 401); session = crypto.randomUUID() }
      let stub = env.WORKSPACES.getByName(session)
      let response: Response
      if (request.method === 'GET' && url.pathname === '/api/workspace') {
        try { response = json(await invoke(stub, 'bootstrap')) }
        catch (error) { if (!(error instanceof Error) || !error.message.includes('expired')) throw error; session = crypto.randomUUID(); fresh = true; stub = env.WORKSPACES.getByName(session); response = json(await invoke(stub, 'bootstrap')) }
      } else {
        const parts = url.pathname.split('/').filter(Boolean)
        if (url.pathname === '/api/runs' && request.method === 'POST') {
          const body = await readJson(request) as { workflowId?: string; mode?: RunMode; input?: Json }
          assert(body && typeof body.workflowId === 'string' && /^[A-Za-z0-9_-]{1,160}$/.test(body.workflowId), 'A valid workflowId is required.')
          assert(body.input !== undefined, 'Run input is required.')
          return json(await invoke(stub, 'createRun', { ...body, mode: body.mode ?? 'run' }), 201)
        }
        const id = parts[2]
        assert(id && /^[A-Za-z0-9_-]{1,256}$/.test(id), 'Invalid resource ID.')
        if (parts[1] === 'workflows' && parts.length === 3 && request.method === 'PUT') response = json(await invoke(stub, 'saveWorkflow', { id, workflow: await readJson(request) }))
        else if (parts[1] === 'workflows' && parts.length === 4 && request.method === 'POST') response = json(await invoke(stub, 'workflowAction', { id, action: parts[3] }))
        else if (parts[1] === 'runs' && parts.length === 3 && request.method === 'GET') response = json(await invoke(stub, 'getRun', { id }))
        else if (parts[1] === 'runs' && parts.length === 4 && request.method === 'POST') response = json(await invoke(stub, 'runAction', { id, action: parts[3] }))
        else throw new HttpError(404, 'API route not found.')
      }
      if (fresh) response.headers.set('Set-Cookie', `${cookieName}=${session}; HttpOnly; SameSite=Strict; Path=/; Max-Age=${Math.floor((env.sessionTtlMs ?? 86_400_000) / 1000)}${url.protocol === 'https:' ? '; Secure' : ''}`)
      return response
    } catch (error) {
      if (error instanceof HttpError) return json({ error: error.message }, error.status)
      return json({ error: 'The request could not be completed.' }, 500)
    }
}
