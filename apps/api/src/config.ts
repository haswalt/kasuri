export function sessionTtlMs(value = '24'): number {
  const hours = Number(value)
  if (!Number.isInteger(hours) || hours < 0 || hours > 8760) throw new Error('SESSION_TTL_HOURS must be 0 (persistent) or an integer from 1 to 8760.')
  return hours * 3_600_000
}
