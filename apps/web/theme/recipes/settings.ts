import { defineSlotRecipe } from '@pandacss/dev'

export const settings = defineSlotRecipe({
  className: 'settings',
  slots: [
    'settingsLayout',
    'settingsNav',
    'settingsContent',
    'settingsFields',
    'profileSummary',
    'settingRow'
  ],
  base: {
    settingsLayout: {
      display: 'grid',
      gridTemplateColumns: '200px minmax(0, 800px)',
      gap: '32px'
    },
    settingsNav: {
      display: 'flex',
      flexDirection: 'column',
      gap: '10px',
      paddingTop: '8px',
      '& > span': {
        display: 'contents'
      },
      '& .dashboard__eyebrow': {
        padding: '8px 10px',
        marginTop: '6px',
        fontSize: '10px'
      },
      '& button': {
        background: 'transparent',
        border: '0',
        justifyContent: 'flex-start',
        color: '{colors.text-2}',
        padding: '9px 10px'
      },
      '& button.active': {
        background: '{colors.accent-soft}',
        color: '{colors.accent}'
      }
    },
    settingsContent: {
      display: 'flex',
      flexDirection: 'column',
      gap: '20px',
      '& .dashboard__pageHeading': {
        marginBottom: '4px'
      }
    },
    settingsFields: {
      padding: '22px',
      display: 'flex',
      flexDirection: 'column',
      gap: '20px',
      '& label': {
        maxWidth: '420px'
      }
    },
    profileSummary: {
      display: 'flex',
      alignItems: 'center',
      gap: '12px',
      '& .shell__avatar': {
        width: '44px',
        height: '44px'
      },
      '& p': {
        fontSize: '12px',
        marginTop: '4px'
      }
    },
    settingRow: {
      padding: '18px 22px',
      display: 'flex',
      alignItems: 'center',
      justifyContent: 'space-between',
      gap: '20px',
      borderBottom: '1px solid {colors.line}',
      '&:last-child': {
        border: '0'
      },
      '& strong': {
        fontSize: '13px',
        fontWeight: '500'
      },
      '& p': {
        fontSize: '12px',
        marginTop: '5px',
        maxWidth: '500px'
      }
    }
  }
})
