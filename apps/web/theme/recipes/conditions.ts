import { defineSlotRecipe } from '@pandacss/dev'
export const conditions = defineSlotRecipe({
  className: 'conditions', slots: ['conditionBuilder', 'conditionRow', 'conditionValue', 'routeRules', 'routeCard'],
  base: {
    conditionBuilder: { display: 'flex', flexDirection: 'column', gap: '12px', minWidth: '0', '& label': { display: 'flex', flexDirection: 'column', gap: '6px', fontSize: '12px' }, '& input:not([type=checkbox]), & select, & textarea': { width: '100%', minWidth: '0' }, '& label:has(> input[type=checkbox])': { flexDirection: 'row', alignItems: 'center' }, '& p': { margin: '0', fontSize: '12px' } },
    conditionRow: { display: 'flex', flexDirection: 'column', gap: '12px', padding: '12px', border: '1px solid {colors.line}', borderRadius: '8px', minWidth: '0', '& legend': { fontSize: '11px', color: '{colors.text-3}', paddingInline: '4px' } },
    conditionValue: { display: 'grid', gridTemplateColumns: '92px minmax(0,1fr)', gap: '8px', alignItems: 'end' },
    routeRules: { display: 'flex', flexDirection: 'column', gap: '24px', minWidth: '0' },
    routeCard: {
      display: 'flex', flexDirection: 'column', gap: '16px', padding: '16px',
      border: '1px solid {colors.line-strong}', borderInlineStart: '3px solid {colors.accent}',
      background: '{colors.surface-2}', borderRadius: '8px', minWidth: '0',
      '& legend': { maxWidth: '100%', paddingInline: '6px', color: '{colors.text-1}', fontWeight: '600', overflowWrap: 'anywhere' }
    }
  }
})
