import { spawn } from 'node:child_process'
import {
  mkdtempSync,
  rmSync,
  readFileSync,
  writeFileSync,
  cpSync
} from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { once } from 'node:events'
import { createServer } from 'node:net'
import { checkMcpData } from './mcp-data-checks.mjs'
import { checkDataWorkflows } from './data-workflow-checks.mjs'
import { checkData } from './data-checks.mjs'
import { checkPreconditions } from './precondition-checks.mjs'
import { checkAudit } from './audit-checks.mjs'
import { checkPublications } from './publication-checks.mjs'
import { checkStorage } from './storage-checks.mjs'
import { checkStorageLayouts } from './storage-layout-checks.mjs'
import { checkWorkspaces } from './workspace-checks.mjs'
import { checkMcp } from './mcp-checks.mjs'
import { checkMcpDiscovery } from './mcp-discovery-checks.mjs'
import { checkDataTransfer } from './data-transfer-checks.mjs'
import { checkExpressions } from './expression-checks.mjs'
import { checkInputBindings } from './input-binding-checks.mjs'
import { checkRunLimits } from './run-limit-checks.mjs'
import { checkItems } from './items-checks.mjs'
import { checkParallel } from './parallel-checks.mjs'
import { checkInvocations } from './invocation-checks.mjs'
import { checkParsing } from './parsing-checks.mjs'
import { checkValidation } from './validation-checks.mjs'
import { checkConditions } from './condition-checks.mjs'
import { checkWebhookResponses } from './webhook-response-checks.mjs'
import { checkWebhooks } from './webhook-checks.mjs'
import {
  checkHttp,
  checkRestrictedHttp,
  checkAnonymousSecrets
} from './http-checks.mjs'
import { checkRuntime, healthy } from './runtime-checks.mjs'
checkStorage()
const originalFetch = globalThis.fetch
globalThis.fetch = (url, options = {}) => {
  if (process.env.KASURI_TEST_TRACE) console.error('Request:', options.method || 'GET', String(url))
  return originalFetch(url, { ...options, signal: options.signal || AbortSignal.timeout(15000) })
}
const data = mkdtempSync(join(tmpdir(), 'kasuri-runtime-'))
const probe = createServer()
probe.listen(0, '127.0.0.1')
await once(probe, 'listening')
const port = probe.address().port
await new Promise((r) => probe.close(r))
cpSync('dist/worker', join(data, 'worker'), { recursive: true })
cpSync('scripts/fixtures/http-upstream.mjs', join(data, 'upstream.mjs'))
const configPath = join(data, 'test.capnp')
let config = readFileSync('dist/workerd.capnp', 'utf8')
  .replace(
    'services = [',
    `services = [(name = "upstream", worker = (modules = [(name = "upstream.mjs", esModule = embed "upstream.mjs")], compatibilityDate = "2026-11-18")),`
  )
  .replace(
    'compatibilityDate = "2026-11-18",',
    'globalOutbound = "upstream", compatibilityDate = "2026-11-18",'
  )
