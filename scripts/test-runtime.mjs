import { spawn } from 'node:child_process'
import { mkdtempSync,rmSync,readFileSync,writeFileSync,cpSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join,resolve } from 'node:path'
import { once } from 'node:events'
import { createServer } from 'node:net'
import { checkHttp,checkRestrictedHttp,checkAnonymousSecrets } from './http-checks.mjs'
import { checkRuntime,healthy } from './runtime-checks.mjs'
const data=mkdtempSync(join(tmpdir(),'kasuri-runtime-'))
const probe=createServer();probe.listen(0,'127.0.0.1');await once(probe,'listening');const port=probe.address().port;await new Promise(r=>probe.close(r))
cpSync('dist/worker',join(data,'worker'),{recursive:true});cpSync('scripts/fixtures/http-upstream.mjs',join(data,'upstream.mjs'))
const configPath=join(data,'test.capnp')
let config=readFileSync('dist/workerd.capnp','utf8').replace('services = [',`services = [(name = "upstream", worker = (modules = [(name = "upstream.mjs", esModule = embed "upstream.mjs")], compatibilityDate = "2026-11-18")),`).replace('compatibilityDate = "2026-11-18",','globalOutbound = "upstream", compatibilityDate = "2026-11-18",')
writeFileSync(configPath,config)
const origin=`http://127.0.0.1:${port}`;let child;let logs=''
function start(policy={}){child=spawn(resolve('node_modules/workerd/bin/workerd'),['serve',configPath,'--socket-addr',`http=127.0.0.1:${port}`,'--directory-path',`storage=${data}`],{cwd:resolve('dist'),env:{...process.env,BASIC_AUTH:'test:runtime-check',HTTP_ALLOWED_HOSTS:'',HTTP_PRIVATE_HOSTS:'',HTTP_SECRETS:JSON.stringify({api_password:'super-private-password',api_key:'private-api-key',bearer:'Bearer private-bearer'}),...policy}});child.stdout.on('data',b=>logs+=b);child.stderr.on('data',b=>logs+=b)}
async function stop(){if(child&&child.exitCode===null){const done=once(child,'exit');child.kill('SIGTERM');await done}}
try{start();await healthy(origin);await checkRuntime(origin,'test:runtime-check',async()=>{await stop();start()});await checkHttp(origin);await stop();start({HTTP_ALLOWED_HOSTS:'fixture.example,private.internal',HTTP_PRIVATE_HOSTS:'private.internal'});await healthy(origin);await checkRestrictedHttp(origin);await stop();start({BASIC_AUTH:''});await healthy(origin);await checkAnonymousSecrets(origin)}catch(e){console.error(logs);throw e}finally{await stop();rmSync(data,{recursive:true,force:true})}
