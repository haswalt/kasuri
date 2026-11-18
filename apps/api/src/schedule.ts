import { CronExpressionParser } from 'cron-parser'
import type { Workflow } from '../../../packages/contracts/src/index'
import { assert, HttpError } from './http'

export function scheduleFor(workflow: Workflow, after: number): { at: number; cron: string; timezone: string } | null {
  const trigger = workflow.nodes.find(n => n.type === 'Schedule')
  if (!trigger) return null
  const cron = trigger.config.cron
  const timezone = trigger.config.timezone || 'UTC'
  assert(typeof cron === 'string' && cron.trim().split(/\s+/).length === 5, 'Schedule requires a five-field cron expression (minute hour day month weekday).')
  assert(typeof timezone === 'string', 'Schedule timezone must be an IANA timezone.')
  try {
    new Intl.DateTimeFormat('en', { timeZone: timezone }).format(after)
    const at = CronExpressionParser.parse(cron, { currentDate: new Date(after), tz: timezone }).next().getTime()
    return { at, cron, timezone }
  } catch { throw new HttpError(400, 'Invalid cron expression or IANA timezone.') }
}
