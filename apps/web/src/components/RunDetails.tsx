import { Button } from '../design/ui'
import { styles } from '../design/styles'
import { useEffect, useState } from 'react'
import type { Run } from '../lib/workflows'
import type { Json } from '../../../../packages/contracts/src/index'
import type { KeyboardEvent } from 'react'
import { tone, date, download } from '../design/ui-model'
import { api } from '../lib/api'
import { Dialog, Icon, Pill, Tile } from '../design/ui'
type Props = {
  run: Run
  onClose: () => void
  onViewAll: () => void
  onChanged: () => void
  onOpenPage: () => void
  fullPage?: boolean
  initialStep: number
  onStep: (index: number) => void
  onPin: (value: Json) => void
}
export function RunDetails({
  run,
  onClose,
  onViewAll,
  onChanged,
  onOpenPage,
  fullPage,
  initialStep,
  onStep,
  onPin
}: Props) {
  const [detail, setDetail] = useState(run),
    [error, setError] = useState(''),
    [pending, setPending] = useState(false)
  const attention = Math.max(
    0,
    run.nodes.findIndex((n) =>
      ['failed', 'running', 'retry', 'waiting', 'cancelled'].includes(n.status)
    )
  )
  const [cursor, setCursor] = useState(
    initialStep >= 0
      ? Math.min(initialStep, Math.max(0, run.nodes.length - 1))
      : attention
  )
  useEffect(() => {
    if (initialStep >= 0 && initialStep < detail.nodes.length)
      setCursor(initialStep)
  }, [initialStep, detail.nodes.length])
  useEffect(() => {
    let active = true
    let timer: ReturnType<typeof setTimeout> | undefined
    async function load() {
      try {
        const result = await api.getRun(run.id)
        if (!active) return
        setDetail(result)
        setError('')
        if (['running', 'paused'].includes(result.status))
          timer = setTimeout(() => void load(), 1500)
      } catch (e) {
        if (active)
          setError(e instanceof Error ? e.message : 'Could not load run.')
      }
    }
    void load()
    return () => {
      active = false
      clearTimeout(timer)
    }
  }, [run.id])
  async function action(name: string) {
    setPending(true)
    setError('')
    try {
      setDetail(await api.runAction(run.id, name))
      onChanged()
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Action failed.')
    } finally {
      setPending(false)
    }
  }
  function select(index: number) {
    setCursor(index)
    onStep(index)
  }
  const node = detail.nodes[cursor]
  const handleKey = (e: KeyboardEvent<HTMLElement>) => {
    if (e.defaultPrevented) return
    if ((e.target as HTMLElement).closest('input,textarea,select')) return
    if (e.key === 'ArrowLeft' && cursor > 0) {
      e.preventDefault()
      select(cursor - 1)
    }
    if (e.key === 'ArrowRight' && cursor < detail.nodes.length - 1) {
      e.preventDefault()
      select(cursor + 1)
    }
  }
  const content = (
    <div className={styles.debugger} onKeyDown={handleKey}>
      <header className={styles.debuggerHeader}>
        <div>
          <div className={styles.eyebrow + ' ' + styles.mono}>
            RUN · {detail.id}
          </div>
          <div className={styles.debuggerMeta}>
            <Pill status={detail.status} />
            <span>
              Started {date(detail.createdAt)} · {detail.mode} trigger ·
              Revision v{detail.revision}
            </span>
          </div>
        </div>
        <div className={styles.actions}>
          <Button onClick={() => download(detail.id + '.json', detail)}>
            <Icon name="download" />
            Export
          </Button>
          {!fullPage && (
            <Button onClick={onOpenPage}>
              Open run page <Icon name="external" />
            </Button>
          )}
        </div>
      </header>
      {error && (
        <div className={styles.errorPanel} role="alert">
          <p>{error}</p>
          <Button
            onClick={() =>
              void api
                .getRun(run.id)
                .then((value) => {
                  setDetail(value)
                  setError('')
                })
                .catch((e) => setError(String(e)))
            }
          >
            Try again
          </Button>
        </div>
      )}
      {detail.webhookResponse && (
        <details className={styles.infoBox}>
          <summary>Webhook response · {detail.webhookResponse.delivery.replaceAll('_', ' ')}</summary>
          <p>{detail.webhookResponse.delivery === 'preview' ? 'Preview only; no HTTP response was sent.' : 'Submission status is recorded separately from the workflow outcome. Submitted does not confirm client receipt.'}</p>
          {detail.webhookResponse.decision && <pre className={styles.executionInformation}>{JSON.stringify(detail.webhookResponse.decision, null, 2)}</pre>}
        </details>
      )}
      <div className={styles.debuggerBody}>
        <aside className={styles.debuggerSteps}>
          <div className={styles.eyebrow}>STEPS · {detail.nodes.length}</div>
          {detail.nodes.map((n, i) => (
            <Button
              key={n.id}
              className={cursor === i ? 'selected' : ''}
              aria-current={cursor === i ? 'step' : undefined}
              onClick={() => select(i)}
            >
              <span className={styles.mono}>
                {String(i + 1).padStart(2, '0')}
              </span>
              <span
                className={styles.stepDot}
                style={{ background: tone(n.status).fg }}
              />
              <div>
                <strong>{n.label}</strong>
                <small>{n.type}</small>
              </div>
              <small>{n.attempts}×</small>
            </Button>
          ))}
        </aside>
        <section className={styles.stepInspector}>
          <div className={styles.stepTimeline} aria-label="Run step states">
            {detail.nodes.map((n, i) => (
              <Button
                key={n.id}
                aria-label={
                  'Step ' + (i + 1) + ': ' + n.label + ', ' + n.status
                }
                aria-pressed={i === cursor}
                style={{
                  background: ['pending', 'skipped'].includes(n.status)
                    ? 'var(--line-strong)'
                    : tone(n.status).fg
                }}
                onClick={() => select(i)}
              />
            ))}
          </div>
          {node ? (
            <>
              <div className={styles.stepHeading}>
                <div>
                  <div className={styles.eyebrow}>
                    STEP {cursor + 1} OF {detail.nodes.length} · {node.type}
                  </div>
                  <div className={styles.stepTitle}>
                    <Tile kind={node.type} size={28} />
                    <h2>{node.label}</h2>
                    <Pill status={node.status} />
                  </div>
                  <p>
                    {node.error
                      ? 'Connector failed: ' + node.error
                      : node.status === 'skipped'
                        ? 'This branch was not taken.'
                        : node.status === 'pending'
                          ? 'This step has not run.'
                          : 'Recorded result from the workflow engine.'}
                  </p>
                </div>
                <div className={styles.attempts}>
                  <span className={styles.eyebrow}>ATTEMPTS</span>
                  <strong className={styles.mono}>{node.attempts}</strong>
                </div>
              </div>
              <div className={styles.ioGrid}>
                <div>
                  <div className={styles.wellLabel}>
                    <span className={styles.eyebrow}>INPUT</span>
                    <small>{cursor === 0 ? 'Run input' : 'Unavailable'}</small>
                  </div>
                  <pre>
                    {cursor === 0
                      ? JSON.stringify(detail.input, null, 2)
                      : '// Step input is not available in this run record.\n// Use the run input or inspect preceding outputs.'}
                  </pre>
                </div>
                <div>
                  <div className={styles.wellLabel}>
                    <span className={styles.eyebrow}>OUTPUT</span>
                    <small>
                      {node.output === undefined
                        ? 'No data'
                        : new Blob([JSON.stringify(node.output)]).size + ' B'}
                    </small>
                  </div>
                  <pre
                    className={
                      node.status === 'failed' ? styles.failedWell : ''
                    }
                  >
                    {node.output !== undefined
                      ? JSON.stringify(node.output, null, 2)
                      : node.error
                        ? JSON.stringify({ error: node.error }, null, 2)
                        : '// No data: this step did not produce an output.'}
                  </pre>
                </div>
              </div>
              <div className={styles.wellLabel}>
                <span className={styles.eyebrow}>EXECUTION INFORMATION</span>
                <small>Secret values are redacted by the host</small>
              </div>
              <pre className={styles.executionInformation}>
                {[
                  node.type + ' · ' + node.status,
                  'Attempts: ' + node.attempts,
                  node.untilMs
                    ? 'Wake / deadline: ' + new Date(node.untilMs).toISOString()
                    : '',
                  node.error ? 'Failure code: ' + node.error : '',
                  detail.mode === 'simulation'
                    ? 'Simulation: explicit fixtures; no external requests.'
                    : 'Real execution: configured external connectors may call services.',
                  'Detailed timings and execution logs are not available for this run.'
                ]
                  .filter(Boolean)
                  .join('\n')}
              </pre>
            </>
          ) : (
            <p>This run contains no steps.</p>
          )}
        </section>
      </div>
      <footer className={styles.debuggerFooter}>
        <div className={styles.actions}>
          <Button disabled={!cursor} onClick={() => select(cursor - 1)}>
            <Icon name="arrow" />
            Previous step
          </Button>
          <Button
            disabled={cursor >= detail.nodes.length - 1}
            onClick={() => select(cursor + 1)}
          >
            Next step <Icon name="runs" />
          </Button>
          <small>
            Step {cursor + 1} of {detail.nodes.length}
          </small>
        </div>
        <div className={styles.actions}>
          {['running', 'paused'].includes(detail.status) && (
            <>
              <Button
                disabled={pending}
                onClick={() =>
                  void action(detail.status === 'paused' ? 'resume' : 'pause')
                }
              >
                {detail.status === 'paused' ? 'Resume' : 'Pause'} run
              </Button>
              <Button
                tone="danger"
                disabled={pending}
                onClick={() => void action('cancel')}
              >
                Cancel run
              </Button>
            </>
          )}
          <Button onClick={() => onPin(detail.input)}>
            Pin run input as fixture
          </Button>
          <Button disabled title="Checkpoint replay is not available yet">
            Replay from this step
          </Button>
        </div>
      </footer>
    </div>
  )
  return fullPage ? (
    <article className={styles.runDetailPage}>
      <Button visual="ghost" onClick={onViewAll}>
        <Icon name="arrow" />
        All runs
      </Button>
      <h1>{detail.name}</h1>
      {content}
    </article>
  ) : (
    <Dialog
      onKeyDown={handleKey}
      title={detail.name}
      className={styles.runModal}
      onClose={onClose}
    >
      {content}
    </Dialog>
  )
}
