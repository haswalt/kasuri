import type { Json } from '../../../../packages/contracts/src/index'
import { decodeJson } from './input-hints.ts'

export type HttpFieldKind = 'headers' | 'query'
export const httpToken = /^[!#$%&'*+.^_`|~a-z0-9-]{1,128}$/i

export function httpFieldMap(value: Json | undefined): Record<string, Json> | null {
  if (value === undefined || value === '') return {}
  const parsed = decodeJson(value)
  return parsed && typeof parsed === 'object' && !Array.isArray(parsed) ? parsed : null
}

export function httpFieldNameError(kind: HttpFieldKind, fields: Record<string, Json>, name: string, previous?: string) {
  if (!name.trim()) return 'Enter a name.'
  if (kind === 'headers' && !httpToken.test(name)) return 'Enter a valid HTTP header name.'
  const identity = (key: string) => kind === 'headers' ? key.toLowerCase() : key
  if (Object.keys(fields).some(key => key !== previous && identity(key) === identity(name))) {
    return 'This name is already in the list.'
  }
  return ''
}

function reference(value: Json, key: string): string | undefined {
  if (!value || typeof value !== 'object' || Array.isArray(value) || Object.keys(value).length !== 1) return
  const entry = value[key]
  return typeof entry === 'string' ? entry : undefined
}

export function httpHeaderValue(value: Json) {
  const alias = reference(value, '$secret')
  const text = alias ?? (typeof value === 'string' ? value : '')
  const expression = /^(?:(\S+) )?\{\{\s*secrets\.([A-Za-z0-9_.-]*)\s*\}\}$/.exec(text)
  if (expression && (!expression[1] || httpToken.test(expression[1]))) {
    return { source: 'workspace', alias: expression[2], scheme: expression[1] || '' } as const
  }
  if (alias !== undefined) return { source: 'installation', alias } as const
  if (typeof value === 'string') return { source: 'text', text: value } as const
  return { source: 'unsupported' } as const
}

export function workspaceHeaderValue(alias: string, scheme: string): Json {
  return { $secret: `${scheme ? `${scheme} ` : ''}{{ secrets.${alias} }}` }
}

export function httpQueryValue(value: Json) {
  const pointer = reference(value, '$input')
  if (pointer !== undefined) return { source: 'input', pointer } as const
  if (value === null) return { source: 'null', value } as const
  if (typeof value === 'string') return { source: 'text', value } as const
  if (typeof value === 'number') return { source: 'number', value } as const
  if (typeof value === 'boolean') return { source: 'boolean', value } as const
  return { source: 'unsupported' } as const
}
