import { handleRequest } from './router'
export { DemoWorkspace } from './workspace'
export default {
  fetch(request: Request, env: Env) { return handleRequest(request, env) },
} satisfies ExportedHandler<Env>
