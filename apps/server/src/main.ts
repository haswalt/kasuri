import { createServer } from 'node:http'
import { readFile, stat } from 'node:fs/promises'
import { readFileSync } from 'node:fs'
import { resolve, extname, sep } from 'node:path'
import { pathToFileURL } from 'node:url'
import { timingSafeEqual, createHash } from 'node:crypto'
import { initSync, start, advance, control, manifests, validate } from '../../api/generated/engine.js'
import { createEngine } from '../../api/src/engine-bridge'
import { handleRequest } from '../../api/src/router'
import { WorkspaceRegistry } from './workspaces'

initSync({ module: new WebAssembly.Module(readFileSync(new URL('./engine_bg.wasm', import.meta.url))) })
const engine = createEngine({ start, advance, control, manifests, validate })
const mime: Record<string, string> = { '.html':'text/html; charset=utf-8', '.js':'text/javascript; charset=utf-8', '.css':'text/css; charset=utf-8', '.svg':'image/svg+xml', '.png':'image/png', '.ico':'image/x-icon', '.json':'application/json' }
const digest = (value: string) => createHash('sha256').update(value).digest()
export interface ServerOptions { dataDir: string; assetsDir: string; port?: number; host?: string; publicOrigin?: string; sessionTtlHours?: number; basicAuth?: string }
export async function createStandaloneServer(options: ServerOptions) {
  if (options.basicAuth !== undefined && !/^[^:\r\n]+:[^\r\n]{12,}$/.test(options.basicAuth)) throw new Error('BASIC_AUTH_FILE must contain user:password with a password of at least 12 characters.')
  const ttl = (options.sessionTtlHours ?? 0) * 3_600_000
  if (!Number.isSafeInteger(ttl) || (ttl !== 0 && ttl < 3_600_000) || ttl > 8760 * 3_600_000) throw new Error('SESSION_TTL_HOURS must be 0 (persistent) or between 1 and 8760.')
  const publicOrigin = options.publicOrigin ? new URL(options.publicOrigin).origin : undefined
  const assets = resolve(options.assetsDir)
  const registry = new WorkspaceRegistry(resolve(options.dataDir), engine, ttl)
  const assetHandler = { async fetch(request: Request): Promise<Response> {
    const url = new URL(request.url)
    if (!['GET','HEAD'].includes(request.method)) return new Response('Method not allowed', { status:405 })
    let file: string
    try { file = resolve(assets, '.' + decodeURIComponent(url.pathname)) } catch { return new Response('Invalid path', { status:400 }) }
    if (file !== assets && !file.startsWith(assets + sep)) return new Response('Forbidden', { status:403 })
    try { if (!(await stat(file)).isFile()) throw new Error() }
    catch { if (extname(url.pathname)) return new Response('Not found', { status:404 }); file = resolve(assets, 'index.html') }
    try {
      const content = request.method === 'HEAD' ? null : await readFile(file)
      return new Response(content, { headers: { 'Content-Type':mime[extname(file)] ?? 'application/octet-stream', 'X-Content-Type-Options':'nosniff', 'Cache-Control':url.pathname.startsWith('/assets/') ? 'public, max-age=31536000, immutable' : 'no-cache' } })
    } catch { return new Response('Assets unavailable', { status:503 }) }
  } }
  const server = createServer(async (incoming, outgoing) => {
    try {
      const url = new URL(incoming.url ?? '/', publicOrigin ?? `http://${incoming.headers.host ?? 'localhost'}`)
      if (options.basicAuth && url.pathname !== '/api/health') {
        const expected = 'Basic ' + Buffer.from(options.basicAuth).toString('base64')
        if (!timingSafeEqual(digest(incoming.headers.authorization ?? ''), digest(expected))) {
          outgoing.writeHead(401, { 'WWW-Authenticate':'Basic realm="Kasuri", charset="UTF-8"', 'Cache-Control':'no-store' }); outgoing.end('Authentication required.'); return
        }
      }
      const chunks: Buffer[] = []; let size = 0
      for await (const chunk of incoming) {
        const buffer = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk)
        size += buffer.length
        if (size > 131_072) { outgoing.writeHead(413, { 'Content-Type':'application/json' }); outgoing.end('{"error":"Request exceeds 128 KiB."}'); return }
        chunks.push(buffer)
      }
      const headers = new Headers()
      for (const [key, value] of Object.entries(incoming.headers)) if (value !== undefined) headers.set(key, Array.isArray(value) ? value.join(', ') : value)
      const request = new Request(url, { method:incoming.method, headers, ...(chunks.length ? { body:Buffer.concat(chunks) } : {}) })
      const response = await handleRequest(request, { WORKSPACES:registry, ASSETS:assetHandler, sessionTtlMs:ttl })
      outgoing.writeHead(response.status, Object.fromEntries(response.headers.entries()))
      outgoing.end(Buffer.from(await response.arrayBuffer()))
    } catch { if (!outgoing.headersSent) outgoing.writeHead(500, { 'Content-Type':'application/json' }); outgoing.end('{"error":"Request failed."}') }
  })
  server.requestTimeout = 30_000; server.headersTimeout = 15_000
  try { await new Promise<void>((done, reject) => { server.once('error', reject); server.listen(options.port ?? 8080, options.host ?? '0.0.0.0', done) }) }
  catch (error) { await registry.close(); throw error }
  const address = server.address()
  if (!address || typeof address === 'string') throw new Error('Server address unavailable.')
  return {
    origin:`http://127.0.0.1:${address.port}`,
    async close() { await new Promise<void>((done, reject) => server.close(e => e ? reject(e) : done())); await registry.close() },
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const authFile = process.env.BASIC_AUTH_FILE
  const app = await createStandaloneServer({ dataDir:process.env.DATA_DIR ?? './data', assetsDir:process.env.ASSETS_DIR ?? new URL('../public/', import.meta.url).pathname, port:Number(process.env.PORT ?? 8080), host:process.env.HOST ?? '0.0.0.0', publicOrigin:process.env.PUBLIC_ORIGIN, sessionTtlHours:Number(process.env.SESSION_TTL_HOURS ?? 0), basicAuth:authFile ? readFileSync(authFile, 'utf8').trim() : undefined })
  console.log(`Kasuri listening on ${app.origin}`)
  let closing = false
  for (const signal of ['SIGTERM','SIGINT'] as const) process.on(signal, () => { if (!closing) { closing = true; void app.close().then(() => process.exit(0), () => process.exit(1)) } })
}
