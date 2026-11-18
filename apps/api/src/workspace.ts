import { DurableObject } from 'cloudflare:workers'
import { sessionTtlMs } from './config'
import { engine } from './engine'
import { WorkspaceService } from './workspace-service'
export class DemoWorkspace extends DurableObject<Env> {
  private service: WorkspaceService
  constructor(ctx: DurableObjectState, env: Env) {
    super(ctx, env)
    this.service = new WorkspaceService(ctx.storage, engine, sessionTtlMs(env.SESSION_TTL_HOURS))
  }
  api(operation: string, payloadJson = '{}') { return this.service.api(operation, payloadJson) }
  alarm() { return this.service.alarm() }
}