writeFileSync(configPath, config)
const origin = `http://127.0.0.1:${port}`
let child
let logs = ''
function start(policy = {}) {
  child = spawn(
    resolve('node_modules/workerd/bin/workerd'),
    [
      'serve',
      configPath,
      '--socket-addr',
      `http=127.0.0.1:${port}`,
      '--directory-path',
      `storage=${data}`
    ],
    {
      cwd: resolve('dist'),
      env: {
        ...process.env,
        BASIC_AUTH: 'test:runtime-check',
        PUBLIC_ORIGIN: '',
        HTTP_ALLOWED_HOSTS: '',
        HTTP_PRIVATE_HOSTS: '',
        MAX_ACTIVE_RUNS: '',
        // Run the whole suite on either layout with WORKSPACE_STORAGE=tables.
        WORKSPACE_STORAGE: process.env.WORKSPACE_STORAGE ?? '',
        HTTP_SECRETS: JSON.stringify({
          api_password: 'super-private-password',
          api_key: 'private-api-key',
          bearer: 'Bearer private-bearer',
          incoming_key: 'incoming-test-key'
        }),
        ...policy
      }
    }
  )
  child.stdout.on('data', (b) => {
    logs += b
    if (process.env.KASURI_TEST_TRACE) process.stderr.write(b)
  })
  child.stderr.on('data', (b) => {
    logs += b
    if (process.env.KASURI_TEST_TRACE) process.stderr.write(b)
  })
}
async function stop() {
  if (child && child.exitCode === null) {
    const done = once(child, 'exit')
    child.kill('SIGTERM')
    await done
  }
}
try {
  start()
  await healthy(origin)
  if (process.argv.includes('--validation-only')) {
    await checkParsing(origin, 'test:runtime-check', async () => {await stop();start();await healthy(origin)})
    await checkValidation(origin, 'test:runtime-check', async () => {await stop();start();await healthy(origin)})
  } else if (process.argv.includes('--layouts-only')) {
    await checkStorageLayouts(origin, 'test:runtime-check', async (layout) => {await stop();start({ WORKSPACE_STORAGE: layout })})
  } else if (process.argv.includes('--bindings-only')) {
    await checkInputBindings(origin, 'test:runtime-check', async () => {await stop();start();await healthy(origin)})
  } else if (process.argv.includes('--mcp-discovery-only')) {
    await checkMcpDiscovery(origin, 'test:runtime-check')
  } else if (process.argv.includes('--expressions-only')) {
    await checkExpressions(origin, 'test:runtime-check', async () => {await stop();start();await healthy(origin)})
  } else if (process.argv.includes('--items-only')) {
    await checkItems(origin, 'test:runtime-check', async () => {await stop();start();await healthy(origin)})
  } else if (process.argv.includes('--data-transfer-only')) {
    await checkDataTransfer(origin, 'test:runtime-check', async () => {await stop();start();await healthy(origin)})
  } else if (process.argv.includes('--parallel-only')) {
    await checkParallel(origin, 'test:runtime-check', async () => {await stop();start();await healthy(origin)})
  } else if (process.argv.includes('--invocations-only')) {
    await checkInvocations(origin, 'test:runtime-check', async () => {await stop();start();await healthy(origin)})
  } else if (process.argv.includes('--conditions-only')) {
    await checkConditions(origin, 'test:runtime-check', async () => {await stop();start();await healthy(origin)})
  } else if (process.argv.includes('--webhook-responses-only')) {
    await checkWebhookResponses(origin, 'test:runtime-check', async () => {await stop();start();await healthy(origin)})
  } else if (process.argv.includes('--data-workflows-only')) {
    await checkDataWorkflows(origin, 'test:runtime-check', async () => {
      await stop(); start(); await healthy(origin)
    })
  } else if (process.argv.includes('--data-only')) {
    await checkData(origin, 'test:runtime-check', async () => {
      await stop()
      start()
      await healthy(origin)
    })
  } else {
    await checkInputBindings(origin, 'test:runtime-check', async () => {await stop();start();await healthy(origin)})
    await checkMcpDiscovery(origin, 'test:runtime-check')
    await checkExpressions(origin, 'test:runtime-check', async () => {await stop();start();await healthy(origin)})
    await checkItems(origin, 'test:runtime-check', async () => {await stop();start();await healthy(origin)})
    await checkDataTransfer(origin, 'test:runtime-check', async () => {await stop();start();await healthy(origin)})
    await checkParallel(origin, 'test:runtime-check', async () => {await stop();start();await healthy(origin)})
    await checkInvocations(origin, 'test:runtime-check', async () => {await stop();start();await healthy(origin)})
    await checkParsing(origin, 'test:runtime-check', async () => {await stop();start();await healthy(origin)})
    await checkValidation(origin, 'test:runtime-check', async () => {await stop();start();await healthy(origin)})
    await checkConditions(origin, 'test:runtime-check', async () => {await stop();start();await healthy(origin)})
    await checkWebhookResponses(origin, 'test:runtime-check', async () => {await stop();start();await healthy(origin)})
    await checkDataWorkflows(origin, 'test:runtime-check', async () => {
      await stop(); start(); await healthy(origin)
    })
    await checkRuntime(origin, 'test:runtime-check', async () => {
      await stop()
      start()
    })
    await checkWorkspaces(origin, 'test:runtime-check', async () => {
      await stop()
      start()
      await healthy(origin)
    })
    await checkPublications(origin, 'test:runtime-check', async () => {
      await stop()
      start()
      await healthy(origin)
    })
    await checkPreconditions(origin, 'test:runtime-check')
    await checkAudit(origin, 'test:runtime-check')
    await checkMcpData(origin, 'test:runtime-check', async () => {
      await stop()
      start()
      await healthy(origin)
    })
    await checkData(origin, 'test:runtime-check', async () => {
      await stop()
      start()
      await healthy(origin)
    })
    await checkHttp(origin)
    await checkMcp(origin, 'test:runtime-check', async () => {
      await stop()
      start()
      await healthy(origin)
    })
    await checkWebhooks(origin, async () => {
      await stop()
      start()
      await healthy(origin)
    })
    await checkStorageLayouts(origin, 'test:runtime-check', async (layout) => {await stop();start({ WORKSPACE_STORAGE: layout })})
    await stop()
    start({
      HTTP_ALLOWED_HOSTS: 'fixture.example,private.internal',
      HTTP_PRIVATE_HOSTS: 'private.internal'
    })
    await healthy(origin)
    await checkRestrictedHttp(origin)
    await checkRunLimits(origin, 'test:runtime-check', null)
    await stop()
    start({ MAX_ACTIVE_RUNS: '2' })
    await healthy(origin)
    await checkRunLimits(origin, 'test:runtime-check', 2)
    await stop()
    start({ BASIC_AUTH: '' })
    await healthy(origin)
    await checkAnonymousSecrets(origin)
  }
} catch (e) {
  console.error(logs)
  throw e
} finally {
  await stop()
  rmSync(data, { recursive: true, force: true })
}
