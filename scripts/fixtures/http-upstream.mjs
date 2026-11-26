// Test-only upstream. The application backend remains entirely Rust.
const itemAttempts=new Map()
const barriers=new Map()
let retryCount=0
let nextMcpSession=0
const mcpSessions=new Map()
async function mcp(request, portal = false) {
  const authorization = request.headers.get('Authorization')
  const accessId = request.headers.get('CF-Access-Client-Id')
  const accessSecret = request.headers.get('CF-Access-Client-Secret')
  if (portal
    ? authorization || accessId !== 'fixture-portal-client-id' || !['fixture-portal-client-secret','fixture-portal-client-secret-rotated'].includes(accessSecret)
    : authorization !== 'Bearer fixture-mcp-token' || accessId || accessSecret) {
    return new Response(null,{status:401})
  }
  if (request.headers.get('Accept') !== 'application/json, text/event-stream') return new Response(null,{status:400})
  const session = request.headers.get('Mcp-Session-Id')
  if (request.method === 'DELETE') {
    return new Response(null,{status:mcpSessions.delete(session)?204:404})
  }
  const {jsonrpc,id,method,params} = await request.json()
  if (jsonrpc !== '2.0') return new Response(null,{status:400})
  const reply = result => ({jsonrpc:'2.0',id,result})
  if (method === 'initialize' && !session && params.protocolVersion === '2025-11-25') {
    if (mcpSessions.size !== 0) return new Response(null,{status:409})
    const key = 'fixture-session-' + ++nextMcpSession
    mcpSessions.set(key, false)
    return Response.json(reply({
      protocolVersion:'2025-06-18',capabilities:{tools:{}},serverInfo:{name:'Fixture',version:'1'}
    }),{headers:{'Mcp-Session-Id':key}})
  }
  if (!mcpSessions.has(session) || request.headers.get('Mcp-Protocol-Version') !== '2025-06-18') {
    return new Response(null,{status:404})
  }
  if (method === 'notifications/initialized' && id === undefined) {
    mcpSessions.set(session,true)
    return new Response(null,{status:202})
  }
  if (!mcpSessions.get(session)) return new Response(null,{status:400})
  const contract = name => ({
    name, description:name,
    inputSchema:{type:'object',properties:{date:{type:'string'}}}
  })
  if (method === 'tools/list') {
    return Response.json(reply(params.cursor === 'page-2'
      ? {tools:[contract('check_availability'),contract('returns_error')]}
      : {tools:[contract('hidden')],nextCursor:'page-2'}))
  }
  if (method !== 'tools/call') return new Response(null,{status:400})
  if (params.name === 'returns_error') {
    return Response.json(reply({isError:true,content:[{type:'text',text:'private error detail'}]}))
  }
  if (params.name !== 'check_availability' || params.arguments.date !== '2027-06-12') {
    return Response.json({jsonrpc:'2.0',id,error:{code:-32602,message:'Unexpected arguments'}})
  }
  const result = JSON.stringify(reply({
    content:[{type:'text',text:portal ? accessSecret : 'fixture-mcp-token'}],
    structuredContent:{available:true, session, ...(portal?{clientId:accessId}:{})}
  }))
  const encoder = new TextEncoder()
  const stream = new ReadableStream({start(controller) {
    for (const piece of [
      'id: primer\r\ndata:\r', '\n\r\n',
      'data: {"jsonrpc":"2.0","method":"notifications/message"}\r\n\r\n',
      'da', 'ta: ' + result.slice(0,20), result.slice(20) + '\r\n\r\n'
    ]) controller.enqueue(encoder.encode(piece))
    controller.close()
  }})
  return new Response(stream,{headers:{'Content-Type':'text/event-stream; charset=utf-8'}})
}
export default {async fetch(request){
 const url=new URL(request.url)
 if(url.hostname==='cloudflare-dns.com'){
  if(request.headers.has('authorization')||request.headers.has('x-api-key')||request.headers.has('CF-Access-Client-Id')||request.headers.has('CF-Access-Client-Secret'))return new Response('Credentials leaked to DNS',{status:500})
  const name=url.searchParams.get('name');const type=url.searchParams.get('type')==='AAAA'?28:1
  const data=name==='private-dns.example'?'127.0.0.1':name==='mixed-dns.example'&&type===28?'fe80::1':type===1?'1.1.1.1':'2606:4700:4700::1111'
  return Response.json({Status:0,Answer:[{type,data}]})
 }
 if(url.pathname==='/mcp-client')return mcp(request)
 if(url.pathname==='/mcp-portal')return mcp(request,true)
 if(url.hostname==='sentry-fixture.example' && url.pathname==='/api/100/envelope/'){
  if(request.method!=='POST'||request.headers.get('content-type')!=='application/x-sentry-envelope')return new Response(null,{status:415})
  const [header,item,event]=(await request.text()).trimEnd().split('\n').map(line=>JSON.parse(line))
  if(header.dsn!=='https://fixture@sentry-fixture.example/100'||item.type!=='event'||
     event.event_id!==header.event_id||!event.tags['kasuri.run_id']||
     event.tags['kasuri.error_code']!=='invalid_config'||event.logentry?.formatted!=='Kasuri workflow node failed'||
     typeof event.timestamp!=='number') {
   return new Response(null,{status:400})
  }
  return Response.json({id:header.event_id,privateFixtureResponse:'not returned to the run'})
 }
 // The upstream's clock, for load measurements (scripts/scale-check.mjs).
 if(url.pathname==='/clock')return Response.json({now:Date.now()})
 if(url.pathname==='/item-worker'){
  const id=url.searchParams.get('id');const attempt=(itemAttempts.get(id)||0)+1;itemAttempts.set(id,attempt)
  if(id==='retry'&&attempt===1)return new Response('retry',{status:429})
  await new Promise(resolve=>setTimeout(resolve,id==='slow'?350:10))
  return Response.json({id,attempt})
 }
 if(url.pathname==='/parallel-barrier'){
  const key=url.searchParams.get('group')||'default';let barrier=barriers.get(key)
  if(!barrier){let resolve;const ready=new Promise(r=>resolve=r);barrier={count:0,done:0,ready,resolve};barriers.set(key,barrier)}
  barrier.count++;if(barrier.count===2)barrier.resolve(true)
  let timer;const overlapped=await Promise.race([barrier.ready,new Promise(r=>timer=setTimeout(()=>r(false),2500))]);clearTimeout(timer)
  barrier.done++;if(barrier.done===2)barriers.delete(key)
  return Response.json({overlapped,arrivals:barrier.count},{status:overlapped?200:504})
 }
 if(url.pathname==='/auth'){
  const authorization=request.headers.get('authorization');const apiKey=request.headers.get('x-api-key')
  if(authorization!=='Basic '+btoa('api-user:super-private-password')||apiKey!=='private-api-key'||request.headers.get('x-tenant')!=='team-a')return new Response('Authentication failed',{status:401})
  return Response.json({authenticated:true,authorization,apiKey,password:'super-private-password'})
 }
 if(url.pathname==='/bearer')return Response.json({authenticated:request.headers.get('authorization')==='Bearer private-bearer',echo:request.headers.get('authorization')})

 if(url.pathname==='/redirect')return new Response(null,{status:302,headers:{location:'https://private.internal/'}})
 if(url.pathname==='/oversize')return new Response('x'.repeat(40000))
 if(url.pathname==='/invalid')return new Response('not JSON')
 if(url.pathname==='/slow')await new Promise(r=>setTimeout(r,1500))
 if(url.pathname==='/retry'&&++retryCount===1)return new Response('upstream-secret',{status:429})
 if(url.pathname==='/echo')return Response.json({method:request.method,body:await request.json(),query:url.searchParams.get('q'),key:request.headers.get('idempotency-key')},{headers:{'Set-Cookie':'secret=never-store'}})
 return Response.json({items:[{name:'B',active:true},{name:'A',active:false},{name:'A',active:true}]})
}}
