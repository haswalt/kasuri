import type { Json, Run, Workflow } from '../../../../packages/contracts/src/index'
import { conditionFields } from './conditions.ts'
export type InputHints = { sample: Json; schema?: Json }
const object = (value: Json | undefined): Record<string,Json> => value && typeof value === 'object' && !Array.isArray(value) ? value : {}
export function decodeJson(value: Json | undefined): Json | undefined { try { return typeof value === 'string' ? JSON.parse(value) : value } catch { return undefined } }
export function fieldPointer(field: string): string { if (field === '$' || field === '') return ''; return field.startsWith('/') ? field : '/'+field.split('.').map(s=>s.replaceAll('~','~0').replaceAll('/','~1')).join('/') }
export function inputValue(input: Json, field: string): Json | undefined {
  let value: Json | undefined = input
  for (const part of fieldPointer(field).split('/').slice(1)) {
    const key=part.replaceAll('~1','/').replaceAll('~0','~')
    if (!value || typeof value !== 'object' || !Object.hasOwn(value,key) || (Array.isArray(value) && !/^(0|[1-9]\d*)$/.test(key))) return undefined
    value=(value as Record<string,Json>)[key]
  }
  return value
}
export function schemaAt(schema: Json | undefined, field: string): Json | undefined {
  let value=schema
  for (const part of fieldPointer(field).split('/').slice(1)) {
    const current=object(value), key=part.replaceAll('~1','/').replaceAll('~0','~')
    const properties=object(current.properties)
    value=Object.hasOwn(properties,key)?properties[key]:(/^(0|[1-9]\d*)$/.test(key)?current.items:undefined)
  }
  return value
}
export function schemaFields(schema: Json | undefined, path='', depth=0): string[] {
  if (depth>5) return []
  return Object.entries(object(object(schema).properties)).slice(0,60).flatMap(([key,value])=>{ const next=path+'/'+key.replaceAll('~','~0').replaceAll('/','~1'); return [next,...schemaFields(value,next,depth+1)] }).slice(0,100)
}
export function fieldChoices(hints: InputHints): string[] { return [...new Set([...schemaFields(hints.schema),...conditionFields(hints.sample)])].slice(0,100) }
export function arrayChoices(hints: InputHints): string[] { return fieldChoices(hints).filter(path=>Array.isArray(inputValue(hints.sample,path)) || object(schemaAt(hints.schema,path)).type==='array') }
export function valueChoices(hints: InputHints, field: string, samples: Json[] = [hints.sample]): { values: Json[]; source: 'schema' | 'sample' } {
  const schema=object(schemaAt(hints.schema,field))
  if (Array.isArray(schema.enum)) return {values:schema.enum,source:'schema'}
  if (schema.const!==undefined) return {values:[schema.const],source:'schema'}
  const values=samples.map(sample=>inputValue(sample,field)).filter((value):value is Json => value!==undefined)
  return {values:[...new Map(values.map(value=>[JSON.stringify(value),value])).values()].slice(0,20),source:'sample'}
}
function inferredSchema(value:Json, depth=0): Json {
  if(depth>4)return {}
  if(value===null)return {type:'null'}
  if(Array.isArray(value))return {type:'array',items:value.length?inferredSchema(value[0],depth+1):{}}
  if(typeof value==='object')return {type:'object',properties:Object.fromEntries(Object.entries(value).slice(0,60).map(([k,v])=>[k,inferredSchema(v,depth+1)]))}
  return {type:typeof value}
}
export function workflowInputHints(workflow: Workflow, nodeId: string | undefined, run: Run | null, fixture: string): InputHints {
  let initial: Json={};try {initial=JSON.parse(fixture)}catch { /* The input editor reports invalid JSON. */ }
  const cache=new Map<string,InputHints>(),visiting=new Set<string>()
  function output(id:string,depth=0):InputHints {
    if(cache.has(id))return cache.get(id)!
    if(depth>12||visiting.has(id))return {sample:null}
    const node=workflow.nodes.find(n=>n.id===id);if(!node)return {sample:null}
    visiting.add(id)
    const incoming=workflow.edges.find(e=>e.target===id)
    const parent=incoming?output(incoming.source,depth+1):{sample:initial}
    let hint:InputHints=parent
    if(node.type==='Split / Batch')hint={sample:null,schema:{type:'object',properties:{batches:{type:'array',items:{type:'array'}},total:{type:'integer'},batchSize:{type:'integer'}}}}
    else if(node.type==='Each / Loop')hint={sample:null,schema:{type:'object',properties:{items:{type:'array',items:{type:'object',properties:{index:{type:'integer'},itemId:{type:'string'},runId:{type:'string'},status:{enum:['succeeded','failed','cancelled']},output:{},error:{type:'string'}}}},total:{type:'integer'},succeeded:{type:'integer'},failed:{type:'integer'}}}}
    else if(node.type==='Aggregate')hint={sample:null,schema:node.config.operation==='collect'?{type:'array'}:{type:'number'}}
    else if(node.type==='Join') {
      const sample:Record<string,Json>={},properties:Record<string,Json>={}
      for(const edge of workflow.edges.filter(e=>e.target===id)) {const branch=output(edge.source,depth+1);sample[edge.source]=node.config.policy==='collect'?{status:'succeeded',output:branch.sample}:branch.sample;properties[edge.source]=node.config.policy==='collect'?{type:'object',properties:{status:{enum:['succeeded','failed','skipped']},output:branch.schema??{},error:{type:'string'}}}:branch.schema??inferredSchema(branch.sample)}
      hint={sample,schema:{type:'object',properties}}
    } else if(node.type==='Execute Workflow') hint={sample:null,schema:{type:'object',properties:{runId:{type:'string'},workflowId:{type:'string'},revision:{type:'integer'},status:{enum:['running','paused','succeeded','failed','cancelled']},...(node.config.mode==='background'?{}:{output:{}})}}}
    else if(node.type==='Parse Body')hint={sample:null,schema:{type:'object',properties:{valid:{type:'boolean'},value:{},errors:{type:'array'}}}}
    else if(node.type==='Validate Schema')hint={sample:null,schema:{type:'object',properties:{valid:{type:'boolean'},errors:{type:'array'},errorsTruncated:{type:'boolean'},value:decodeJson(node.config.schema)??{}}}}
    else if(node.type==='Map / Transform') {
      const mapping=object(decodeJson(node.config.mapping));const sample:Record<string,Json>={};const properties:Record<string,Json>={}
      for(const [key,value] of Object.entries(mapping)) { const ref=object(value).$input;const actual=typeof ref==='string'?inputValue(parent.sample,ref):value;properties[key]=actual===undefined?{}:inferredSchema(actual);if(actual!==undefined)sample[key]=actual }
      hint={sample,schema:{type:'object',properties}}
    } else if(node.type==='Context Store'||node.type==='Data Tables') {
      const operation=object(decodeJson(node.config.command)).operation
      const status=operation==='insert_if_absent'?['inserted','existing']:operation==='get'?['found','not_found']:undefined
      hint={sample:null,schema:{type:'object',properties:{...(operation==='insert_if_absent'?{inserted:{type:'boolean'}}:{}),status:status?{enum:status}:{type:'string'},row:{type:'object',properties:{key:{type:'string'},value:{},version:{type:'number'}}}}}}
    } else if(node.type==='Request Context') {
      const headers=decodeJson(workflow.nodes.find(n=>n.type==='Webhook')?.config.contextHeaders)??['content-type','x-kasuri-event-id']
      const headerSchema:Json={type:'object',properties:Object.fromEntries((Array.isArray(headers)?headers:[]).filter((h):h is string=>typeof h==='string').map(h=>[h.toLowerCase(),{type:'string'}]))}
      hint=node.config.part==='body'?{sample:initial}:node.config.part==='headers'?{sample:null,schema:headerSchema}:node.config.part==='query'?{sample:null}:{sample:null,schema:{type:'object',properties:{trigger:{enum:['manual','simulation','schedule','webhook']},request:{type:'object',properties:{method:{enum:['POST']},path:{type:'string'},headers:headerSchema,query:{type:'object'}}}}}}
    } else if(node.type==='Filter'||node.type==='Sort') {
      const pointer=typeof node.config.arrayPointer==='string'?node.config.arrayPointer:''
      hint={sample:inputValue(parent.sample,pointer)??null,schema:schemaAt(parent.schema,pointer)}
    } else if(node.type==='HTTP Request') {
      const fixture=object(decodeJson(node.config.simulationResponse));hint={sample:{status:fixture.status??200,contentType:'application/json',body:fixture.body??null}}
    }
    const observed=run?.nodes.find(n=>n.id===id)?.output
    const result=observed===undefined?hint:{...hint,sample:observed}
    visiting.delete(id);cache.set(id,result);return result
  }
  const source=workflow.edges.find(e=>e.target===nodeId)?.source
  return source?output(source):{sample:initial}
}
