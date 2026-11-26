import { useEffect, useState } from 'react'
import type { Json } from '../../../../packages/contracts/src/index'
import { styles } from '../design/styles'
import { api } from '../lib/api'

export function SentryBuilder({ config, onChange }: {
  config: Record<string, Json>
  onChange: (config: Record<string, Json>) => void
}) {
  const [projects, setProjects] = useState<string[] | null>(null)
  const [error, setError] = useState('')
  const selected = typeof config.project === 'string' ? config.project : ''
  useEffect(() => {
    let active = true
    void api.sentryProjects()
      .then(result => { if (active) setProjects(result.projects) })
      .catch(failure => { if (active) setError(failure instanceof Error ? failure.message : 'Could not load Sentry projects.') })
    return () => { active = false }
  }, [])

  return <section className={styles.conditionBuilder}>
    <p>Report a failure to Sentry. Connect a node’s error output, or connect an IF outcome and supply a message.</p>
    {error && <p role="alert">{error}</p>}
    <label>Sentry project
      <select value={selected} onChange={event => onChange({ ...config, project: event.target.value })}>
        <option value="" disabled>{!projects && !error ? 'Loading projects…' : 'Choose a project…'}</option>
        {selected && !projects?.includes(selected) && <option value={selected}>{selected} (saved alias)</option>}
        {projects?.map(project => <option value={project} key={project}>{project}</option>)}
      </select>
    </label>
    {projects?.length === 0 && <p>No Sentry projects are configured for this workspace. Imported examples can still simulate.</p>}
    <label>Message (optional)
      <input maxLength={256} placeholder="Defaults to the engine failure summary"
        value={typeof config.message === 'string' ? config.message : ''}
        onChange={event => {
          const next = { ...config }
          if (event.target.value.trim()) next.message = event.target.value
          else delete next.message
          onChange(next)
        }} />
    </label>
    <p>Reports include the failure code and workflow and run IDs. Simulations preview delivery.</p>
    <a href="https://github.com/haswalt/kasuri/blob/develop/docs/sentry.md" target="_blank" rel="noreferrer">Configure a Sentry project</a>
  </section>
}
