import { defineSlotRecipe } from '@pandacss/dev'

export const editor = defineSlotRecipe({
  className: 'editor',
  slots: [
    'editorShell',
    'editorMain',
    'editorTopbar',
    'editorTitle',
    'editorActions',
    'switchTrack',
    'simulationToolbar',
    'simulationProgress',
    'buildToolbar',
    'editorLayout',
    'brandBack'
  ],
  base: {
    editorShell: {
      height: '100vh',
      minWidth: '1280px',
      overflow: 'hidden',
      display: 'flex',
      flexDirection: 'column',
      background: '{colors.surface}',
      '& .feedback__notice': {
        bottom: '42px'
      }
    },
    editorMain: {
      display: 'flex',
      flexDirection: 'column',
      flex: '1',
      minHeight: '0'
    },
    editorTopbar: {
      height: '56px',
      display: 'flex',
      alignItems: 'center',
      borderBottom: '1px solid {colors.line}',
      background: '{colors.surface}',
      padding: '0 18px',
      gap: '16px',
      flex: 'none'
    },
    editorTitle: {
      display: 'flex',
      alignItems: 'center',
      gap: '9px',
      minWidth: '180px',
      flex: '1',
      '& input': {
        border: '0',
        padding: '0',
        height: '30px',
        maxWidth: '235px',
        fontSize: '14px',
        fontWeight: '600',
        background: 'transparent'
      },
      '& small': {
        fontSize: '10px',
        whiteSpace: 'nowrap'
      },
      '@media (max-width: 1350px)': {
        '& input': {
          maxWidth: '180px'
        },
        '& small': {
          display: 'none'
        },
        minWidth: '160px'
      }
    },
    editorActions: {
      display: 'flex',
      alignItems: 'center',
      gap: '8px',
      '& > button': {
        fontSize: '11px',
        padding: '6px 10px',
        minHeight: '30px'
      },
      '& .live-control': {
        marginInline: '8px',
        whiteSpace: 'nowrap',
        display: 'flex',
        gap: '7px',
        color: '{colors.text-2}',
        fontSize: '10.5px'
      },
      '@media (max-width: 1350px)': {
        '& .live-control': {
          display: 'none'
        }
      }
    },
    switchTrack: {
      width: '26px',
      height: '16px',
      borderRadius: '10px',
      background: '{colors.line-strong}',
      display: 'inline-flex',
      alignItems: 'center',
      padding: '2px',
      '&:after': {
        content: "''",
        width: '12px',
        height: '12px',
        background: '{colors.surface}',
        borderRadius: '50%',
        boxShadow: '{shadows.shadow}'
      },
      '&.on': {
        background: '{colors.ok}',
        justifyContent: 'flex-end'
      }
    },
    simulationToolbar: {
      height: '48px',
      display: 'flex',
      alignItems: 'center',
      gap: '9px',
      background: '{colors.accent-soft}',
      borderBottom: '1px solid {colors.line}',
      padding: '0 20px',
      flex: 'none',
      '& > div:first-child': {
        marginRight: 'auto',
        display: 'flex',
        alignItems: 'center',
        gap: '13px'
      },
      '& .dashboard__eyebrow': {
        color: '{colors.accent}'
      },
      '& p': {
        fontSize: '12px'
      },
      '& button': {
        minHeight: '28px',
        padding: '4px 10px',
        fontSize: '11.5px'
      },
      '& .shared__segmented button': {
        padding: '3px 7px',
        minHeight: '24px'
      }
    },
    simulationProgress: {
      display: 'flex',
      flexDirection: 'column',
      gap: '4px',
      minWidth: '100px',
      margin: '0 8px',
      '& small': {
        fontSize: '10px'
      },
      '& progress': {
        height: '3px',
        width: '100%',
        accentColor: '{colors.accent}'
      }
    },
    buildToolbar: {
      background: '{colors.surface}',
      height: '38px',
      paddingInline: '20px',
      gap: '20px',
      '& > span': {
        display: 'flex',
        alignItems: 'center',
        gap: '7px',
        color: '{colors.text-2}',
        fontSize: '11px'
      },
      '& p': {
        fontSize: '11px',
        flex: '1',
        color: '{colors.text-3}'
      },
      '& > button': {
        minHeight: '25px',
        padding: '3px 9px'
      }
    },
    editorLayout: {
      display: 'grid',
      gridTemplateColumns: '240px minmax(0, 1fr) clamp(280px, var(--inspector-width, 400px), max(280px, calc(100vw - 480px)))',
      flex: '1',
      minHeight: '0',
      overflow: 'hidden'
    },
    brandBack: {
      border: '0',
      background: 'transparent',
      padding: '0 16px 0 0',
      borderRight: '1px solid {colors.line}',
      borderRadius: '0',
      gap: '12px',
      color: '{colors.text-2}',
      fontSize: '12px',
      minHeight: '30px',
      '& svg': {
        width: '14px'
      }
    }
  }
})
