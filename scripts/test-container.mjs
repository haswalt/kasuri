import { execFileSync } from 'node:child_process'
import { randomUUID } from 'node:crypto'
import { mkdtempSync, readFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import assert from 'node:assert/strict'
import { checkMcp } from './mcp-checks.mjs'
import { checkRuntime, healthy } from './runtime-checks.mjs'
const image = process.argv[2] || 'kasuri:local'
const name = `kasuri-check-${randomUUID().slice(0, 8)}`
const docker = (...args) =>
  execFileSync('docker', args, { encoding: 'utf8' }).trim()
const secrets = mkdtempSync(join(tmpdir(), 'kasuri-secret-'))
execFileSync(process.execPath, [resolve('scripts/setup-selfhost.mjs')], {
  cwd: secrets
})
try {
  docker('volume', 'create', name)
  docker(
    'run',
    '--detach',
    '--name',
    name,
    '--publish',
    '127.0.0.1::8080',
    '--volume',
    `${name}:/data`,
    '--mount',
    `type=bind,source=${join(secrets, 'secrets/basic_auth')},target=/run/secrets/basic_auth,readonly`,
    '--env',
    'BASIC_AUTH_FILE=/run/secrets/basic_auth',
    '--read-only',
    '--tmpfs',
    '/tmp',
    '--cap-drop',
    'ALL',
    '--security-opt',
    'no-new-privileges',
    image
  )
  let origin = `http://${docker('port', name, '8080/tcp').split('\n')[0]}`
  await healthy(origin)
  assert.equal(docker('exec', name, 'id', '-u'), '1000')
  await checkRuntime(
    origin,
    readFileSync(join(secrets, 'secrets/basic_auth'), 'utf8').trim(),
    async () => {
      docker('restart', name)
      origin = `http://${docker('port', name, '8080/tcp').split('\n')[0]}`
      return origin
    }
  )
  await checkMcp(
    origin,
    readFileSync(join(secrets, 'secrets/basic_auth'), 'utf8').trim()
  )
} catch (e) {
  try {
    console.error(docker('logs', name))
  } catch {}
  throw e
} finally {
  try {
    docker('rm', '--force', name)
  } catch {}
  try {
    docker('volume', 'rm', name)
  } catch {}
  rmSync(secrets, { recursive: true, force: true })
}
