import { sessionTtlMs } from './config'
import { handleRequest } from './router'
export { DemoWorkspace } from './workspace'
export default {
  fetch(request: Request, env: Env) { return handleRequest(request, { WORKSPACES:env.WORKSPACES, ASSETS:env.ASSETS, sessionTtlMs:sessionTtlMs(env.SESSION_TTL_HOURS) }) },
} satisfies ExportedHandler<Env>
