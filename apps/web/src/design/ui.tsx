import { button, statusPill, categoryTile } from '../../styled-system/recipes'
import { cx } from '../../styled-system/css'
import type { ComponentPropsWithRef } from 'react'
import { styles } from './styles'
import { useEffect, useRef, useState } from 'react'
import type { ReactNode, KeyboardEventHandler } from 'react'
import { category, nodeIcon, tone } from './ui-model'
const paths: Record<string, string[]> = {
  diamond: ['M12 3 21 12 12 21 3 12Z'],
  runs: ['M6 18 18 6', 'M6 6h12v12'],
  grid: ['M3 3h7v7H3zM14 3h7v7h-7zM3 14h7v7H3zM14 14h7v7h-7z'],
  key: ['M15 7a5 5 0 1 0 0 10 5 5 0 0 0 0-10Z', 'M10 12H3v5h3v-3h4'],
  plug: ['M8 3v5m8-5v5', 'M5 8h14v3a7 7 0 0 1-14 0ZM12 18v4'],
  file: ['M14 2H5v20h14V7Z', 'M14 2v6h5M8 12h8M8 16h6'],
  sliders: ['M4 6h16M4 12h16M4 18h16', 'M8 3v6m8 0v6M10 15v6'],
  search: ['M21 21l-5-5', 'M10.5 3a7.5 7.5 0 1 0 0 15 7.5 7.5 0 0 0 0-15Z'],
  plus: ['M12 5v14M5 12h14'],
  down: ['m8 9 4 4 4-4'],
  chevrons: ['m8 8 4-4 4 4m-8 8 4 4 4-4'],
  arrow: ['M19 12H5m6-6-6 6 6 6'],
  close: ['m6 6 12 12M6 18 18 6'],
  check: ['m5 12 4 4L19 6'],
  more: ['M5 12h.01M12 12h.01M19 12h.01'],
  play: ['m8 4 12 8-12 8Z'],
  pause: ['M8 5v14M16 5v14'],
  step: ['m5 5 10 7-10 7ZM19 5v14'],
  reset: ['M3 11a9 9 0 1 1 2 7', 'M3 4v7h7'],
  moon: ['M20 14a8 8 0 0 1-10-10A9 9 0 1 0 20 14Z'],
  monitor: ['M3 3h18v14H3zM8 21h8m-4-4v4'],
  download: ['M12 3v12m-5-5 5 5 5-5', 'M4 16v5h16v-5'],
  upload: ['M12 16V4m-5 5 5-5 5 5', 'M4 16v5h16v-5'],
  clock: ['M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18Z', 'M12 7v5l3 2'],
  bolt: ['m13 2-9 12h7l-1 8L21 9h-8Z'],
  branch: ['M6 3v12a4 4 0 0 0 4 4h7', 'M6 7h8a4 4 0 0 0 4-4M15 16l3 3-3 3'],
  command: [
    'M9 7V5a2 2 0 1 0-2 2h10a2 2 0 1 0-2-2v14a2 2 0 1 0 2-2H7a2 2 0 1 0 2 2Z'
  ],
  database: [
    'M3 6c0-5 18-5 18 0s-18 5-18 0Zm0 0v12c0 5 18 5 18 0V6M3 12c0 5 18 5 18 0'
  ],
  hash: ['M5 9h14M4 15h14M10 3 8 21M16 3l-2 18'],
  warning: ['m12 3 10 18H2ZM12 9v4m0 4h.01'],
  spark: ['m12 3 2.5 6.5L21 12l-6.5 2.5L12 21l-2.5-6.5L3 12l6.5-2.5Z'],
  copy: ['M8 8h13v13H8zM16 8V3H3v13h5'],
  lock: ['M5 10h14v11H5zM8 10V6a4 4 0 0 1 8 0v4'],
  book: [
    'M12 5c-4-3-7-3-10-2v16c4-1 7-1 10 2 3-3 6-3 10-2V3c-3-1-6-1-10 2Zm0 0v16'
  ],
  logout: ['M10 3H3v18h7M8 12h13m-4-4 4 4-4 4'],
  pencil: ['m4 16 12-12 4 4L8 20H4ZM14 6l4 4'],
  code: ['m8 6-6 6 6 6m8-12 6 6-6 6M14 3l-4 18'],
  external: ['M14 3h7v7M10 14 21 3M10 3H3v18h18v-7'],
  refresh: [
    'M20 7a9 9 0 0 0-15-1L2 9m0-6v6h6M4 17a9 9 0 0 0 15 1l3-3m0 6v-6h-6'
  ],
  stop: ['M5 5h14v14H5z']
}
export function Icon({
  name = 'diamond',
  size = 16
}: {
  name?: string
  size?: number
}) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.7"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {(paths[name] || paths.diamond).map((d, i) => (
        <path key={i} d={d} />
      ))}
    </svg>
  )
}
export function Brand({ compact = false }: { compact?: boolean }) {
  return (
    <span className={styles.brand}>
      <img src="/brand/kasuri-mark.svg" width="28" height="28" alt="" />
      {!compact && (
        <>
          <b>kasuri</b>
          <span className={styles.alpha}>alpha</span>
        </>
      )}
    </span>
  )
}
export function Tile({ kind, size = 32 }: { kind: string; size?: number }) {
  return (
    <span
      className={categoryTile({ category: category(kind) })}
      style={{ width: size, height: size }}
    >
      <Icon name={nodeIcon(kind)} size={size / 2} />
    </span>
  )
}
export function Pill({ status, label }: { status: string; label?: string }) {
  const t = tone(status)
  const aliases: Record<string, string> = {
    success: 'succeeded',
    retry: 'retrying',
    pending: 'queued',
    skipped: 'cancelled'
  }
  const value = aliases[status] || status
  const variant =
    statusPill.variantMap.status.find((item) => item === value) || 'neutral'
  return (
    <span className={statusPill({ status: variant })}>
      <i />
      {label || t.label}
    </span>
  )
}
export function PageHeader({
  eyebrow,
  title,
  description,
  actions
}: {
  eyebrow: string
  title: string
  description: string
  actions?: ReactNode
}) {
  return (
    <div className={styles.pageHeading}>
      <div>
        <div className={styles.eyebrow}>{eyebrow}</div>
        <h1>{title}</h1>
        <p>{description}</p>
      </div>
      <div className={styles.actions}>{actions}</div>
    </div>
  )
}
export function Empty({
  icon = 'diamond',
  title,
  children,
  action
}: {
  icon?: string
  title: string
  children: ReactNode
  action?: ReactNode
}) {
  return (
    <div className={styles.empty}>
      <span className={styles.emptyIcon}>
        <Icon name={icon} size={24} />
      </span>
      <h2>{title}</h2>
      <p>{children}</p>
      {action}
    </div>
  )
}
export function Search({
  value,
  onChange,
  placeholder = 'Search…'
}: {
  value: string
  onChange: (value: string) => void
  placeholder?: string
}) {
  return (
    <label className={styles.search}>
      <Icon name="search" />
      <input
        data-search
        aria-label={placeholder}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
      />
      <kbd>/</kbd>
    </label>
  )
}
export function Menu({
  label,
  children,
  up = false,
  iconOnly = false
}: {
  label: ReactNode
  children: ReactNode
  up?: boolean
  iconOnly?: boolean
}) {
  const [open, setOpen] = useState(false)
  const [autoUp, setAutoUp] = useState(false)
  const root = useRef<HTMLDivElement>(null)
  const trigger = useRef<HTMLButtonElement>(null)
  useEffect(() => {
    if (!open) return
    const close = (e: PointerEvent) => {
      if (!root.current?.contains(e.target as Node)) setOpen(false)
    }
    const key = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        setOpen(false)
        trigger.current?.focus()
      }
    }
    document.addEventListener('pointerdown', close)
    document.addEventListener('keydown', key)
    root.current
      ?.querySelectorAll('[role=menu] button,[role=menu] a')
      .forEach((el) => {
        if (!el.getAttribute('role')) el.setAttribute('role', 'menuitem')
      })
    return () => {
      document.removeEventListener('pointerdown', close)
      document.removeEventListener('keydown', key)
    }
  }, [open])
  return (
    <div className={styles.menuRoot} ref={root}>
      <Button
        ref={trigger}
        className={iconOnly ? styles.iconButton : styles.menuTrigger}
        aria-label={iconOnly ? 'Workflow actions' : undefined}
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => {
          setAutoUp(
            (trigger.current?.getBoundingClientRect().bottom || 0) >
              innerHeight / 2
          )
          setOpen(!open)
        }}
        onKeyDown={(e) => {
          if (e.key === 'ArrowDown') {
            e.preventDefault()
            setOpen(true)
            requestAnimationFrame(() =>
              root.current
                ?.querySelector<HTMLButtonElement>('[role=menu] button')
                ?.focus()
            )
          }
        }}
      >
        {label}
      </Button>
      {open && (
        <div
          className={styles.menuPanel + ' ' + (up || autoUp ? 'up' : '')}
          role="menu"
          onClick={(e) => {
            if ((e.target as Element).closest('button:not(:disabled),a'))
              setOpen(false)
          }}
          onKeyDown={(e) => {
            if (!['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(e.key)) return
            e.preventDefault()
            const items = [
              ...e.currentTarget.querySelectorAll<HTMLElement>(
                'button:not(:disabled),a[href]'
              )
            ]
            const index = items.indexOf(document.activeElement as HTMLElement)
            items[
              e.key === 'Home'
                ? 0
                : e.key === 'End'
                  ? items.length - 1
                  : (index + (e.key === 'ArrowDown' ? 1 : items.length - 1)) %
                    items.length
            ]?.focus()
          }}
        >
          {children}
        </div>
      )}
    </div>
  )
}
export function Dialog({
  title,
  children,
  onClose,
  className = '',
  onKeyDown
}: {
  title: string
  children: ReactNode
  onClose: () => void
  className?: string
  onKeyDown?: KeyboardEventHandler<HTMLDialogElement>
}) {
  const ref = useRef<HTMLDialogElement>(null)
  useEffect(() => {
    const before = document.activeElement
    const dialog = ref.current
    dialog?.showModal()
    return () => {
      dialog?.close()
      if (before instanceof HTMLElement && before.isConnected) before.focus()
    }
  }, [])
  return (
    <dialog
      className={styles.dialog + ' ' + className}
      ref={ref}
      aria-label={title}
      aria-modal="true"
      onKeyDown={onKeyDown}
      onCancel={(e) => {
        e.preventDefault()
        onClose()
      }}
      onClick={(e) => {
        if (e.target === ref.current) onClose()
      }}
    >
      <div className={styles.dialogTitle}>
        <h2>{title}</h2>
        <Button
          className={styles.iconButton}
          aria-label="Close dialog"
          onClick={onClose}
        >
          <Icon name="close" />
        </Button>
      </div>
      {children}
    </dialog>
  )
}

/** Native button semantics with shared visual and tone variants. */
export function Button({
  visual,
  tone,
  className,
  ...props
}: ComponentPropsWithRef<'button'> & {
  visual?: 'default' | 'primary' | 'ghost'
  tone?: 'default' | 'danger'
}) {
  return (
    <button
      data-visual={visual || 'default'}
      data-tone={tone || 'default'}
      className={cx(button({ visual, tone }), className)}
      {...props}
    />
  )
}
