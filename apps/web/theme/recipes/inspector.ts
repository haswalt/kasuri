import { defineSlotRecipe } from '@pandacss/dev'

export const inspector = defineSlotRecipe({
  className: 'inspector',
  slots: [
    'editorStatus',
    'inspector',
    'inspectorResize',
    'inspectorHeading',
    'nodeId',
    'underlineTabs',
    'inspectorScroll',
    'removeNode',
    'sectionTitle',
    'webhookSettings',
    'edgeEditor',
    'statusSpacer',
    'editorError',
    'executionLog',
    'stepDot',
    'resultFinished',
    'selectedNodeData'
  ],
  base: {
    editorStatus: {
      '& i': {
        width: '6px',
        height: '6px',
        borderRadius: '50%',
        background: 'currentColor',
        display: 'inline-block'
      },
      height: '28px',
      display: 'flex',
      alignItems: 'center',
      gap: '18px',
      padding: '0 18px',
      fontSize: '10px',
      color: '{colors.text-3}',
      borderTop: '1px solid {colors.line}',
      background: '{colors.surface}',
      flex: 'none',
      '& > span': {
        display: 'flex',
        alignItems: 'center',
        gap: '6px'
      }
    },
    inspector: {
      position: 'relative',
      borderLeft: '1px solid {colors.line}',
      background: '{colors.surface}',
      display: 'flex',
      flexDirection: 'column',
      overflow: 'hidden',
      minHeight: '0'
    },
    inspectorResize: {
      position: 'absolute', insetBlock: '0', insetInlineStart: '0', width: '8px',
      cursor: 'ew-resize', touchAction: 'none', zIndex: '1',
      _hover: { background: '{colors.line-strong}' },
      _focusVisible: { outline: '2px solid {colors.accent}', outlineOffset: '-2px' }
    },
    inspectorHeading: {
      padding: '18px 18px 14px',
      display: 'flex',
      flexDirection: 'column',
      gap: '9px',
      '& > div': {
        display: 'flex',
        alignItems: 'center',
        gap: '8px',
        fontSize: '12px'
      },
      '& strong': {
        fontWeight: '500'
      },
      '& .shared__mono': {
        fontSize: '10px'
      }
    },
    nodeId: {
      fontSize: '9px',
      textOverflow: 'ellipsis',
      whiteSpace: 'nowrap',
      overflow: 'hidden'
    },
    underlineTabs: {
      display: 'flex',
      gap: '20px',
      padding: '0 18px',
      borderBottom: '1px solid {colors.line}',
      flex: 'none',
      '& button': {
        padding: '0',
        minHeight: '38px',
        background: 'transparent',
        border: '0',
        borderBottom: '2px solid transparent',
        borderRadius: '0',
        color: '{colors.text-3}',
        fontSize: '12px',
        fontWeight: '400'
      },
      "& button[aria-pressed='true']": {
        color: '{colors.accent}',
        borderBottomColor: '{colors.accent}'
      }
    },
    inspectorScroll: {
      padding: '18px',
      overflow: 'auto',
      display: 'flex',
      flexDirection: 'column',
      gap: '18px',
      flex: '1',
      '& label': {
        fontSize: '12px'
      },
      '& input': {
        height: '34px',
        fontSize: '12px'
      },
      '& select': {
        height: '34px',
        fontSize: '12px'
      },
      '& textarea': {
        fontSize: '11.5px'
      },
      '& h3': {
        fontSize: '12px'
      },
      '& .feedback__infoBox': {
        fontSize: '10px'
      }
    },
    removeNode: {
      marginTop: '8px',
      alignSelf: 'flex-start',
      fontSize: '11px !important'
    },
    sectionTitle: {
      fontSize: '12px',
      marginBottom: '10px'
    },
    webhookSettings: {
      display: 'flex',
      flexDirection: 'column',
      gap: '12px',
      borderTop: '1px solid {colors.line}',
      paddingTop: '16px',
      '& p': {
        fontSize: '10.5px'
      },
      '& button': {
        fontSize: '11px'
      },
      '& input': {
        font: '10px {fonts.font-mono}'
      }
    },
    edgeEditor: {
      display: 'flex',
      gap: '5px',
      alignItems: 'flex-end',
      '& label': {
        minWidth: '0',
        flex: '1'
      },
      '& input': {
        fontSize: '10px',
        padding: '4px'
      },
      '& select': {
        fontSize: '10px',
        padding: '4px'
      }
    },
    statusSpacer: {
      flex: '1'
    },
    editorError: {
      display: 'flex',
      alignItems: 'center',
      gap: '8px',
      padding: '8px 18px',
      background: '{colors.fail-soft}',
      color: '{colors.fail}',
      fontSize: '12px',
      flex: 'none',
      '& button': {
        marginLeft: 'auto'
      }
    },
    executionLog: {
      overflow: 'auto',
      maxHeight: '45%',
      minHeight: '160px',
      borderBottom: '1px solid {colors.line}',
      '& > button': {
        width: '100%',
        border: '0',
        borderBottom: '1px solid {colors.line}',
        borderRadius: '0',
        padding: '12px 16px',
        display: 'flex',
        justifyContent: 'flex-start',
        gap: '9px',
        textAlign: 'left',
        background: 'transparent'
      },
      '& > button.selected': {
        background: '{colors.accent-soft}'
      },
      '& > button > div': {
        display: 'flex',
        flex: '1',
        flexDirection: 'column',
        gap: '5px'
      },
      '& strong': {
        fontSize: '13px',
        fontWeight: '500'
      },
      '& small': {
        fontSize: '10px'
      }
    },
    stepDot: {
      display: 'inline-block',
      width: '7px',
      height: '7px',
      borderRadius: '50%',
      background: '{colors.text-3}',
      flex: 'none',
      '&.succeeded': {
        background: '{colors.ok}'
      },
      '&.running': {
        background: '{colors.run}'
      },
      '&.failed': {
        background: '{colors.fail}'
      },
      '&.waiting': {
        background: '{colors.wait}'
      }
    },
    resultFinished: {
      padding: '15px',
      display: 'flex',
      alignItems: 'center',
      gap: '8px'
    },
    selectedNodeData: {
      overflow: 'auto',
      padding: '18px',
      display: 'flex',
      flexDirection: 'column',
      gap: '12px',
      '& h3': {
        fontSize: '13px'
      },
      '& pre': {
        height: '110px',
        margin: '0',
        fontSize: '10px'
      }
    }
  }
})
