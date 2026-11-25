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
export function assembledInputHints(bindings:Record<string,Json>,incoming:InputHints):InputHints {
 const properties:Record<string,Json>={},sample:Record<string,Json>={}
 for(const [key,value] of Object.entries(bindings)) {const expression=object(value),op=expression.op;let actual:Json|undefined;let schema:Json={}
  if(op==='literal'){actual=expression.value;schema=inferredSchema(actual??null)}
  else if(op==='input'&&typeof expression.path==='string'){actual=inputValue(incoming.sample,expression.path);schema=schemaAt(incoming.schema,expression.path)??{}}
  else if(['add','subtract','multiply','divide','date_add_ms','date_parse','length'].includes(String(op)))schema={type:'number'}
  else if(['concat','to_text','lower','upper','trim','date_iso'].includes(String(op)))schema={type:'string'}
  else if(['exists','equals','greater_than','less_than','and','or','not'].includes(String(op)))schema={type:'boolean'}
  properties[key]=schema;if(actual!==undefined)sample[key]=actual
 }
 return {sample,schema:{type:'object',properties}}
}
export function workflowInputHints(workflow: Workflow, nodeId: string | undefined, run: Run | null, fixture: string, outputOf=false, unbound=false): InputHints {
  let initial: Json={};try {initial=JSON.parse(fixture)}catch { /* The input editor reports invalid JSON. */ }
  const cache=new Map<string,InputHints>(),visiting=new Set<string>()
  function output(id:string,depth=0):InputHints {
    if(cache.has(id))return cache.get(id)!
    if(depth>12||visiting.has(id))return {sample:null}
    const node=workflow.nodes.find(n=>n.id===id);if(!node)return {sample:null}
    visiting.add(id)
    const incoming=workflow.edges.find(e=>e.target===id)
    const original=incoming?output(incoming.source,depth+1):{sample:initial}
    const parent=Object.keys(node.inputBindings??{}).length?assembledInputHints(node.inputBindings!,original):original
    let hint:InputHints=parent
    if(node.type==='Expressions')hint={sample:null,schema:{type:'object',properties:Object.fromEntries(Object.keys(object(node.config.fields)).map(key=>[key,{}]))}}
    else if(node.type==='Merge'&&node.config.operation)hint={sample:null,schema:node.config.operation==='select'?{}:{type:'array'}}
    else if(node.type==='Split / Batch')hint={sample:null,schema:{type:'object',properties:{batches:{type:'array',items:{type:'array'}},total:{type:'integer'},batchSize:{type:'integer'}}}}
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
    } else if(node.type==='MCP Client') {
      const fixture=object(node.config.simulationResponse)
      hint=node.config.operation==='list'
        ? {sample:{tools:fixture.tools??[],truncated:!!fixture.nextCursor},schema:{type:'object',properties:{
          tools:{type:'array',items:{type:'object',properties:{name:{type:'string'},inputSchema:{type:'object'}}}},truncated:{type:'boolean'}
        }}}
        : {sample:{tool:node.config.tool??'',content:fixture.content??[],structuredContent:fixture.structuredContent??null},
          schema:{type:'object',properties:{tool:{type:'string'},content:{type:'array'},structuredContent:{type:['object','null']}}}}
    } else if(node.type==='Email Trigger') {
      hint={sample:initial,schema:{type:'object',properties:{
        from:{type:'string'},to:{type:'string'},subject:{type:'string'},body:{type:'string'},
        messageId:{type:['string','null']},automatic:{type:'boolean'},
        attachments:{type:'array',items:{type:'object',properties:{
          id:{type:'string'},name:{type:'string'},contentType:{enum:['application/pdf','image/png','image/jpeg','image/webp']},
          size:{type:'number'}
        }}},attachmentsSkipped:{type:'integer'}
      }}}
    } else if(node.type==='Send Email') {
      hint={sample:{status:'preview',messageId:null},schema:{type:'object',properties:{
        status:{enum:['accepted','preview']},messageId:{type:['string','null']}
      }}}
    } else if(node.type==='Clairic') {
      const fields=Array.isArray(node.config.fields)?node.config.fields.map(object):[]
      const fixture=object(node.config.simulationResponse)
      const answers=object(fixture.answers)
      const fieldSchemas: [string, Json][]=fields.flatMap(field=>{
        if (typeof field.name!=='string'||!field.name) return []
        const type=field.type==='number'||field.type==='boolean'?field.type:'string'
        const schema: Json = field.type==='enum'&&Array.isArray(field.enum)
          ? {enum:[...field.enum,null]} : {type:[type,'null']}
        return [[field.name,schema]]
      })
      const names=fieldSchemas.map(([name])=>name)
      const answer=(name:string,key:'value'|'confidence')=>{
        const value=object(answers[name])
        return value.valid===false||value.complete===false?null:value[key]??null
      }
      hint={sample:{values:Object.fromEntries(names.map(name=>[name,answer(name,'value')])),
        confidence:Object.fromEntries(names.map(name=>[name,answer(name,'confidence')])),
        truncated:fixture.truncated??false},
        schema:{type:'object',properties:{values:{type:'object',properties:Object.fromEntries(fieldSchemas)},
        confidence:{type:'object',properties:Object.fromEntries(names.map(name=>[name,{type:['number','null']}]))},
        truncated:{type:'boolean'}}}}
    } else if(node.type==='HTTP Request') {
      const fixture=object(decodeJson(node.config.simulationResponse));hint={sample:{status:fixture.status??200,contentType:'application/json',body:fixture.body??null}}
    }
    const observed=run?.nodes.find(n=>n.id===id)?.output
    const result=observed===undefined?hint:{...hint,sample:observed}
    visiting.delete(id);cache.set(id,result);return result
  }
  if(outputOf&&nodeId)return output(nodeId)
  const source=workflow.edges.find(e=>e.target===nodeId)?.source
  const original=source?output(source):{sample:initial}
  const selected=workflow.nodes.find(n=>n.id===nodeId)
  return !unbound&&Object.keys(selected?.inputBindings??{}).length?assembledInputHints(selected!.inputBindings!,original):original
}
