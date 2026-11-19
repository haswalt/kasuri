import { defineConfig } from '@pandacss/dev'
import { tokens, semanticTokens } from './theme/tokens.ts'
import { globalCss } from './theme/global.ts'
import {
  button,
  statusPill,
  categoryTile,
  workflowNode,
  connectorPort
} from './theme/recipes/primitives.ts'
import { shell } from './theme/recipes/shell.ts'
import { inspector } from './theme/recipes/inspector.ts'
import { menu } from './theme/recipes/menu.ts'
import { dashboard } from './theme/recipes/dashboard.ts'
import { feedback } from './theme/recipes/feedback.ts'
import { catalogue } from './theme/recipes/catalogue.ts'
import { settings } from './theme/recipes/settings.ts'
import { dialog } from './theme/recipes/dialog.ts'
import { editor } from './theme/recipes/editor.ts'
import { palette } from './theme/recipes/palette.ts'
import { canvas } from './theme/recipes/canvas.ts'
import { runDebugger } from './theme/recipes/debugger.ts'
import { login } from './theme/recipes/login.ts'
import { conditions } from './theme/recipes/conditions.ts'
import { data } from './theme/recipes/data.ts'
import { shared } from './theme/recipes/shared.ts'

export default defineConfig({
  presets: ['@pandacss/preset-base'],
  preflight: false,
  include: ['./src/**/*.{ts,tsx}'],
  outdir: 'styled-system',
  conditions: {
    extend: {
      dark: '[data-theme="dark"] &',
      systemDark: {
        '@media (prefers-color-scheme: dark)': {
          ':root:not([data-theme]) &': '@slot'
        }
      }
    }
  },
  globalCss,
  theme: {
    tokens,
    semanticTokens,
    keyframes: {
      'kasuri-flow': { to: { strokeDashoffset: '-20' } },
      'kasuri-pulse': { '50%': { opacity: '.35' } }
    },
    recipes: { button, statusPill, categoryTile, workflowNode, connectorPort },
    slotRecipes: {
      conditions,
      data,
      shell,
      inspector,
      menu,
      dashboard,
      feedback,
      catalogue,
      settings,
      dialog,
      editor,
      palette,
      canvas,
      runDebugger,
      login,
      shared
    }
  }
})
