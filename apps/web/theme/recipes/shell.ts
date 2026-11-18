import { defineSlotRecipe } from '@pandacss/dev'

export const shell = defineSlotRecipe({
  className: 'shell',
  slots: [
    'appShell',
    'sidebar',
    'brandLink',
    'brand',
    'alpha',
    'navItem',
    'navCount',
    'sidebarBottom',
    'connectionStatus',
    'avatar',
    'workspaceAvatar',
    'accountText',
    'appMain',
    'topbar',
    'slash',
    'topbarRight'
  ],
  base: {
    appShell: {
      display: 'grid',
      gridTemplateColumns: '{sizes.sidebar-w} minmax(0, 1fr)',
      minHeight: '100vh'
    },
    sidebar: {
      position: 'sticky',
      top: '0',
      height: '100vh',
      background: '{colors.surface}',
      borderRight: '1px solid {colors.line}',
      padding: '22px 12px 14px',
      display: 'flex',
      flexDirection: 'column',
      gap: '22px',
      zIndex: '10',
      '& nav': {
        display: 'flex',
        flexDirection: 'column',
        gap: '3px'
      },
      '& .menu__menuPanel': {
        left: '0',
        right: 'auto',
        width: '266px'
      }
    },
    brandLink: {
      padding: '0 10px',
      display: 'block',
      color: '{colors.text}'
    },
    brand: {
      display: 'inline-flex',
      alignItems: 'center',
      gap: '10px',
      whiteSpace: 'nowrap',
      '& b': {
        fontSize: '19px',
        letterSpacing: '-0.02em'
      }
    },
    alpha: {
      fontSize: '10px',
      color: '{colors.text-3}',
      background: '{colors.surface-2}',
      border: '1px solid {colors.line}',
      padding: '2px 5px',
      borderRadius: '4px',
      letterSpacing: '0'
    },
    navItem: {
      height: '36px',
      display: 'flex',
      alignItems: 'center',
      gap: '10px',
      padding: '0 10px',
      color: '{colors.text-2}',
      borderRadius: '6px',
      fontSize: '14px',
      '&:hover': {
        textDecoration: 'none',
        background: '{colors.surface-2}'
      },
      '&.selected': {
        color: '{colors.accent}',
        background: '{colors.accent-soft}',
        fontWeight: '500'
      }
    },
    navCount: {
      marginLeft: 'auto',
      fontSize: '11px',
      color: '{colors.text-3}',
      background: '{colors.surface}',
      padding: '1px 5px',
      borderRadius: '4px'
    },
    sidebarBottom: {
      marginTop: 'auto',
      display: 'flex',
      flexDirection: 'column',
      gap: '14px',
      '& > .shell__connectionStatus': {
        padding: '0 10px'
      }
    },
    connectionStatus: {
      display: 'inline-flex',
      alignItems: 'center',
      gap: '7px',
      fontSize: '12px',
      color: '{colors.text-2}',
      '& i': {
        width: '6px',
        height: '6px',
        borderRadius: '50%',
        background: '{colors.skip}',
        display: 'inline-block'
      },
      '& i.online': {
        background: '{colors.ok}'
      }
    },
    avatar: {
      flex: 'none',
      display: 'inline-flex',
      alignItems: 'center',
      justifyContent: 'center',
      width: '28px',
      height: '28px',
      borderRadius: '50%',
      background: '{colors.surface-2}',
      color: '{colors.text-2}',
      fontSize: '13px',
      fontWeight: '600'
    },
    workspaceAvatar: {
      flex: 'none',
      display: 'inline-flex',
      alignItems: 'center',
      justifyContent: 'center',
      width: '28px',
      height: '28px',
      borderRadius: '7px',
      background: '{colors.accent-soft}',
      color: '{colors.accent}',
      fontSize: '13px',
      fontWeight: '600'
    },
    accountText: {
      display: 'flex',
      flex: '1',
      minWidth: '0',
      flexDirection: 'column',
      gap: '3px',
      textAlign: 'left',
      '& strong': {
        fontSize: '13px',
        fontWeight: '500'
      },
      '& small': {
        fontSize: '11px'
      }
    },
    appMain: {
      minWidth: '0'
    },
    topbar: {
      height: '{sizes.header-h}',
      display: 'flex',
      alignItems: 'center',
      gap: '10px',
      padding: '0 40px',
      borderBottom: '1px solid {colors.line}',
      background: '{colors.surface}',
      '@media (max-width: 1350px)': {
        paddingInline: '28px'
      }
    },
    slash: {
      color: '{colors.text-3}'
    },
    topbarRight: {
      marginLeft: 'auto',
      border: '1px solid {colors.line}',
      padding: '4px 10px',
      borderRadius: '6px'
    }
  }
})
