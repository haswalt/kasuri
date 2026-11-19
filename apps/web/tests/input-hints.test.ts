import test from 'node:test'
import assert from 'node:assert/strict'
import {arrayChoices,fieldChoices,inputValue,schemaAt,valueChoices,fieldPointer,workflowInputHints} from '../src/lib/input-hints.ts'
import type {Workflow} from '../../../packages/contracts/src/index.ts'
test('known schema fields and enum values guide inputs without inventing sample data',()=>{
 const hints={sample:{status:'pending',items:[{active:true}]},schema:{type:'object',properties:{status:{enum:['pending','paid']},items:{type:'array',items:{properties:{active:{type:'boolean'}}}}}}}
 assert.ok(fieldChoices(hints).includes('/status'))
 assert.deepEqual(arrayChoices(hints),['/items'])
 assert.deepEqual(valueChoices(hints,'status'),{values:['pending','paid'],source:'schema'})
 assert.deepEqual(schemaAt(hints.schema,'/items/0/active'),{type:'boolean'})
 assert.equal(inputValue(hints.sample,'items.0.active'),true)
 assert.equal(fieldPointer('customer.email'),'/customer/email')
 assert.equal(inputValue({},'/constructor'),undefined)
 assert.equal(schemaAt({properties:{}},'/constructor'),undefined)
})
test('observed values are deduplicated while const and escaped paths remain exact',()=>{
 assert.deepEqual(valueChoices({sample:null},'x',[{x:1},{x:1},{x:2}]).values,[1,2])
 assert.deepEqual(valueChoices({sample:null,schema:{properties:{x:{const:false}}}},'x').values,[false])
 assert.equal(inputValue({'a/b':{'~x':3}},'/a~1b/~0x'),3)
})
test('upstream mappings and collection outputs retain fields for guided sorting',()=>{
 const workflow={nodes:[{id:'map',type:'Map / Transform',config:{mapping:{items:[{rank:1,active:true}]}}},{id:'filter',type:'Filter',config:{arrayPointer:'/items'}},{id:'sort',type:'Sort',config:{}}],edges:[{source:'map',target:'filter'},{source:'filter',target:'sort'}]} as Workflow
 const hints=workflowInputHints(workflow,'sort',null,'{}')
 assert.ok(Array.isArray(hints.sample))
 assert.ok(fieldChoices({sample:hints.sample[0],schema:schemaAt(hints.schema,'/0')}).includes('/rank'))
})
