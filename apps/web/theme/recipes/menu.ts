import { defineSlotRecipe } from '@pandacss/dev'

export const menu = defineSlotRecipe({
  className: 'menu',
  slots: ['menuRoot', 'menuTrigger', 'menuPanel', 'menuCaption'],
  base: {
    menuRoot: {
      position: 'relative'
    },
    menuTrigger: {
      width: '100%',
      padding: '10px',
      borderColor: '{colors.line}',
      gap: '9px'
    },
    menuPanel: {
      position: 'absolute',
      top: 'calc(100% + 7px)',
      right: '0',
      minWidth: '240px',
      background: '{colors.surface}',
      border: '1px solid {colors.line-strong}',
      borderRadius: '10px',
      boxShadow: '{shadows.shadow-popover}',
      padding: '6px',
      zIndex: '100',
      '&.up': {
        top: 'auto',
        bottom: 'calc(100% + 7px)'
      },
      '& button': {
        display: 'flex',
        justifyContent: 'flex-start',
        alignItems: 'center',
        gap: '9px',
        width: '100%',
        border: '0',
        background: 'transparent',
        padding: '8px',
        borderRadius: '6px',
        textAlign: 'left',
        minHeight: '34px',
        fontSize: '13px',
        color: '{colors.text-2}'
      },
      '& a': {
        display: 'flex',
        justifyContent: 'flex-start',
        alignItems: 'center',
        gap: '9px',
        width: '100%',
        border: '0',
        background: 'transparent',
        padding: '8px',
        borderRadius: '6px',
        textAlign: 'left',
        minHeight: '34px',
        fontSize: '13px',
        color: '{colors.text-2}'
      },
      '& button:hover': {
        background: '{colors.surface-2}',
        textDecoration: 'none'
      },
      '& a:hover': {
        background: '{colors.surface-2}',
        textDecoration: 'none'
      },
      '& [data-tone=danger]': {
        color: '{colors.fail}'
      },
      '& small': {
        marginLeft: 'auto'
      },
      '& kbd': {
        marginLeft: 'auto'
      }
    },
    menuCaption: {
      padding: '9px 8px',
      fontSize: '10px',
      letterSpacing: '0.08em',
      color: '{colors.text-3}',
      display: 'flex',
      flexDirection: 'column',
      gap: '5px',
      '& small': {
        margin: '0',
        letterSpacing: '0'
      }
    }
  }
})
