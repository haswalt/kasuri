import { defineSlotRecipe } from '@pandacss/dev'

export const shared = defineSlotRecipe({
  className: 'shared',
  slots: [
    'link',
    'mono',
    'muted',
    'danger',
    'srOnly',
    'emptyIcon',
    'iconButton',
    'empty',
    'filterRow',
    'block',
    'textLink',
    'chip',
    'panelHeading',
    'segmented'
  ],
  base: {
    link: {
      border: '0',
      background: 'transparent',
      color: '{colors.accent}',
      padding: '0',
      minHeight: 'auto',
      textAlign: 'left'
    },
    mono: {
      fontFamily: '{fonts.font-mono}'
    },
    muted: {
      color: '{colors.text-3}'
    },
    danger: {
      color: '{colors.fail}'
    },
    srOnly: {
      position: 'absolute',
      width: '1px',
      height: '1px',
      overflow: 'hidden',
      clip: 'rect(0, 0, 0, 0)'
    },
    emptyIcon: {
      width: '48px',
      height: '48px',
      border: '1px solid {colors.line}',
      background: '{colors.surface-2}',
      color: '{colors.text-3}',
      borderRadius: '12px',
      display: 'flex',
      alignItems: 'center',
      justifyContent: 'center',
      marginBottom: '3px'
    },
    iconButton: {
      padding: '5px',
      minHeight: '28px',
      width: '28px',
      height: '28px',
      border: '0',
      background: 'transparent',
      flex: 'none',
      color: '{colors.text-3}',
      '&:hover': {
        background: '{colors.surface-2}'
      }
    },
    empty: {
      display: 'flex',
      flexDirection: 'column',
      alignItems: 'center',
      justifyContent: 'center',
      textAlign: 'center',
      padding: '48px 24px',
      gap: '12px',
      '& p': {
        maxWidth: '480px',
        fontSize: '13px',
        lineHeight: '1.7'
      },
      '& button': {
        marginTop: '6px'
      }
    },
    filterRow: {
      borderTop: '1px solid {colors.line}',
      display: 'flex',
      padding: '12px 16px',
      gap: '10px',
      '& select': {
        width: 'auto',
        maxWidth: '210px',
        height: '34px',
        fontSize: '12px'
      },
      '& .dashboard__search': {
        marginLeft: 'auto'
      }
    },
    block: {
      display: 'block',
      marginTop: '4px',
      fontSize: '11px'
    },
    textLink: {
      color: '{colors.text} !important',
      fontWeight: '500'
    },
    chip: {
      display: 'inline-flex',
      alignItems: 'center',
      fontSize: '10.5px',
      padding: '3px 6px',
      borderRadius: '4px',
      background: '{colors.surface-2}',
      color: '{colors.text-3}',
      fontWeight: '400',
      whiteSpace: 'nowrap'
    },
    panelHeading: {
      padding: '14px 18px',
      borderBottom: '1px solid {colors.line}',
      display: 'flex',
      alignItems: 'center',
      justifyContent: 'space-between',
      gap: '8px',
      '& h2': {
        fontSize: '14px'
      }
    },
    segmented: {
      display: 'inline-flex',
      alignItems: 'center',
      gap: '2px',
      border: '1px solid {colors.line}',
      borderRadius: '7px',
      padding: '2px',
      background: '{colors.surface-2}',
      width: 'fit-content',
      flex: 'none',
      '& button': {
        background: 'transparent',
        border: '1px solid transparent',
        minHeight: '26px',
        padding: '4px 10px',
        color: '{colors.text-2}',
        fontSize: '12px',
        fontWeight: '400'
      },
      "& button[aria-pressed='true']": {
        background: '{colors.surface}',
        borderColor: '{colors.line}',
        color: '{colors.text}',
        boxShadow: '{shadows.shadow}',
        fontWeight: '500'
      }
    }
  }
})
