import type { Json, Workflow } from '../../../../packages/contracts/src/index'
import type { InputHints } from '../lib/input-hints'
import { ExpressionBuilder } from './ExpressionBuilder'
import { Button } from '../design/ui'
import { styles } from '../design/styles'

type Props = {
  workflow: Workflow
  nodeId: string
  bindings: Record<string, Json>
  hints: InputHints
  nodeHints: (id: string) => InputHints
  triggerHints: InputHints
  onChange: (bindings: Record<string, Json>) => void
}

export function InputBindingsBuilder({ workflow, nodeId, bindings, hints, nodeHints, triggerHints, onChange }: Props) {
  return <details className={styles.conditionBuilder}>
    <summary>Existing input mapping</summary>
    <p>This workflow maps incoming data into the fields below before the node runs.</p>
    <ExpressionBuilder config={{ fields: bindings }} workflow={workflow} nodeId={nodeId} hints={hints} nodeHints={nodeHints} triggerHints={triggerHints}
      onChange={config => onChange(config.fields as Record<string, Json>)} />
    <Button visual="ghost" onClick={() => onChange({})}>Remove mapping and use incoming data</Button>
  </details>
}
