import assert from 'node:assert/strict'
import { test } from 'node:test'
import {
  httpFieldMap, httpFieldNameError, httpHeaderValue, httpQueryValue, workspaceHeaderValue
} from '../src/lib/http-fields.ts'

test('opens legacy HTTP JSON strings without changing configured values', () => {
  const saved = '{"Authorization":{"$secret":"legacy_bearer"},"X-App":"frontend"}'
  const parsed = httpFieldMap(saved)
  assert.deepEqual(parsed, { Authorization: { $secret: 'legacy_bearer' }, 'X-App': 'frontend' })
  assert.deepEqual(httpHeaderValue(parsed!.Authorization), { source: 'installation', alias: 'legacy_bearer' })
  assert.deepEqual(httpHeaderValue(parsed!['X-App']), { source: 'text', text: 'frontend' })
  assert.equal(httpFieldMap('{"Authorization":'), null)
  assert.equal(httpFieldMap(['not a field map']), null)
  assert.deepEqual(httpFieldMap(undefined), {})
})

test('edits vault references without converting credential expressions to literals', () => {
  const saved = { source: 'workspace', scheme: 'Api-Key', alias: 'shipping.key' }
  assert.deepEqual(httpHeaderValue('Api-Key {{ secrets.shipping.key }}'), saved)
  assert.deepEqual(httpHeaderValue(workspaceHeaderValue('shipping.key', 'Api-Key')), saved)
  assert.deepEqual(workspaceHeaderValue('shipping.key', 'Api-Key'), { $secret: 'Api-Key {{ secrets.shipping.key }}' })
  assert.deepEqual(httpHeaderValue(workspaceHeaderValue('', '')), { source: 'workspace', scheme: '', alias: '' })
})

test('query rows preserve scalar types and input pointers', () => {
  const parsed = httpFieldMap('{"page":2,"active":false,"empty":null,"q":{"$input":"/search~1term"}}')!
  assert.deepEqual(httpQueryValue(parsed.page), { source: 'number', value: 2 })
  assert.deepEqual(httpQueryValue(parsed.active), { source: 'boolean', value: false })
  assert.deepEqual(httpQueryValue(parsed.empty), { source: 'null', value: null })
  assert.deepEqual(httpQueryValue(parsed.q), { source: 'input', pointer: '/search~1term' })
  assert.deepEqual(httpQueryValue({ $input: '' }), { source: 'input', pointer: '' })
})

test('rejects header injection and case-insensitive duplicates while allowing case-only rename', () => {
  const fields = { Authorization: 'synthetic-only' }
  assert.ok(httpFieldNameError('headers', fields, 'authorization'))
  assert.ok(httpFieldNameError('headers', fields, 'New\r\nInjected'))
  assert.equal(httpFieldNameError('headers', fields, 'authorization', 'Authorization'), '')
  assert.equal(httpFieldNameError('query', { Page: 1 }, 'page'), '')
})
