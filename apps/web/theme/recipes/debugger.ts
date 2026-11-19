import { defineSlotRecipe } from '@pandacss/dev'

export const runDebugger = defineSlotRecipe({
  className: 'debugger',
  slots: [
    'runModal',
    'itemResults',
    'debugger',
    'debuggerHeader',
    'debuggerMeta',
    'debuggerBody',
    'debuggerSteps',
    'stepInspector',
    'stepTimeline',
    'stepHeading',
    'stepTitle',
    'attempts',
    'ioGrid',
    'wellLabel',
    'failedWell',
    'executionInformation',
    'debuggerFooter',
    'runDetailPage'
  ],
  base: {
    itemResults: {
      maxHeight:'240px', overflow:'auto', marginBottom:'20px',
      '& h3':{fontSize:'12px',marginBottom:'10px'},
      '& table':{width:'100%',borderCollapse:'collapse',fontSize:'11px'},
      '& th, & td':{padding:'8px',textAlign:'left',borderBottom:'1px solid {colors.line}',verticalAlign:'top'},
      '& code':{whiteSpace:'pre-wrap',overflowWrap:'anywhere'},
    },
    runModal: {
      '&&': {
        width: '1180px',
        maxHeight: 'calc(100vh - 48px)',
        overflow: 'hidden'
      },
      '& .dialog__dialogTitle': {
        padding: '16px 22px'
      },
      '& .dialog__dialogTitle h2': {
        fontSize: '18px'
      },
      '@media (max-width: 1350px)': {
        '&&': {
          maxHeight: 'calc(100vh - 32px)'
        }
      }
    },
    debugger: {
      display: 'flex',
      flexDirection: 'column',
      minHeight: '0'
    },
    debuggerHeader: {
      display: 'flex',
      alignItems: 'center',
      justifyContent: 'space-between',
      padding: '14px 22px',
      gap: '16px',
      borderBottom: '1px solid {colors.line}',
      '& .dashboard__eyebrow': {
        fontSize: '9.5px',
        letterSpacing: '0.04em',
        textTransform: 'none'
      },
      '& .dashboard__actions': {
        padding: '0'
      },
      '& button': {
        fontSize: '11px',
        minHeight: '30px',
        padding: '5px 9px'
      }
    },
    debuggerMeta: {
      display: 'flex',
      alignItems: 'center',
      gap: '12px',
      marginTop: '9px',
      fontSize: '11px',
      color: '{colors.text-3}'
    },
    debuggerBody: {
      display: 'grid',
      gridTemplateColumns: '280px minmax(0, 1fr)',
      height: 'min(540px, calc(100vh - 270px))',
      minHeight: '280px'
    },
    debuggerSteps: {
      borderRight: '1px solid {colors.line}',
      overflow: 'auto',
      padding: '18px 0',
      '& > .dashboard__eyebrow': {
        padding: '0 18px 13px'
      },
      '& > button': {
        border: '0',
        borderRadius: '0',
        display: 'flex',
        width: '100%',
        gap: '9px',
        textAlign: 'left',
        justifyContent: 'flex-start',
        padding: '13px 18px',
        background: 'transparent',
        minHeight: '58px'
      },
      '& > button.selected': {
        background: '{colors.accent-soft}'
      },
      '& > button > div': {
        display: 'flex',
        flex: '1',
        flexDirection: 'column',
        gap: '5px',
        minWidth: '0'
      },
      '& strong': {
        fontSize: '12px',
        fontWeight: '500',
        whiteSpace: 'nowrap',
        overflow: 'hidden',
        textOverflow: 'ellipsis'
      },
      '& small': {
        fontSize: '9px',
        textTransform: 'uppercase'
      },
      '& .shared__mono': {
        fontSize: '10px',
        color: '{colors.text-3}'
      }
    },
    stepInspector: {
      padding: '22px',
      overflow: 'auto'
    },
    stepTimeline: {
      display: 'flex',
      gap: '4px',
      alignItems: 'center',
      height: '12px',
      marginBottom: '24px',
      '& button': {
        height: '6px',
        minHeight: '0',
        padding: '0',
        border: '0',
        flex: '1',
        borderRadius: '3px'
      },
      "& button[aria-pressed='true']": {
        outline: '1px solid {colors.accent}',
        outlineOffset: '3px',
        height: '8px'
      }
    },
    stepHeading: {
      display: 'flex',
      justifyContent: 'space-between',
      gap: '12px',
      marginBottom: '20px',
      '& p': {
        fontSize: '11px',
        marginTop: '10px'
      }
    },
    stepTitle: {
      display: 'flex',
      alignItems: 'center',
      gap: '9px',
      marginTop: '9px',
      '& h2': {
        fontSize: '16px'
      }
    },
    attempts: {
      display: 'flex',
      flexDirection: 'column',
      gap: '12px',
      textAlign: 'right',
      minWidth: '70px',
      '& strong': {
        fontSize: '15px'
      }
    },
    ioGrid: {
      display: 'grid',
      gridTemplateColumns: '1fr 1fr',
      gap: '16px',
      '& pre': {
        height: '220px',
        margin: '0 0 18px',
        fontSize: '11px'
      }
    },
    wellLabel: {
      display: 'flex',
      alignItems: 'center',
      justifyContent: 'space-between',
      marginBottom: '8px',
      '& small': {
        fontSize: '9px'
      }
    },
    failedWell: {
      borderColor: '{colors.fail} !important',
      background: '{colors.fail-soft} !important',
      color: '{colors.fail}'
    },
    executionInformation: {
      fontSize: '10px',
      height: '108px',
      margin: '0'
    },
    debuggerFooter: {
      padding: '14px 18px',
      borderTop: '1px solid {colors.line}',
      display: 'flex',
      alignItems: 'center',
      justifyContent: 'space-between',
      gap: '12px',
      '& .dashboard__actions': {
        padding: '0'
      },
      '& button': {
        fontSize: '10.5px',
        padding: '6px 9px',
        minHeight: '30px'
      },
      '& small': {
        fontSize: '10px'
      },
      '@media (max-width: 1350px)': {
        flexWrap: 'wrap'
      }
    },
    runDetailPage: {
      position: 'fixed',
      inset: '56px 0 0 240px',
      overflow: 'auto',
      zIndex: '9',
      background: '{colors.bg}',
      padding: '24px 40px',
      '& > h1': {
        margin: '15px 0 22px'
      },
      '& .debugger__debugger': {
        background: '{colors.surface}',
        border: '1px solid {colors.line}',
        borderRadius: '10px',
        overflow: 'hidden'
      },
      '& .debugger__debuggerBody': {
        height: 'calc(100vh - 340px)',
        minHeight: '400px'
      },
      '& .debugger__debuggerFooter': {
        flexWrap: 'wrap'
      }
    }
  }
})
