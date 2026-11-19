import {useEffect,useRef,useState} from 'react'
import type {DataTable} from '../../../../packages/contracts/src/index'
import {api} from '../lib/api'
import {uid} from '../lib/workflows'
import {readRoute} from '../lib/navigation'
import {Button,Dialog} from '../design/ui'
import {styles} from '../design/styles'
type Props={table:DataTable;onChanged:()=>Promise<void>}
export function DataTransfer({table,onChanged}:Props){
 const [open,setOpen]=useState(false),[format,setFormat]=useState<'json'|'csv'>('json'),[mode,setMode]=useState<'insert'|'upsert'>('insert'),[content,setContent]=useState(''),[busy,setBusy]=useState(false),[error,setError]=useState(''),[notice,setNotice]=useState('')
 const [preview,setPreview]=useState<{rows:number;skippedExpired:number;dataRevision:number;key:string}|null>(null)
 const alive=useRef(true)
 useEffect(()=>{alive.current=true;return()=>{alive.current=false}},[])
 const active=(id:string,workspace:string)=>alive.current&&table.definition.id===id&&readRoute().workspaceKey===workspace
 async function check(){const id=table.definition.id,workspace=readRoute().workspaceKey;setBusy(true);setError('');setPreview(null);try{const definition=await api.data({operation:'get_table',tableId:id});if(!active(id,workspace))return;const revision=definition.result.table!.dataRevision;const result=await api.importData(id,{format,content,mode,expectedDataRevision:revision});if(active(id,workspace))setPreview({rows:result.rows!,skippedExpired:result.skippedExpired,dataRevision:result.dataRevision,key:uid()})}catch(e){if(active(id,workspace))setError(String(e))}finally{if(active(id,workspace))setBusy(false)}}
 async function commit(){if(!preview)return;const id=table.definition.id,workspace=readRoute().workspaceKey;setBusy(true);setError('');try{const result=await api.importData(id,{format,content,mode,expectedDataRevision:preview.dataRevision,commit:true,idempotencyKey:preview.key});if(active(id,workspace)){setNotice(`Imported ${result.imported} records; skipped ${result.skippedExpired} expired records.`);setOpen(false);setPreview(null);await onChanged()}}catch(e){if(active(id,workspace))setError(String(e))}finally{if(active(id,workspace))setBusy(false)}}
 async function exportFile(format:'json'|'csv'){
  const id=table.definition.id,workspace=readRoute().workspaceKey;setBusy(true);setError('');setNotice('')
  try{let cursor:string|undefined,revision:number|undefined,total=0,count=0;const parts:string[]=[];let complete=false
   for(let page=0;page<1000;page++){if(!active(id,workspace))return;const result=await api.exportData(id,format,cursor,revision);if(!active(id,workspace))return;revision=result.dataRevision;total+=new TextEncoder().encode(result.content).length;count+=result.count;if(total>10*1024*1024)throw Error('Export exceeds 10 MiB. Use the paged export API for a larger data set.');if(format==='json'){const fragment=result.content.slice(1,-1);if(fragment)parts.push(fragment)}else parts.push(page?result.content.slice(result.content.indexOf('\n')+1):result.content);if(!result.nextCursor){complete=true;break}cursor=result.nextCursor}
   if(!complete)throw Error('Export exceeds the browser page limit. Use the paged export API.');const latest=await api.data({operation:'get_table',tableId:id});if(!active(id,workspace))return;if(latest.result.table?.dataRevision!==revision)throw Error('Data changed during export. Export again.');
   const text=format==='json'?`{"schemaVersion":1,"tableId":${JSON.stringify(id)},"rows":[${parts.join(',')}]}`:parts.join('');const url=URL.createObjectURL(new Blob([text],{type:format==='json'?'application/json':'text/csv;charset=utf-8'}));const a=document.createElement('a');a.href=url;a.download=id+'.'+format;a.click();URL.revokeObjectURL(url);setNotice(`Exported ${count} records.`)
  }catch(e){if(active(id,workspace))setError(String(e))}finally{if(active(id,workspace))setBusy(false)}
 }
 return <section aria-label="Data import and export">
  <div className={styles.actions}><Button disabled={busy} onClick={()=>{setOpen(true);setError('');setPreview(null)}}>Import records</Button><Button disabled={busy} onClick={()=>void exportFile('json')}>Export JSON</Button><Button disabled={busy} onClick={()=>void exportFile('csv')}>Export CSV</Button></div>
  <small>Exports include all live records, regardless of view filters.</small>
  {notice&&<p role="status">{notice}</p>}{error&&!open&&<p role="alert">{error}</p>}
  {open&&<Dialog title="Import records" onClose={()=>{if(!busy)setOpen(false)}}><div className={styles.settingsFields}>
   <p>Preview up to 100 records in a 128 KiB file (up to 64 KiB decoded row data). The entire import commits together; existing records are preserved unless you choose replacement.</p>
   <label>Format<select disabled={busy} value={format} onChange={e=>{setFormat(e.target.value as 'json'|'csv');setPreview(null)}}><option value="json">JSON records</option><option value="csv">Kasuri CSV · JSON key/value columns</option></select></label>
   <label>Import mode<select disabled={busy} value={mode} onChange={e=>{setMode(e.target.value as 'insert'|'upsert');setPreview(null)}}><option value="insert">Insert new records only</option><option value="upsert">Insert or replace records</option></select></label>
   <label>File<input type="file" disabled={busy} accept=".json,.csv,application/json,text/csv" onChange={async e=>{const file=e.target.files?.[0];if(!file)return;setPreview(null);try{if(file.size>131072)throw Error('File exceeds 128 KiB.');setContent(await file.text());setError('')}catch(error){setError(String(error))}}} /></label>
   <label>Records<textarea className={styles.mono} disabled={busy} rows={10} value={content} onChange={e=>{setContent(e.target.value);setPreview(null)}} /></label>
   {preview&&<p role="status">{preview.rows} records · {preview.skippedExpired} already expired · data revision {preview.dataRevision}. {mode==='upsert'?'Matching keys will be replaced.':'Existing keys cause the entire import to be rejected.'}</p>}
   {error&&<p role="alert">{error}</p>}
   <div className={styles.actions}><Button disabled={busy} onClick={()=>setOpen(false)}>Cancel</Button><Button disabled={busy||!content} onClick={()=>void check()}>Preview import</Button><Button visual="primary" disabled={busy||!preview} onClick={()=>void commit()}>Import {preview?preview.rows:''} records</Button></div>
  </div></Dialog>}
 </section>
}
