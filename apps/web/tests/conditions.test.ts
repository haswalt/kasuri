import test from 'node:test'
import assert from 'node:assert/strict'
import { readConditions, conditionProblem, conditionFields } from '../src/lib/conditions.ts'
test('legacy equality opens as a typed condition without mutating its saved configuration', () => {
 const config = { pointer: '/inserted', equals: true }; const before = structuredClone(config)
 const result = readConditions(config); assert.equal(result.rows[0].valueType, 'boolean'); assert.equal(result.rows[0].value,true); assert.deepEqual(config,before)
})
test('structured rows retain typed values and expose incomplete configuration', () => {
 const result=readConditions({match:'any',conditions:[{field:'amount',operator:'greater_than',valueType:'number',value:'100'},{field:'inserted',operator:'equals',valueType:'boolean',value:false}]})
 assert.equal(result.match,'any');assert.equal(conditionProblem(result.rows[0]),undefined);assert.equal(result.rows[1].value,false)
 assert.ok(conditionProblem({...result.rows[0],value:'invalid'}));assert.ok(readConditions({conditions:'not JSON'}).error)
})
test('field suggestions escape JSON Pointer names and bound nested traversal',()=>{
 assert.ok(conditionFields({'a/b':{'~key':true}}).includes('/a~1b/~0key'))
 assert.ok(conditionFields(Object.fromEntries(Array.from({length:1000},(_,i)=>[String(i),i]))).length<=100)
})
