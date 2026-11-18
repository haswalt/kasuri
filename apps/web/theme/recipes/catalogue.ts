import { defineSlotRecipe } from '@pandacss/dev'

export const catalogue = defineSlotRecipe({
  className: 'catalogue',
  slots: [
    'catalogueToolbar',
    'catalogueGrid',
    'catalogueCard',
    'catalogueCardTop',
    'builtIn',
    'capabilityChips',
    'catalogueFooter',
    'connectionsLayout',
    'connectionDetail'
  ],
  base: {
    catalogueToolbar: {
      display: 'flex',
      alignItems: 'center',
      justifyContent: 'space-between',
      gap: '10px',
      '& .dashboard__tabs button': {
        paddingInline: '8px',
        fontSize: '12px'
      },
      '@media (max-width: 1350px)': {
        flexWrap: 'wrap',
        '& .dashboard__search': {
          marginLeft: 'auto'
        }
      }
    },
    catalogueGrid: {
      display: 'grid',
      gridTemplateColumns: 'repeat(4, minmax(0, 1fr))',
      gap: '14px',
      '@media (min-width: 1700px)': {
        gridTemplateColumns: 'repeat(5, minmax(0, 1fr))'
      }
    },
    catalogueCard: {
      display: 'flex',
      alignItems: 'flex-start',
      flexDirection: 'column',
      justifyContent: 'flex-start',
      border: '1px solid {colors.line}',
      background: '{colors.surface}',
      padding: '16px',
      textAlign: 'left',
      borderRadius: '10px',
      boxShadow: '{shadows.shadow}',
      gap: '8px',
      minHeight: '196px',
      '&:hover': {
        borderColor: '{colors.line-strong}',
        background: '{colors.surface}'
      },
      '& .dashboard__eyebrow': {
        fontSize: '9px'
      },
      '& h2': {
        fontSize: '14px',
        fontWeight: '500'
      },
      '& p': {
        fontSize: '11.5px',
        lineHeight: '1.6',
        fontWeight: '400',
        flex: '1'
      }
    },
    catalogueCardTop: {
      display: 'flex',
      width: '100%',
      alignItems: 'center',
      justifyContent: 'space-between',
      marginBottom: '7px'
    },
    builtIn: {
      display: 'inline-flex',
      alignItems: 'center',
      fontSize: '10.5px',
      padding: '3px 6px',
      borderRadius: '4px',
      background: '{colors.accent-soft}',
      color: '{colors.accent}',
      fontWeight: '400',
      whiteSpace: 'nowrap'
    },
    capabilityChips: {
      display: 'flex',
      flexWrap: 'wrap',
      gap: '5px',
      marginTop: '4px',
      '& span': {
        border: '1px solid {colors.line}',
        borderRadius: '4px',
        padding: '2px 5px',
        fontSize: '9.5px',
        fontWeight: '400',
        color: '{colors.text-3}'
      }
    },
    catalogueFooter: {
      padding: '16px',
      display: 'flex',
      alignItems: 'center',
      gap: '10px',
      border: '1px dashed {colors.line-strong}',
      borderRadius: '8px',
      background: '{colors.accent-soft}',
      fontSize: '12px',
      color: '{colors.text-2}',
      '& small': {
        marginLeft: 'auto'
      }
    },
    connectionsLayout: {
      display: 'grid',
      gridTemplateColumns: 'minmax(0, 1fr) 340px',
      gap: '20px'
    },
    connectionDetail: {
      padding: '24px',
      display: 'flex',
      flexDirection: 'column',
      alignItems: 'flex-start',
      gap: '16px',
      '& p': {
        fontSize: '13px'
      }
    }
  }
})
