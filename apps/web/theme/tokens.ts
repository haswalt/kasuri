import { defineTokens, defineSemanticTokens } from '@pandacss/dev'

export const tokens = defineTokens({
  fonts: {
    'font-sans': {
      value: 'Inter, system-ui, -apple-system, "Segoe UI", sans-serif'
    },
    'font-mono': {
      value: '"JetBrains Mono", ui-monospace, SFMono-Regular, Menlo, monospace'
    }
  },
  radii: {
    'radius-xs': {
      value: '4px'
    },
    'radius-sm': {
      value: '6px'
    },
    'radius-md': {
      value: '8px'
    },
    'radius-lg': {
      value: '10px'
    },
    'radius-xl': {
      value: '12px'
    }
  },
  sizes: {
    'sidebar-w': {
      value: '240px'
    },
    'header-h': {
      value: '56px'
    },
    'statusbar-h': {
      value: '28px'
    }
  }
})

export const semanticTokens = defineSemanticTokens({
  colors: {
    bg: {
      value: {
        base: '#F7F8FA',
        _dark: '#0B0D11',
        _systemDark: '#0B0D11'
      }
    },
    surface: {
      value: {
        base: '#FFFFFF',
        _dark: '#111419',
        _systemDark: '#111419'
      }
    },
    'surface-2': {
      value: {
        base: '#F2F3F6',
        _dark: '#181C23',
        _systemDark: '#181C23'
      }
    },
    canvas: {
      value: {
        base: '#F4F5F8',
        _dark: '#0D1015',
        _systemDark: '#0D1015'
      }
    },
    dot: {
      value: {
        base: '#D3D7DF',
        _dark: '#22272F',
        _systemDark: '#22272F'
      }
    },
    line: {
      value: {
        base: '#E5E7EB',
        _dark: '#1F242C',
        _systemDark: '#1F242C'
      }
    },
    'line-strong': {
      value: {
        base: '#D0D4DC',
        _dark: '#2E343E',
        _systemDark: '#2E343E'
      }
    },
    text: {
      value: {
        base: '#181B21',
        _dark: '#E6E8EC',
        _systemDark: '#E6E8EC'
      }
    },
    'text-2': {
      value: {
        base: '#525967',
        _dark: '#A3ABB9',
        _systemDark: '#A3ABB9'
      }
    },
    'text-3': {
      value: {
        base: '#6B7280',
        _dark: '#8B93A3',
        _systemDark: '#8B93A3'
      }
    },
    accent: {
      value: {
        base: '#0B6E79',
        _dark: '#4CC3CB',
        _systemDark: '#4CC3CB'
      }
    },
    'accent-fg': {
      value: {
        base: '#FFFFFF',
        _dark: '#061A1C',
        _systemDark: '#061A1C'
      }
    },
    'accent-soft': {
      value: {
        base: '#E4F2F3',
        _dark: 'rgba(76, 195, 203, .14)',
        _systemDark: 'rgba(76, 195, 203, .14)'
      }
    },
    ok: {
      value: {
        base: '#1A7F37',
        _dark: '#3FB950',
        _systemDark: '#3FB950'
      }
    },
    'ok-soft': {
      value: {
        base: '#E9F5EC',
        _dark: 'rgba(63, 185, 80, .13)',
        _systemDark: 'rgba(63, 185, 80, .13)'
      }
    },
    run: {
      value: {
        base: '#0969DA',
        _dark: '#58A6FF',
        _systemDark: '#58A6FF'
      }
    },
    'run-soft': {
      value: {
        base: '#E4EFFC',
        _dark: 'rgba(88, 166, 255, .16)',
        _systemDark: 'rgba(88, 166, 255, .16)'
      }
    },
    warn: {
      value: {
        base: '#9A6700',
        _dark: '#D29922',
        _systemDark: '#D29922'
      }
    },
    'warn-soft': {
      value: {
        base: '#FBF2DD',
        _dark: 'rgba(210, 153, 34, .14)',
        _systemDark: 'rgba(210, 153, 34, .14)'
      }
    },
    fail: {
      value: {
        base: '#CF222E',
        _dark: '#F85149',
        _systemDark: '#F85149'
      }
    },
    'fail-soft': {
      value: {
        base: '#FCECED',
        _dark: 'rgba(248, 81, 73, .13)',
        _systemDark: 'rgba(248, 81, 73, .13)'
      }
    },
    wait: {
      value: {
        base: '#8250DF',
        _dark: '#A78BFA',
        _systemDark: '#A78BFA'
      }
    },
    'wait-soft': {
      value: {
        base: '#F3EEFD',
        _dark: 'rgba(167, 139, 250, .14)',
        _systemDark: 'rgba(167, 139, 250, .14)'
      }
    },
    skip: {
      value: {
        base: '#6E7781',
        _dark: '#768390',
        _systemDark: '#768390'
      }
    },
    'c-trigger': {
      value: {
        base: '#7C5CD6',
        _dark: '#A78BFA',
        _systemDark: '#A78BFA'
      }
    },
    'c-trigger-bg': {
      value: {
        base: '#F1EDFB',
        _dark: 'rgba(167, 139, 250, .13)',
        _systemDark: 'rgba(167, 139, 250, .13)'
      }
    },
    'c-http': {
      value: {
        base: '#2563EB',
        _dark: '#7DA6FF',
        _systemDark: '#7DA6FF'
      }
    },
    'c-http-bg': {
      value: {
        base: '#E8EFFD',
        _dark: 'rgba(125, 166, 255, .13)',
        _systemDark: 'rgba(125, 166, 255, .13)'
      }
    },
    'c-logic': {
      value: {
        base: '#475467',
        _dark: '#B4BCC8',
        _systemDark: '#B4BCC8'
      }
    },
    'c-logic-bg': {
      value: {
        base: '#EEF0F3',
        _dark: 'rgba(180, 188, 200, .12)',
        _systemDark: 'rgba(180, 188, 200, .12)'
      }
    },
    'c-cf': {
      value: {
        base: '#B4590F',
        _dark: '#FB923C',
        _systemDark: '#FB923C'
      }
    },
    'c-cf-bg': {
      value: {
        base: '#FCEFE3',
        _dark: 'rgba(251, 146, 60, .12)',
        _systemDark: 'rgba(251, 146, 60, .12)'
      }
    },
    'c-app': {
      value: {
        base: '#A8436A',
        _dark: '#F472B6',
        _systemDark: '#F472B6'
      }
    },
    'c-app-bg': {
      value: {
        base: '#F9ECF1',
        _dark: 'rgba(244, 114, 182, .12)',
        _systemDark: 'rgba(244, 114, 182, .12)'
      }
    },
    'c-error': {
      value: {
        base: '#CF222E',
        _dark: '#F85149',
        _systemDark: '#F85149'
      }
    },
    'c-error-bg': {
      value: {
        base: '#FCECED',
        _dark: 'rgba(248, 81, 73, .12)',
        _systemDark: 'rgba(248, 81, 73, .12)'
      }
    },
    backdrop: {
      value: {
        base: 'rgba(12, 16, 24, .45)'
      }
    }
  },
  shadows: {
    shadow: {
      value: {
        base: '0 1px 2px rgba(16, 24, 40, .04)',
        _dark: 'none',
        _systemDark: 'none'
      }
    },
    'shadow-popover': {
      value: {
        base: '0 12px 32px rgba(12, 16, 24, .16)'
      }
    },
    'shadow-modal': {
      value: {
        base: '0 24px 64px rgba(12, 16, 24, .28)'
      }
    }
  }
})
