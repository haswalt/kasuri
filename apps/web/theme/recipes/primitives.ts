import { defineRecipe } from '@pandacss/dev'

export const button = defineRecipe({
  staticCss: [
    { visual: ['default', 'primary', 'ghost'] },
    { tone: ['default', 'danger'] }
  ],
  className: 'button',
  base: {
    font: 'inherit',
    color: 'inherit',
    WebkitTapHighlightColor: 'transparent',
    cursor: 'pointer',
    border: '1px solid {colors.line-strong}',
    borderRadius: '7px',
    background: '{colors.surface}',
    padding: '8px 12px',
    display: 'inline-flex',
    alignItems: 'center',
    justifyContent: 'center',
    gap: '7px',
    fontSize: '13px',
    fontWeight: '500',
    minHeight: '34px',
    transition: 'background 0.15s',
    _hover: {
      background: '{colors.surface-2}'
    },
    _disabled: {
      cursor: 'not-allowed',
      opacity: '0.5'
    }
  },
  variants: {
    visual: {
      default: {},
      primary: {
        background: '{colors.accent}',
        borderColor: '{colors.accent}',
        color: '{colors.accent-fg}',
        _hover: {
          filter: 'brightness(0.95)',
          background: 'accent'
        }
      },
      ghost: {
        border: '0',
        background: 'transparent',
        color: '{colors.accent}',
        padding: '0',
        minHeight: 'auto',
        textAlign: 'left',
        _hover: { background: 'transparent' }
      }
    },
    tone: {
      default: {},
      danger: {
        color: 'fail'
      }
    }
  },
  defaultVariants: {
    visual: 'default',
    tone: 'default'
  }
})

export const statusPill = defineRecipe({
  className: 'status-pill',
  base: {
    display: 'inline-flex',
    alignItems: 'center',
    gap: '5px',
    fontSize: '11px',
    padding: '4px 7px',
    borderRadius: '5px',
    fontWeight: '500',
    whiteSpace: 'nowrap',
    lineHeight: '1.2',
    '& i': {
      width: '5px',
      height: '5px',
      background: 'currentColor',
      borderRadius: '50%'
    }
  },
  variants: {
    status: {
      neutral: {
        color: 'text-3',
        background: 'surface-2'
      },
      queued: {
        color: 'skip',
        background: 'surface-2'
      },
      running: {
        color: 'run',
        background: 'run-soft',
        '& i': {
          animation: 'kasuri-pulse 1.4s ease-in-out infinite'
        }
      },
      waiting: {
        color: 'wait',
        background: 'wait-soft'
      },
      retrying: {
        color: 'warn',
        background: 'warn-soft'
      },
      succeeded: {
        color: 'ok',
        background: 'ok-soft'
      },
      failed: {
        color: 'fail',
        background: 'fail-soft'
      },
      cancelled: {
        color: 'text-2',
        background: 'surface-2'
      },
      active: {
        color: 'ok',
        background: 'ok-soft'
      },
      paused: {
        color: 'warn',
        background: 'warn-soft'
      },
      inactive: {
        color: 'text-2',
        background: 'surface-2'
      },
      draft: {
        color: 'text-2',
        background: 'surface-2'
      }
    }
  },
  defaultVariants: {
    status: 'neutral'
  },
  staticCss: [
    {
      status: ['*']
    }
  ]
})

export const categoryTile = defineRecipe({
  className: 'category-tile',
  base: {
    display: 'inline-flex',
    alignItems: 'center',
    justifyContent: 'center',
    borderRadius: '7px',
    flex: 'none'
  },
  variants: {
    category: {
      trigger: {
        color: 'c-trigger',
        background: 'c-trigger-bg'
      },
      http: {
        color: 'c-http',
        background: 'c-http-bg'
      },
      logic: {
        color: 'c-logic',
        background: 'c-logic-bg'
      },
      cf: {
        color: 'c-cf',
        background: 'c-cf-bg'
      },
      app: {
        color: 'c-app',
        background: 'c-app-bg'
      },
      error: {
        color: 'c-error',
        background: 'c-error-bg'
      }
    }
  },
  staticCss: [
    {
      category: ['*']
    }
  ]
})

export const workflowNode = defineRecipe({
  className: 'workflow-node',
  base: {
    height: '56px',
    width: '200px',
    display: 'flex',
    alignItems: 'center',
    gap: '10px',
    padding: '10px 12px',
    background: '{colors.surface}',
    border: '1px solid {colors.line-strong}',
    borderRadius: '8px',
    boxShadow: '{shadows.shadow}',
    position: 'relative'
  },
  variants: {
    state: {
      idle: {},
      running: {
        borderColor: '{colors.run}',
        boxShadow: '0 0 0 3px {colors.run-soft}',
        '& small': {
          color: '{colors.run}'
        }
      },
      succeeded: {
        borderColor: '{colors.ok}'
      },
      failed: {
        borderColor: '{colors.fail}'
      },
      skipped: {
        borderStyle: 'dashed',
        opacity: '0.45'
      },
      waiting: {
        borderColor: '{colors.wait}'
      },
      retry: {
        borderColor: '{colors.warn}'
      }
    },
    selected: {
      true: {
        borderColor: '{colors.accent}',
        boxShadow: '0 0 0 3px {colors.accent-soft}',
        '& .canvas__portLabel': {
          display: 'block'
        }
      },
      false: {}
    }
  },
  defaultVariants: {
    state: 'idle',
    selected: false
  },
  staticCss: [
    {
      state: ['*']
    },
    {
      selected: ['true', 'false']
    }
  ]
})

export const connectorPort = defineRecipe({
  className: 'connector-port',
  variants: { error: { true: { color: 'fail' }, false: {} } },
  staticCss: [{ error: ['true', 'false'] }]
})
