import { defineSlotRecipe } from '@pandacss/dev'

export const dashboard = defineSlotRecipe({
  className: 'dashboard',
  slots: [
    'pageContent',
    'pageHeading',
    'eyebrow',
    'actions',
    'panel',
    'statStrip',
    'statGrid',
    'stat',
    'stateBar',
    'stateLegend',
    'lastRun',
    'listToolbar',
    'tabs',
    'search',
    'tableScroll',
    'nameCell',
    'workflowLink',
    'workflowTile',
    'numeric',
    'nowrap',
    'triggerLabel',
    'tableFooter'
  ],
  base: {
    pageContent: {
      padding: '32px 40px',
      display: 'flex',
      flexDirection: 'column',
      gap: '24px',
      maxWidth: '1880px',
      margin: 'auto',
      '@media (max-width: 1350px)': {
        paddingInline: '28px'
      }
    },
    pageHeading: {
      display: 'flex',
      justifyContent: 'space-between',
      alignItems: 'flex-end',
      gap: '20px',
      '& h1': {
        margin: '6px 0'
      }
    },
    eyebrow: {
      fontSize: '10.5px',
      fontWeight: '500',
      letterSpacing: '0.11em',
      color: '{colors.text-3}',
      textTransform: 'uppercase',
      lineHeight: '1.5'
    },
    actions: {
      display: 'flex',
      alignItems: 'center',
      gap: '8px',
      flexShrink: '0'
    },
    panel: {
      background: '{colors.surface}',
      border: '1px solid {colors.line}',
      borderRadius: '10px',
      boxShadow: '{shadows.shadow}',
      minWidth: '0'
    },
    statStrip: {
      background: '{colors.surface}',
      border: '1px solid {colors.line}',
      borderRadius: '10px',
      boxShadow: '{shadows.shadow}',
      minWidth: '0',
      overflow: 'hidden'
    },
    statGrid: {
      display: 'grid',
      gridTemplateColumns: 'repeat(4, minmax(0, 1fr))'
    },
    stat: {
      padding: '19px 22px',
      display: 'flex',
      flexDirection: 'column',
      gap: '7px',
      borderRight: '1px solid {colors.line}',
      '&:last-child': {
        border: '0'
      },
      '& > span': {
        color: '{colors.text-2}',
        fontSize: '13px'
      },
      '& > strong': {
        fontSize: '26px',
        fontWeight: '600',
        letterSpacing: '-0.025em',
        fontVariantNumeric: 'tabular-nums'
      },
      '& > strong.small-stat': {
        fontSize: '16px',
        letterSpacing: '0',
        padding: '4px 0'
      }
    },
    stateBar: {
      margin: '0 22px',
      display: 'flex',
      gap: '2px',
      height: '6px',
      background: '{colors.surface-2}',
      borderRadius: '3px',
      overflow: 'hidden',
      '& span': {
        minWidth: '0.8%',
        borderRadius: '2px'
      }
    },
    stateLegend: {
      display: 'flex',
      alignItems: 'center',
      flexWrap: 'wrap',
      gap: '19px',
      padding: '12px 22px 16px',
      '& span': {
        display: 'flex',
        alignItems: 'center',
        gap: '6px',
        fontSize: '11px',
        color: '{colors.text-2}'
      },
      '& i': {
        width: '6px',
        height: '6px',
        borderRadius: '50%',
        display: 'inline-block'
      },
      '& b': {
        fontSize: '11px',
        color: '{colors.text}',
        fontVariantNumeric: 'tabular-nums'
      }
    },
    lastRun: {
      '& i': {
        width: '6px',
        height: '6px',
        borderRadius: '50%',
        display: 'inline-block'
      },
      display: 'flex',
      flexDirection: 'column',
      gap: '5px',
      alignItems: 'flex-start',
      color: '{colors.text-2} !important',
      fontSize: '12px !important',
      '& > span': {
        display: 'flex',
        alignItems: 'center',
        gap: '6px'
      },
      '& small': {
        fontSize: '10.5px'
      }
    },
    listToolbar: {
      display: 'flex',
      alignItems: 'center',
      justifyContent: 'space-between',
      gap: '12px',
      padding: '13px 16px'
    },
    tabs: {
      display: 'flex',
      gap: '3px',
      alignItems: 'center',
      flexWrap: 'wrap',
      '& button': {
        border: '0',
        background: 'transparent',
        color: '{colors.text-2}',
        minHeight: '30px',
        padding: '5px 10px',
        fontWeight: '400',
        gap: '7px'
      },
      "& button[aria-pressed='true']": {
        color: '{colors.accent}',
        background: '{colors.accent-soft}',
        fontWeight: '500'
      },
      '& small': {
        fontSize: '11px'
      },
      "& button[aria-pressed='true'] small": {
        color: '{colors.accent}'
      }
    },
    search: {
      display: 'flex',
      alignItems: 'center',
      flexDirection: 'row',
      gap: '8px',
      border: '1px solid {colors.line-strong}',
      borderRadius: '7px',
      padding: '0 9px',
      minWidth: '210px',
      width: '250px',
      height: '34px',
      background: '{colors.surface}',
      color: '{colors.text-3}',
      '& input': {
        border: '0',
        padding: '0',
        background: 'transparent',
        height: '30px',
        fontSize: '12px',
        outline: 'none'
      },
      '&:focus-within': {
        outline: '2px solid {colors.accent}',
        outlineOffset: '2px'
      },
      '& svg': {
        flexShrink: '0'
      }
    },
    tableScroll: {
      overflowX: 'auto'
    },
    nameCell: {
      display: 'flex',
      alignItems: 'center',
      gap: '10px',
      textAlign: 'left',
      '& > span:last-child': {
        display: 'flex',
        flexDirection: 'column',
        gap: '5px',
        minWidth: '0'
      },
      '& strong': {
        fontWeight: '500',
        color: '{colors.text}'
      }
    },
    workflowLink: {
      padding: '0',
      minHeight: '0',
      border: '0',
      background: 'transparent',
      maxWidth: '320px',
      justifyContent: 'flex-start',
      '&:hover': {
        background: 'transparent'
      },
      '& small': {
        fontSize: '11.5px',
        maxWidth: '270px',
        textOverflow: 'ellipsis',
        overflow: 'hidden',
        whiteSpace: 'nowrap'
      },
      '& .dashboard__nameCell': {
        minWidth: '0'
      },
      '& strong': {
        overflow: 'hidden',
        textOverflow: 'ellipsis',
        whiteSpace: 'nowrap',
        maxWidth: '245px'
      }
    },
    workflowTile: {
      display: 'flex',
      alignItems: 'center',
      justifyContent: 'center',
      width: '32px',
      height: '32px',
      border: '1px solid {colors.line}',
      borderRadius: '7px',
      background: '{colors.surface-2}',
      color: '{colors.text-2}',
      flexShrink: '0'
    },
    numeric: {
      textAlign: 'right',
      fontVariantNumeric: 'tabular-nums'
    },
    nowrap: {
      whiteSpace: 'nowrap'
    },
    triggerLabel: {
      fontSize: '10.5px',
      color: '{colors.text-2}',
      whiteSpace: 'nowrap'
    },
    tableFooter: {
      borderTop: '1px solid {colors.line}',
      padding: '12px 16px',
      display: 'flex',
      alignItems: 'center',
      justifyContent: 'space-between',
      fontSize: '11.5px',
      color: '{colors.text-3}',
      '& button': {
        fontSize: '11.5px'
      }
    }
  }
})
