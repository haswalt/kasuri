// Test-only upstream. The application backend remains entirely Rust.
let retryCount=0
export default {async fetch(request){
 const url=new URL(request.url)
 if(url.hostname==='cloudflare-dns.com'){
  if(request.headers.has('authorization')||request.headers.has('x-api-key'))return new Response('Credentials leaked to DNS',{status:500})
  const name=url.searchParams.get('name');const type=url.searchParams.get('type')==='AAAA'?28:1
  const data=name==='private-dns.example'?'127.0.0.1':name==='mixed-dns.example'&&type===28?'fe80::1':type===1?'1.1.1.1':'2606:4700:4700::1111'
  return Response.json({Status:0,Answer:[{type,data}]})
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
