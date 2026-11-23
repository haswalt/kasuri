import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { defaultExpression, expressionOperations, upstreamNodes } from '../src/lib/expressions.ts'
import { ports, connect } from '../src/lib/canvas.ts'
import { parseWorkflow, validateWorkflow } from '../src/lib/workflows.ts'
import { workflowInputHints, fieldChoices } from '../src/lib/input-hints.ts'
test('expression choices define explicit typed references and bounded argument defaults',()=>{
 assert.equal(new Set(expressionOperations.map(o=>o[0])).size,expressionOperations.length)
 assert.deepEqual(defaultExpression('node'),{op:'node',nodeId:'',path:''})
 assert.equal((defaultExpression('if').args as unknown[]).length,3)
})
test('public router example imports, exposes dynamic ports and limits references to ancestors',()=>{
 const workflow=parseWorkflow(readFileSync(new URL('../../../examples/workflows/26-expressions-router.json',import.meta.url),'utf8'))
 assert.deepEqual(validateWorkflow(workflow),[])
 const route=workflow.nodes.find(n=>n.id==='route')!
 assert.deepEqual(ports(route.type,route.config),['large','fallback','error'])
 assert.throws(()=>connect(workflow,'route','start','success'))
 assert.deepEqual(upstreamNodes(workflow,'calculate').map(n=>n.id),['start','source'])
 assert.ok(fieldChoices(workflowInputHints(workflow,'source',null,'{}',true)).includes('/price'))
 assert.ok(fieldChoices(workflowInputHints(workflow,'route',null,'{}')).includes('/total'))
})
test('merge example keeps explicit deterministic source order on import',()=>{
 const workflow=parseWorkflow(readFileSync(new URL('../../../examples/workflows/27-merge-collections.json',import.meta.url),'utf8'))
 assert.deepEqual(validateWorkflow(workflow),[])
 assert.deepEqual(workflow.nodes.find(n=>n.id==='appended')!.config.sources,['stock','catalogue'])
})

test('format-two input assembly imports and guides configuration with assembled field names',()=>{
 const workflow=parseWorkflow(readFileSync(new URL('../../../examples/workflows/29-order-response-bindings.json',import.meta.url),'utf8'))
 assert.equal(workflow.schemaVersion,2)
 assert.deepEqual(validateWorkflow(workflow),[])
 const hints=workflowInputHints(workflow,'reply',null,'{}')
 assert.ok(fieldChoices(hints).includes('/orderId'))
 assert.ok(fieldChoices(hints).includes('/shipping'))
 const original=workflowInputHints(workflow,'reply',null,'{}',false,true)
 assert.ok(fieldChoices(original).includes('/output'))
 const old={...workflow,schemaVersion:1 as const}
 assert.ok(validateWorkflow(old).some(e=>e.includes('format 2')))
})
test('binding references and workflow scope defaults survive import without evaluating strings',()=>{
 const workflow=parseWorkflow(readFileSync(new URL('../../../examples/workflows/31-bound-http-and-condition.json',import.meta.url),'utf8'))
 assert.deepEqual(workflow.nodes.find(n=>n.id==='send')!.inputBindings!.orderId,{op:'node',nodeId:'order',path:'/orderId'})
 assert.deepEqual(defaultExpression('workflow'),{op:'workflow',path:'/id'})
 const raw=JSON.parse(readFileSync(new URL('../../../examples/workflows/31-bound-http-and-condition.json',import.meta.url),'utf8'));raw.nodes[0].inputBindings=[]
 assert.throws(()=>parseWorkflow(JSON.stringify(raw)),/bindings must be an object/)
})
