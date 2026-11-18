import { defineSlotRecipe } from '@pandacss/dev'

export const palette = defineSlotRecipe({
  className: 'palette',
  slots: [
    'nodeSidebar',
    'paletteSearch',
    'paletteContent',
    'paletteGroup',
    'paletteNode',
    'paletteNote',
    'paletteEmpty'
  ],
  base: {
    nodeSidebar: {
      borderRight: '1px solid {colors.line}',
      background: '{colors.surface}',
      display: 'flex',
      flexDirection: 'column',
      minHeight: '0',
      '& .panel-body': {
        padding: '16px',
        display: 'flex',
        flexDirection: 'column',
        gap: '18px',
        overflow: 'auto'
      },
      '& .panel-body p': {
        fontSize: '11.5px'
      },
      '& textarea': {
        fontSize: '11px',
        minHeight: '230px'
      },
      '& .panel-body button': {
        fontSize: '11px',
        padding: '7px'
      }
    },
    paletteSearch: {
      padding: '12px',
      display: 'flex',
      alignItems: 'center',
      gap: '7px',
      borderBottom: '1px solid {colors.line}',
      color: '{colors.text-3}',
      '& input': {
        border: '0',
        background: '{colors.surface-2}',
        height: '30px',
        padding: '5px',
        fontSize: '12px'
      }
    },
    paletteContent: {
      overflow: 'auto',
      padding: '12px 10px',
      flex: '1'
    },
    paletteGroup: {
      display: 'flex',
      justifyContent: 'space-between',
      alignItems: 'center',
      fontSize: '9px',
      fontWeight: '500',
      letterSpacing: '0.1em',
      textTransform: 'uppercase',
      color: '{colors.text-3}',
      padding: '12px 5px 7px',
      '& small': {
        fontSize: '9px'
      }
    },
    paletteNode: {
      display: 'flex',
      width: '100%',
      gap: '9px',
      justifyContent: 'flex-start',
      border: '0',
      background: 'transparent',
      minHeight: '36px',
      padding: '5px 6px',
      fontSize: '12.5px',
      fontWeight: '400',
      '& > span:nth-child(2)': {
        flex: '1',
        textAlign: 'left'
      },
      '& > svg': {
        color: '{colors.text-3}',
        opacity: '0'
      },
      '&:hover > svg': {
        opacity: '1'
      },
      "&[draggable='true']": {
        cursor: 'grab'
      }
    },
    paletteNote: {
      borderTop: '1px solid {colors.line}',
      padding: '13px 12px',
      display: 'flex',
      alignItems: 'center',
      gap: '7px',
      color: '{colors.text-3}',
      fontSize: '10px'
    },
    paletteEmpty: {
      fontSize: '12px',
      padding: '20px 5px'
    }
  }
})
