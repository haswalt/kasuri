export class HttpError extends Error { constructor(public status: number, message: string) { super(message) } }
export function assert(condition: unknown, message: string, status = 400): asserts condition { if (!condition) throw new HttpError(status, message) }
export async function readJson(request: Request): Promise<unknown> {
  assert(request.headers.get('content-type')?.split(';')[0] === 'application/json', 'Use application/json.', 415)
  assert(request.body, 'A JSON body is required.')
  const reader = request.body.getReader()
  const chunks: Uint8Array[] = []; let size = 0
  try {
    while (true) {
      const { done, value } = await reader.read(); if (done) break
      size += value.byteLength
      if (size > 131_072) { await reader.cancel(); throw new HttpError(413, 'Request exceeds the 128 KiB demo limit.') }
      chunks.push(value)
    }
  } finally { reader.releaseLock() }
  const bytes = new Uint8Array(size); let offset = 0
  for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.length }
  try { return JSON.parse(new TextDecoder().decode(bytes)) } catch { throw new HttpError(400, 'Invalid JSON body.') }
}
export function json(value: unknown, status = 200): Response { return Response.json(value, { status, headers: { 'Cache-Control': 'no-store', 'X-Content-Type-Options': 'nosniff' } }) }
