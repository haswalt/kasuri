// Test-only upstream. The application backend remains entirely Rust.
let retryCount=0
export default {async fetch(request){
 const url=new URL(request.url)
 if(url.pathname==='/redirect')return new Response(null,{status:302,headers:{location:'https://private.internal/'}})
 if(url.pathname==='/oversize')return new Response('x'.repeat(40000))
 if(url.pathname==='/invalid')return new Response('not JSON')
 if(url.pathname==='/slow')await new Promise(r=>setTimeout(r,1500))
 if(url.pathname==='/retry'&&++retryCount===1)return new Response('upstream-secret',{status:429})
 if(url.pathname==='/echo')return Response.json({method:request.method,body:await request.json(),query:url.searchParams.get('q'),key:request.headers.get('idempotency-key')},{headers:{'Set-Cookie':'secret=never-store'}})
 return Response.json({items:[{name:'B',active:true},{name:'A',active:false},{name:'A',active:true}]})
}}
