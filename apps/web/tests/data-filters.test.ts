import test from 'node:test'
import assert from 'node:assert/strict'
import {filterConfig,dataFilters} from '../src/lib/data-filters.ts'
test('data filters round trip fixed operators and typed values',()=>{const rows=[{pointer:'/active',operator:'eq' as const,value:true},{pointer:'/count',operator:'gt' as const,value:2},{pointer:'',operator:'ne' as const,value:null}];assert.deepEqual(dataFilters(filterConfig(rows)),rows)})
test('data filters normalize paths and reject incomplete or unsupported rules',()=>{assert.deepEqual(dataFilters({conditions:[{field:'user.name',operator:'equals',valueType:'string',value:'Ada'}]}),[{pointer:'/user/name',operator:'eq',value:'Ada'}]);assert.throws(()=>dataFilters(filterConfig([])));assert.throws(()=>dataFilters({conditions:[{field:'x',operator:'contains',value:'a'}]}))})
