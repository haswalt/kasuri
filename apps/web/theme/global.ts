import { defineGlobalStyles } from '@pandacss/dev'

// Element defaults, theme aliases and third-party React Flow integration only.
export const globalCss = defineGlobalStyles({
  '*': {
    boxSizing: 'border-box'
  },
  body: {
    margin: '0',
    minWidth: '1280px',
    fontSize: '14px',
    WebkitFontSmoothing: 'antialiased'
  },
  input: {
    font: 'inherit',
    color: 'inherit',
    WebkitTapHighlightColor: 'transparent',
    background: '{colors.surface}',
    border: '1px solid {colors.line-strong}',
    borderRadius: '7px',
    padding: '9px 11px',
    minWidth: '0',
    width: '100%',
    outlineOffset: '2px',
    height: '36px'
  },
  textarea: {
    font: 'inherit',
    color: 'inherit',
    WebkitTapHighlightColor: 'transparent',
    background: '{colors.surface}',
    border: '1px solid {colors.line-strong}',
    borderRadius: '7px',
    padding: '9px 11px',
    minWidth: '0',
    width: '100%',
    outlineOffset: '2px',
    lineHeight: '1.6',
    resize: 'vertical'
  },
  select: {
    font: 'inherit',
    color: 'inherit',
    WebkitTapHighlightColor: 'transparent',
    background: '{colors.surface}',
    border: '1px solid {colors.line-strong}',
    borderRadius: '7px',
    padding: '9px 11px',
    minWidth: '0',
    width: '100%',
    outlineOffset: '2px',
    height: '36px'
  },
  a: {
    WebkitTapHighlightColor: 'transparent',
    color: '{colors.accent}',
    textDecoration: 'none'
  },
  'a:hover': {
    textDecoration: 'underline'
  },
  label: {
    display: 'flex',
    flexDirection: 'column',
    gap: '6px',
    fontSize: '13px',
    color: '{colors.text-2}'
  },
  h1: {
    margin: '0',
    fontSize: '28px',
    fontWeight: '600',
    letterSpacing: '-0.025em',
    lineHeight: '1.3'
  },
  h2: {
    margin: '0',
    fontSize: '16px',
    fontWeight: '600'
  },
  h3: {
    margin: '0',
    fontSize: '14px',
    fontWeight: '600'
  },
  p: {
    margin: '0',
    color: '{colors.text-2}',
    lineHeight: '1.6'
  },
  small: {
    fontSize: '12px',
    color: '{colors.text-3}'
  },
  strong: {
    fontWeight: '600'
  },
  b: {
    fontWeight: '600'
  },
  pre: {
    fontFamily: '{fonts.font-mono}',
    fontSize: '12px',
    background: '{colors.surface-2}',
    padding: '12px',
    border: '1px solid {colors.line}',
    borderRadius: '7px',
    overflow: 'auto',
    lineHeight: '1.6',
    whiteSpace: 'pre-wrap',
    overflowWrap: 'anywhere',
    color: '{colors.text-2}'
  },
  code: {
    fontFamily: '{fonts.font-mono}',
    fontSize: '12px'
  },
  kbd: {
    font: '11px {fonts.font-mono}',
    background: '{colors.surface-2}',
    border: '1px solid {colors.line}',
    padding: '1px 4px',
    borderRadius: '4px',
    color: '{colors.text-3}'
  },
  hr: {
    border: '0',
    borderTop: '1px solid {colors.line}',
    margin: '8px 0'
  },
  table: {
    borderCollapse: 'collapse',
    width: '100%',
    fontSize: '13px',
    fontVariantNumeric: 'tabular-nums'
  },
  th: {
    fontSize: '11.5px',
    color: '{colors.text-3}',
    fontWeight: '500',
    background: '{colors.surface-2}',
    textAlign: 'left',
    padding: '9px 16px',
    borderBlock: '1px solid {colors.line}',
    whiteSpace: 'nowrap'
  },
  td: {
    padding: '13px 16px',
    borderBottom: '1px solid {colors.line}',
    height: '62px'
  },
  'tbody tr:hover': {
    background: '{colors.surface-2}'
  },
  'tr:last-child td': {
    borderBottom: '0'
  },
  '.react-flow__controls': {
    boxShadow: '{shadows.shadow} !important',
    background: '{colors.surface} !important',
    border: '1px solid {colors.line} !important',
    borderRadius: '6px !important',
    overflow: 'hidden',
    bottom: '7px !important',
    left: '6px !important'
  },
  '.react-flow__controls-button': {
    background: '{colors.surface} !important',
    border: '0 !important',
    borderBottom: '1px solid {colors.line} !important',
    color: '{colors.text-2} !important',
    minHeight: '27px',
    width: '28px',
    padding: '7px !important'
  },
  '.react-flow__controls-button svg': {
    fill: 'currentColor !important'
  },
  '.react-flow__attribution': {
    display: 'none'
  },
  '.react-flow__handle': {
    minWidth: '7px !important',
    minHeight: '7px !important',
    width: '7px !important',
    height: '7px !important',
    background: '{colors.surface} !important',
    border: '1px solid {colors.line-strong} !important'
  },
  '.react-flow__handle:hover': {
    background: '{colors.accent-soft} !important',
    borderColor: '{colors.accent} !important'
  },
  '.react-flow__edge.selected .react-flow__edgeupdater': {
    fill: '{colors.surface}',
    stroke: '{colors.accent}',
    strokeWidth: '2px',
    opacity: '1'
  },
  '.react-flow__edge-text': {
    font: '10px {fonts.font-mono}',
    fill: '{colors.text-2}'
  },
  '.react-flow__edge-textbg': {
    fill: '{colors.surface}'
  },
  '.kasuri-flow': {
    '--xy-background-color': '{colors.canvas}',
    '--xy-edge-label-background-color': '{colors.surface}',
    '--xy-edge-label-color': '{colors.text-2}'
  },
  '.react-flow__controls.horizontal': {
    flexDirection: 'row !important'
  },
  ':root': {
    '--bg': 'var(--colors-bg)',
    '--surface': 'var(--colors-surface)',
    '--surface-2': 'var(--colors-surface-2)',
    '--canvas': 'var(--colors-canvas)',
    '--dot': 'var(--colors-dot)',
    '--line': 'var(--colors-line)',
    '--line-strong': 'var(--colors-line-strong)',
    '--text': 'var(--colors-text)',
    '--text-2': 'var(--colors-text-2)',
    '--text-3': 'var(--colors-text-3)',
    '--accent': 'var(--colors-accent)',
    '--accent-fg': 'var(--colors-accent-fg)',
    '--accent-soft': 'var(--colors-accent-soft)',
    '--ok': 'var(--colors-ok)',
    '--ok-soft': 'var(--colors-ok-soft)',
    '--run': 'var(--colors-run)',
    '--run-soft': 'var(--colors-run-soft)',
    '--warn': 'var(--colors-warn)',
    '--warn-soft': 'var(--colors-warn-soft)',
    '--fail': 'var(--colors-fail)',
    '--fail-soft': 'var(--colors-fail-soft)',
    '--wait': 'var(--colors-wait)',
    '--wait-soft': 'var(--colors-wait-soft)',
    '--skip': 'var(--colors-skip)',
    '--c-trigger': 'var(--colors-c-trigger)',
    '--c-trigger-bg': 'var(--colors-c-trigger-bg)',
    '--c-http': 'var(--colors-c-http)',
    '--c-http-bg': 'var(--colors-c-http-bg)',
    '--c-logic': 'var(--colors-c-logic)',
    '--c-logic-bg': 'var(--colors-c-logic-bg)',
    '--c-cf': 'var(--colors-c-cf)',
    '--c-cf-bg': 'var(--colors-c-cf-bg)',
    '--c-app': 'var(--colors-c-app)',
    '--c-app-bg': 'var(--colors-c-app-bg)',
    '--c-error': 'var(--colors-c-error)',
    '--c-error-bg': 'var(--colors-c-error-bg)',
    '--shadow': 'var(--shadows-shadow)',
    '--shadow-popover': 'var(--shadows-shadow-popover)',
    '--shadow-modal': 'var(--shadows-shadow-modal)',
    '--backdrop': 'var(--colors-backdrop)',
    '--font-sans': 'var(--fonts-font-sans)',
    '--font-mono': 'var(--fonts-font-mono)',
    '--radius-xs': 'var(--radii-radius-xs)',
    '--radius-sm': 'var(--radii-radius-sm)',
    '--radius-md': 'var(--radii-radius-md)',
    '--radius-lg': 'var(--radii-radius-lg)',
    '--radius-xl': 'var(--radii-radius-xl)',
    '--sidebar-w': 'var(--sizes-sidebar-w)',
    '--header-h': 'var(--sizes-header-h)',
    '--statusbar-h': 'var(--sizes-statusbar-h)'
  },
  'html, body': {
    background: 'bg',
    color: 'text',
    fontFamily: 'font-sans'
  },
  ':root, [data-theme="light"]': {
    colorScheme: 'light'
  },
  '[data-theme="dark"]': {
    colorScheme: 'dark'
  },
  '@media (prefers-color-scheme: dark)': {
    ':root:not([data-theme])': {
      colorScheme: 'dark'
    }
  },
  '::placeholder': {
    color: 'text-3'
  },
  ':focus-visible': {
    outline: '2px solid {colors.accent}',
    outlineOffset: '2px'
  },
  '@media (prefers-reduced-motion: reduce)': {
    '*, *::before, *::after': {
      animation: 'none !important',
      transition: 'none !important'
    }
  }
})
