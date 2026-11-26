import { stageRuntimeFixture } from './stage-runtime-fixture.mjs'
import { spawn } from 'node:child_process'
import {
  mkdtempSync,
  rmSync,
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
import { checkWorkspaces } from './workspace-checks.mjs'
import { checkMcp } from './mcp-checks.mjs'
import { checkMcpClient } from './mcp-client-checks.mjs'
import { checkSentry } from './sentry-checks.mjs'
import { checkPlatformAuth } from './platform-auth-checks.mjs'
import { checkMcpDiscovery } from './mcp-discovery-checks.mjs'
import { checkDataTransfer } from './data-transfer-checks.mjs'
import { checkExpressions } from './expression-checks.mjs'
import { checkInputBindings } from './input-binding-checks.mjs'
import { checkRunLimits } from './run-limit-checks.mjs'
import { checkRetention } from './retention-checks.mjs'
import { checkRunObjects } from './run-object-checks.mjs'
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
cpSync('scripts/fixtures/http-upstream.mjs', join(data, 'upstream.mjs'))
const configPath = join(data, 'test.capnp')
// RUN_EXECUTION=queues needs the config with workerd's (experimental) Queues Worker.
const queues = process.env.RUN_EXECUTION === 'queues'
// The run-object checks use queues when the suite does, otherwise run objects.
const offloaded = queues ? 'queues' : 'objects'
const config = stageRuntimeFixture(data, { queues })
  .replace('        (name = "HTTP_SECRETS",', '        (name = "SENTRY_PROJECTS", fromEnvironment = "SENTRY_PROJECTS"),\n        (name = "MCP_CONNECTIONS", fromEnvironment = "MCP_CONNECTIONS"),\n        (name = "MCP_PORTAL_CLIENT_ID", fromEnvironment = "MCP_PORTAL_CLIENT_ID"),\n        (name = "MCP_PORTAL_CLIENT_SECRET", fromEnvironment = "MCP_PORTAL_CLIENT_SECRET"),\n        (name = "HTTP_SECRETS",')
  .replace(
    'services = [',
    `services = [(name = "upstream", worker = (modules = [(name = "upstream.mjs", esModule = embed "upstream.mjs")], compatibilityDate = "2026-11-18", bindings = [(name = "TEST_ACCESS_JWKS", fromEnvironment = "TEST_ACCESS_JWKS")])),`
  )
  .replace(
    '\n      compatibilityDate = "2026-11-18",',
    '\n      globalOutbound = "upstream", compatibilityDate = "2026-11-18",'
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
      `storage=${data}`,
      ...(queues ? ['--experimental'] : [])
    ],
    {
      cwd: data,
      env: {
        ...process.env,
        PLATFORM_AUTH: 'local', PLATFORM_ACCESS_ISSUER: '', PLATFORM_ACCESS_AUDIENCE: '', PLATFORM_ACCESS_LEGACY_OWNERS: '', BASIC_AUTH: 'test:runtime-check',
        PUBLIC_ORIGIN: '',
        HTTP_ALLOWED_HOSTS: '',
        HTTP_PRIVATE_HOSTS: '',
        MAX_ACTIVE_RUNS: '',
        RUN_HISTORY: '',
        RUN_HISTORY_DAYS: '',
        FAILED_RUN_HISTORY_DAYS: '',
        SENTRY_PROJECTS: '',
        MCP_CONNECTIONS: '',
        MCP_PORTAL_CLIENT_ID: '',
        MCP_PORTAL_CLIENT_SECRET: '',
        // Run the whole suite with run objects (RUN_EXECUTION=objects) or queues.
        RUN_EXECUTION: process.env.RUN_EXECUTION ?? '',
        WORKSPACE_SECRETS_KEY: Buffer.alloc(32, 9).toString('base64url'),
        HTTP_SECRETS: JSON.stringify({
          api_password: 'super-private-password',
          api_key: 'private-api-key',
          bearer: 'Bearer private-bearer',
          incoming_key: 'incoming-test-key'
        }),
        TEST_ACCESS_JWKS: "",
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
async function restartPlatform(policy, keys) {
  await stop()
  start({ ...policy, TEST_ACCESS_JWKS: JSON.stringify(keys) })
  await healthy(origin)
}
try {
  start()
  await healthy(origin)
  if (process.argv.includes('--mcp-only')) {
    await checkMcp(origin, 'test:runtime-check', async () => { await stop(); start(); await healthy(origin) })
  } else if (process.argv.includes('--http-only')) {
    await checkHttp(origin)
  } else if (process.argv.includes('--platform-auth-only')) {
    await checkPlatformAuth(origin, restartPlatform)
  } else if (process.argv.includes('--sentry-only')) {
    await checkSentry(origin, async policy => { await stop(); start(policy); await healthy(origin) })
  } else if (process.argv.includes('--mcp-client-only')) {
    await checkMcpClient(origin, async policy => { await stop(); start(policy); await healthy(origin) })
  } else if (process.argv.includes('--validation-only')) {
    await checkParsing(origin, 'test:runtime-check', async () => {await stop();start();await healthy(origin)})
    await checkValidation(origin, 'test:runtime-check', async () => {await stop();start();await healthy(origin)})
  } else if (process.argv.includes('--run-objects-only')) {
    await stop(); start({ RUN_EXECUTION: offloaded }); await healthy(origin)
    await checkRunObjects(origin, 'test:runtime-check', data, offloaded, async () => {await stop();start({ RUN_EXECUTION: offloaded });await healthy(origin)})
  } else if (process.argv.includes('--retention-only')) {
    await checkRetention(origin, 'test:runtime-check', async (policy) => {await stop();start(policy);await healthy(origin)})
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
    await checkSentry(origin, async policy => { await stop(); start(policy); await healthy(origin) })
    await checkMcpClient(origin, async policy => { await stop(); start(policy); await healthy(origin) })
    await stop(); start(); await healthy(origin)
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
    await stop()
    start({
      HTTP_ALLOWED_HOSTS: 'fixture.example,private.internal',
      HTTP_PRIVATE_HOSTS: 'private.internal'
    })
    await healthy(origin)
    await checkRestrictedHttp(origin)
    await checkRunLimits(origin, 'test:runtime-check', null)
    await checkRetention(origin, 'test:runtime-check', async (policy) => {await stop();start(policy);await healthy(origin)})
    await stop()
    start({ RUN_EXECUTION: offloaded })
    await healthy(origin)
    await checkRunObjects(origin, 'test:runtime-check', data, offloaded, async () => {await stop();start({ RUN_EXECUTION: offloaded });await healthy(origin)})
    await stop()
    start({ MAX_ACTIVE_RUNS: '2' })
    await healthy(origin)
    await checkRunLimits(origin, 'test:runtime-check', 2)
    await stop()
    start({ BASIC_AUTH: '' })
    await healthy(origin)
    await checkAnonymousSecrets(origin)
    await restartPlatform({}, { keys: [] })
    await checkPlatformAuth(origin, restartPlatform)
  }
} catch (e) {
  console.error(logs)
  throw e
} finally {
  await stop()
  rmSync(data, { recursive: true, force: true })
}
