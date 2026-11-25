import { cpSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import { queuesConfig } from './fixtures/queues.mjs'

// Run the built Cloudflare Worker against disposable workerd storage in tests.
export function stageRuntimeFixture(directory, { queues = false } = {}) {
  cpSync('crates/workflow-worker/build', join(directory, 'worker'), { recursive: true })
  cpSync('apps/web/dist', join(directory, 'public'), { recursive: true })
  const config = readFileSync('scripts/fixtures/workerd.capnp', 'utf8')
  return queues ? queuesConfig(config) : config
}
