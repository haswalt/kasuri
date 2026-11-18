import { defineSlotRecipe } from '@pandacss/dev'

export const feedback = defineSlotRecipe({
  className: 'feedback',
  slots: [
    'infoBox',
    'notice',
    'errorPanel',
    'skeleton',
    'skeletonTitle',
    'skeletonList'
  ],
  base: {
    infoBox: {
      border: '1px solid {colors.line}',
      background: '{colors.surface-2}',
      borderRadius: '7px',
      padding: '11px',
      fontSize: '11.5px',
      color: '{colors.text-2}',
      lineHeight: '1.7'
    },
    notice: {
      display: 'flex',
      alignItems: 'center',
      gap: '10px',
      padding: '10px 12px 10px 16px',
      background: '{colors.text}',
      color: '{colors.bg}',
      fontSize: '12px',
      borderBottom: '1px solid {colors.line}',
      flex: 'none',
      '& > span': {
        flex: '1'
      },
      position: 'fixed',
      bottom: '24px',
      left: '50%',
      transform: 'translateX(-50%)',
      zIndex: '120',
      border: '1px solid {colors.line-strong}',
      borderRadius: '8px',
      boxShadow: '{shadows.shadow-popover}',
      maxWidth: '700px',
      width: 'max-content',
      minWidth: '300px',
      '& .shared__iconButton': {
        color: '{colors.bg}'
      },
      '& [data-visual=ghost]': {
        color: '{colors.bg}'
      },
      '& .shared__iconButton:hover': {
        background: 'transparent'
      }
    },
    errorPanel: {
      padding: '24px',
      background: '{colors.fail-soft}',
      border: '1px solid {colors.line}',
      borderRadius: '8px',
      color: '{colors.fail}',
      display: 'flex',
      gap: '12px',
      alignItems: 'flex-start',
      flexDirection: 'column'
    },
    skeleton: {
      background: '{colors.surface-2}',
      borderRadius: '5px',
      height: '40px'
    },
    skeletonTitle: {
      width: '260px',
      height: '66px'
    },
    skeletonList: {
      padding: '20px',
      display: 'flex',
      flexDirection: 'column',
      gap: '14px'
    }
  }
})
