import { createWorkflow, appendNode } from '../src/lib/workflows.ts'
import type { Workflow } from '../src/lib/workflows.ts'
export function testWorkflow(): Workflow {
  let workflow = createWorkflow('Webhook enrichment')
  workflow.description = 'Receive an event, enrich its payload and notify the team.'
  workflow.nodes[0] = { ...workflow.nodes[0], type: 'Webhook', label: 'Receive event' }
  workflow = appendNode(workflow, 'HTTP Request')
  workflow.nodes[1] = { ...workflow.nodes[1], label: 'Enrich payload', config: { url: 'https://api.example.com/enrich', method: 'GET' } }
  workflow = appendNode(workflow, 'Map / Transform')
  workflow = appendNode(workflow, 'Slack')
  return workflow
}
