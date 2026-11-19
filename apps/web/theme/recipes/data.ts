import { defineSlotRecipe } from '@pandacss/dev'
export const data = defineSlotRecipe({
  className: 'data',
  slots: ['dataLayout', 'dataSidebar', 'dataContent', 'dataHeading', 'dataValue', 'dataFields', 'dataNotice'],
  base: {
    dataLayout: { display: 'grid', gridTemplateColumns: '220px minmax(0,1fr)', gap: '24px', '@media (max-width: 1000px)': { gridTemplateColumns: '1fr' } },
    dataSidebar: { display: 'flex', flexDirection: 'column', gap: '6px', alignSelf: 'start', padding: '12px', border: '1px solid {colors.line}', borderRadius: '10px', background: '{colors.surface}', '& > button': { justifyContent: 'flex-start', textAlign: 'left', width: '100%', '&[aria-current=page]': { background: '{colors.accent-soft}', color: '{colors.accent}', borderColor: '{colors.accent}' } }, '& small': { color: '{colors.text-3}', display: 'block' } },
    dataContent: { minWidth: '0', display: 'flex', flexDirection: 'column', gap: '16px' },
    dataHeading: { display: 'flex', justifyContent: 'space-between', alignItems: 'center', gap: '12px', flexWrap: 'wrap', '& h2': { margin: '0' } },
    dataValue: { display: 'block', fontSize: '11px', maxWidth: '400px', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', color: '{colors.text-2}' },
    dataFields: { display: 'flex', flexWrap: 'wrap', gap: '8px', alignItems: 'center', fontSize: '12px', color: '{colors.text-3}' },
    dataNotice: { padding: '12px 16px', border: '1px solid {colors.line}', borderRadius: '8px', background: '{colors.surface-2}', color: '{colors.text-2}', fontSize: '13px' }
  }
})
