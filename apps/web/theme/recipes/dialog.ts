import { defineSlotRecipe } from '@pandacss/dev'

export const dialog = defineSlotRecipe({
  className: 'dialog',
  slots: ['dialog', 'dialogTitle', 'shortcutList'],
  base: {
    dialog: {
      padding: '0',
      background: '{colors.surface}',
      color: '{colors.text}',
      border: '1px solid {colors.line}',
      borderRadius: '12px',
      boxShadow: '{shadows.shadow-modal}',
      maxWidth: 'calc(100vw - 64px)',
      maxHeight: 'calc(100vh - 64px)',
      width: '540px',
      overflow: 'auto',
      '&::backdrop': {
        background: '{colors.backdrop}'
      },
      '& .dashboard__actions': {
        padding: '16px'
      },
      '& .dashboard__actions button': {
        whiteSpace: 'nowrap'
      }
    },
    dialogTitle: {
      display: 'flex',
      alignItems: 'center',
      justifyContent: 'space-between',
      padding: '18px 22px',
      borderBottom: '1px solid {colors.line}'
    },
    shortcutList: {
      padding: '20px',
      display: 'flex',
      flexDirection: 'column',
      gap: '20px',
      '& > div': {
        display: 'flex',
        gap: '18px',
        alignItems: 'center',
        fontSize: '13px'
      },
      '& kbd': {
        minWidth: '55px',
        textAlign: 'center'
      }
    }
  }
})
