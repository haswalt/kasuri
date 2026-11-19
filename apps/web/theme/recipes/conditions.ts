import { defineSlotRecipe } from '@pandacss/dev'
export const conditions = defineSlotRecipe({
  className: 'conditions', slots: ['conditionBuilder', 'conditionRow', 'conditionValue'],
  base: {
    conditionBuilder: { display: 'flex', flexDirection: 'column', gap: '12px', minWidth: '0', '& label': { display: 'flex', flexDirection: 'column', gap: '6px', fontSize: '12px' }, '& input, & select, & textarea': { width: '100%', minWidth: '0' }, '& p': { margin: '0', fontSize: '12px' } },
    conditionRow: { display: 'flex', flexDirection: 'column', gap: '12px', padding: '12px', border: '1px solid {colors.line}', borderRadius: '8px', minWidth: '0', '& legend': { fontSize: '11px', color: '{colors.text-3}', paddingInline: '4px' } },
    conditionValue: { display: 'grid', gridTemplateColumns: '92px minmax(0,1fr)', gap: '8px', alignItems: 'end' }
  }
})
