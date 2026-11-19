import type {DataFilter,Json} from '../../../../packages/contracts/src/index'
import {conditionProblem,emptyCondition,readConditions} from './conditions.ts'
import type {Condition,ValueType} from './conditions.ts'
import {fieldPointer} from './input-hints.ts'
const names={eq:'equals',ne:'not_equals',gt:'greater_than',lt:'less_than',exists:'exists'} as const
export function filterConfig(filters:DataFilter[]):Record<string,Json>{return {match:'all',conditions:filters.length?filters.map(filter=>({field:filter.pointer||'$',operator:names[filter.operator],value:filter.value??null,valueType:(filter.value===null||filter.value===undefined?'null':typeof filter.value==='object'?'json':typeof filter.value) as ValueType})): [emptyCondition()]}}
export function dataFilters(config:Record<string,Json>):DataFilter[]{
 const result=readConditions(config);if(result.error||result.rows.length>16)throw Error('Use up to 16 valid filters.')
 return result.rows.map((row:Condition)=>{const problem=conditionProblem(row);if(problem)throw Error(problem);const operator=Object.entries(names).find(([,value])=>value===row.operator)?.[0] as DataFilter['operator']|undefined;if(!operator)throw Error('Unsupported data filter operator.');let value=row.value;if((row.valueType==='number'||row.valueType==='json')&&typeof value==='string')value=JSON.parse(value);return{pointer:fieldPointer(row.field),operator,...(operator==='exists'?{}:{value})}})
}
