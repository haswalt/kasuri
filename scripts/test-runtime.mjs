import { spawn } from 'node:child_process'
import { mkdtempSync,rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join,resolve } from 'node:path'
import { once } from 'node:events'
import { createServer } from 'node:net'
import { checkRuntime,healthy } from './runtime-checks.mjs'
const data=mkdtempSync(join(tmpdir(),'kasuri-runtime-'))
const probe=createServer();probe.listen(0,'127.0.0.1');await once(probe,'listening');const port=probe.address().port;await new Promise(r=>probe.close(r))
const origin=`http://127.0.0.1:${port}`;let child;let logs=''
function start(){child=spawn(resolve('node_modules/workerd/bin/workerd'),['serve','workerd.capnp','--socket-addr',`http=127.0.0.1:${port}`,'--directory-path',`storage=${data}`],{cwd:resolve('dist'),env:{...process.env,BASIC_AUTH:'test:runtime-check'}});child.stdout.on('data',b=>logs+=b);child.stderr.on('data',b=>logs+=b)}
async function stop(){if(child&&child.exitCode===null){const done=once(child,'exit');child.kill('SIGTERM');await done}}
try{start();await healthy(origin);await checkRuntime(origin,'test:runtime-check',async()=>{await stop();start()})}catch(e){console.error(logs);throw e}finally{await stop();rmSync(data,{recursive:true,force:true})}
