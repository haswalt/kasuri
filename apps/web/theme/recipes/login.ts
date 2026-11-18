import { defineSlotRecipe } from '@pandacss/dev'

export const login = defineSlotRecipe({
  className: 'login',
  slots: [
    'loginScreen',
    'loginArt',
    'loginBenefits',
    'decorativeWorkflow',
    'decorativeStep',
    'decorativeNode',
    'decorativeLine',
    'loginForm',
    'loginFooter'
  ],
  base: {
    loginScreen: {
      display: 'grid',
      gridTemplateColumns: '55.5% 44.5%',
      minHeight: '100vh'
    },
    loginArt: {
      position: 'relative',
      background: '{colors.canvas}',
      backgroundImage: 'radial-gradient({colors.dot} 1px, transparent 1px)',
      backgroundSize: '20px 20px',
      borderRight: '1px solid {colors.line}',
      padding: '40px 64px',
      display: 'flex',
      flexDirection: 'column',
      alignItems: 'flex-start',
      justifyContent: 'center',
      '& > .shell__brand': {
        position: 'absolute',
        top: '40px',
        left: '48px'
      },
      '& > .shell__brand img': {
        width: '34px',
        height: '34px'
      },
      '& h1': {
        fontSize: '34px',
        letterSpacing: '-0.035em',
        margin: '32px 0 12px'
      },
      '& p': {
        maxWidth: '400px'
      }
    },
    loginBenefits: {
      display: 'flex',
      flexDirection: 'column',
      gap: '14px',
      marginTop: '24px',
      fontSize: '13px',
      color: '{colors.text-2}',
      '& > span': {
        display: 'flex',
        alignItems: 'center',
        gap: '10px'
      },
      '& svg': {
        color: '{colors.ok}'
      }
    },
    decorativeWorkflow: {
      alignSelf: 'center',
      width: '260px',
      margin: '30px 0 5px'
    },
    decorativeStep: {
      display: 'flex',
      alignItems: 'center',
      flexDirection: 'column'
    },
    decorativeNode: {
      display: 'flex',
      width: '240px',
      gap: '10px',
      alignItems: 'center',
      background: '{colors.surface}',
      border: '1px solid {colors.line-strong}',
      padding: '11px 13px',
      borderRadius: '8px',
      boxShadow: '{shadows.shadow}',
      '& > div': {
        display: 'flex',
        flex: '1',
        flexDirection: 'column',
        gap: '4px'
      },
      '& small': {
        fontSize: '8px',
        letterSpacing: '0.08em'
      },
      '& strong': {
        fontSize: '13px',
        fontWeight: '500'
      },
      '&.decorative-0': {
        borderColor: '{colors.ok}'
      },
      '&.decorative-1': {
        borderColor: '{colors.ok}'
      },
      '&.decorative-0 > svg': {
        color: '{colors.ok}'
      },
      '&.decorative-1 > svg': {
        color: '{colors.ok}'
      },
      '&.decorative-2': {
        borderColor: '{colors.run}',
        boxShadow: '0 0 0 3px {colors.run-soft}'
      },
      '&.decorative-2 small': {
        color: '{colors.run}'
      },
      '&.decorative-2 > svg': {
        color: '{colors.run}'
      },
      '&.decorative-3': {
        borderStyle: 'dashed',
        opacity: '0.6'
      }
    },
    decorativeLine: {
      width: '1px',
      height: '26px',
      background: '{colors.line-strong}'
    },
    loginForm: {
      background: '{colors.surface}',
      display: 'flex',
      justifyContent: 'center',
      alignItems: 'center',
      padding: '40px',
      '& > div': {
        width: '380px',
        display: 'flex',
        flexDirection: 'column',
        gap: '16px'
      },
      '& h1': {
        fontSize: '27px'
      },
      '& .feedback__infoBox': {
        padding: '20px',
        marginBlock: '8px',
        display: 'flex',
        flexDirection: 'column',
        gap: '12px'
      },
      '& .feedback__infoBox p': {
        fontSize: '12px'
      },
      '& > div > button': {
        height: '46px'
      },
      '& a': {
        fontSize: '12px',
        display: 'flex',
        alignItems: 'center',
        gap: '6px'
      }
    },
    loginFooter: {
      borderTop: '1px solid {colors.line}',
      paddingTop: '20px',
      marginTop: '20px',
      display: 'flex',
      justifyContent: 'space-between',
      alignItems: 'center',
      gap: '10px',
      fontSize: '10px',
      color: '{colors.text-3}',
      '& .shell__connectionStatus': {
        fontSize: '10px'
      }
    }
  }
})
