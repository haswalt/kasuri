import { defineSlotRecipe } from '@pandacss/dev'

export const canvas = defineSlotRecipe({
  className: 'canvas',
  slots: [
    'canvasHelp',
    'canvasDropOverlay',
    'nodeCardCopy',
    'flowPort',
    'portLabel',
    'zoomValue',
    'graphCanvas',
    'canvasToolbar'
  ],
  base: {
    canvasHelp: {
      position: 'absolute',
      bottom: '15px',
      right: '16px',
      zIndex: '5',
      fontSize: '10px',
      color: '{colors.text-3}',
      maxWidth: '380px',
      textAlign: 'right',
      pointerEvents: 'none',
      lineHeight: '1.7',
      '& span': {
        display: 'block',
        fontSize: '9px'
      }
    },
    canvasDropOverlay: {
      position: 'absolute',
      inset: '0',
      zIndex: '50'
    },
    nodeCardCopy: {
      display: 'flex',
      flex: '1',
      flexDirection: 'column',
      gap: '4px',
      minWidth: '0',
      '& small': {
        fontSize: '9px',
        textTransform: 'uppercase',
        letterSpacing: '0.075em',
        lineHeight: '1',
        whiteSpace: 'nowrap',
        overflow: 'hidden',
        textOverflow: 'ellipsis'
      },
      '& strong': {
        fontSize: '13px',
        fontWeight: '500',
        overflow: 'hidden',
        textOverflow: 'ellipsis',
        whiteSpace: 'nowrap',
        lineHeight: '1.3'
      }
    },
    flowPort: {
      position: 'absolute',
      bottom: '0',
      width: '0',
      height: '0',
      '&:has([data-port=error])': {
        bottom: 'auto'
      }
    },
    portLabel: {
      position: 'absolute',
      top: '8px',
      left: '6px',
      font: '8px {fonts.font-mono}',
      color: '{colors.text-3}',
      display: 'none',
      whiteSpace: 'nowrap'
    },
    zoomValue: {
      display: 'flex',
      alignItems: 'center',
      justifyContent: 'center',
      font: '10px {fonts.font-mono}',
      padding: '0 8px',
      borderLeft: '1px solid {colors.line}',
      color: '{colors.text-2}',
      minWidth: '44px'
    },
    graphCanvas: {
      position: 'relative',
      minHeight: '0',
      overflow: 'hidden',
      background: '{colors.canvas}',
      '& .react-flow': {
        height: '100%',
        position: 'absolute',
        inset: '0'
      }
    },
    canvasToolbar: {
      position: 'absolute',
      top: '16px',
      left: '18px',
      right: '16px',
      display: 'flex',
      gap: '14px',
      alignItems: 'center',
      zIndex: '5',
      pointerEvents: 'none',
      '& small': {
        marginLeft: 'auto',
        fontSize: '9px',
        color: '{colors.text-3}'
      },
      '& > div': {
        display: 'flex',
        alignItems: 'center',
        gap: '5px',
        pointerEvents: 'auto'
      },
      '& button': {
        fontSize: '10px',
        minHeight: '24px',
        padding: '4px 8px',
        background: '{colors.surface}'
      },
      '& .shared__iconButton': {
        width: '24px',
        height: '24px'
      }
    }
  }
})
