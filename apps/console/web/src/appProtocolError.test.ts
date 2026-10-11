import assert from "node:assert/strict";
import test from "node:test";
import { createServer } from "node:http";
import { once } from "node:events";
import { AppBridge } from "./apps/appBridge.ts";
import { AppProtocolError, appRequestError, readAppProtocolFailure, type JSONRPCMessage, type Transport } from "./apps/protocol.ts";
import { interceptResourceReadRequests } from "./apps/resourceRead.ts";
import type { AppDescriptor } from "./types.ts";

const standard = {code:-32602, message:"request refused", data:{opaque:{sentinel:"do-not-dump"}}};
const envelope = {kind:"mcpProtocol", error:standard};
const app: AppDescriptor = {server:"fleet",resourceUri:"ui://fleet/overview.html",standalonePath:"/apps/fleet/overview",name:"overview",tools:[],agentMessageTargets:[],resourceDependencies:[],toolDependencies:[]};
async function settle() { await new Promise(resolve => setImmediate(resolve)); }

async function responseFailure(body: unknown, status=422, type="application/json") {
  return readAppProtocolFailure(new Response(JSON.stringify(body),{status, headers:{"content-type":type}}));
}

test("actual HTTP protocol error crosses tool and resource bridges without dumping opaque data", async()=>{
  const server=createServer((_request,response)=>{response.writeHead(422,{"content-type":"application/json"});response.end(JSON.stringify(envelope));});
  server.listen(0,"127.0.0.1"); await once(server,"listening");
  const address=server.address(); assert.ok(address && typeof address !== "string");
  const endpoint=`http://127.0.0.1:${address.port}`;
  try {
    const failure=await readAppProtocolFailure(await fetch(endpoint));
    assert.ok(failure instanceof AppProtocolError);
    assert.equal(failure.message,"The MCP server refused this request.");
    assert.equal(String(failure).includes("do-not-dump"),false);
    assert.equal(JSON.stringify(failure).includes("do-not-dump"),false);
    const sent:JSONRPCMessage[]=[];
    const transport:Transport={start:async()=>{},send:async message=>{sent.push(message);},close:async()=>{}};
    const bridge=new AppBridge(null,{name:"fixture",version:"1"},{},{hostContext:{theme:"dark",displayMode:"inline",availableDisplayModes:["inline"],locale:"en",platform:"web",containerDimensions:{width:100}}});
    bridge.oncalltool=async()=>{throw failure;}; await bridge.connect(transport);
    transport.onmessage?.({jsonrpc:"2.0",id:1,method:"tools/call",params:{name:"inspect",arguments:{}}}); await settle();
    assert.deepEqual(sent[0],{jsonrpc:"2.0",id:1,error:standard}); await bridge.close();
    const reads:JSONRPCMessage[]=[];
    const inner:Transport={start:async()=>{},send:async message=>{reads.push(message);},close:async()=>{}};
    const reader=interceptResourceReadRequests(inner,app,async()=>{throw failure;});
    inner.onmessage?.({jsonrpc:"2.0",id:2,method:"resources/read",params:{uri:"fleet://session/demo"}});await settle();
    assert.deepEqual(reads[0],{jsonrpc:"2.0",id:2,error:standard}); await reader.close();
  } finally { await new Promise<void>((resolve,reject)=>server.close(error=>error?reject(error):resolve())); }
});

test("malformed protocol envelopes and nonprotocol HTTP statuses remain generic", async()=>{
  for(const body of [{...envelope,error:null},{...envelope,error:{code:"-32602",message:"bad"}},{...envelope,error:{code:1.5,message:"bad"}},{...envelope,error:{...standard,unexpected:true}},{...envelope,unexpected:true},{kind:"transport",error:standard}]) assert.equal(await responseFailure(body),undefined);
  for(const status of [400,401,403,502]) assert.equal(await responseFailure(envelope,status),undefined);
  assert.equal(await responseFailure(envelope,422,"text/plain"),undefined);
  assert.equal(await readAppProtocolFailure(new Response("not JSON",{status:422,headers:{"content-type":"application/json"}})),undefined);
});

test("declared and streamed oversized errors are cancelled and never admitted", async()=>{
  let cancelled=false;
  const body=new ReadableStream<Uint8Array>({pull(controller){controller.enqueue(new Uint8Array(2*1024*1024+1));},cancel(){cancelled=true;}});
  assert.equal(await readAppProtocolFailure(new Response(body,{status:422,headers:{"content-type":"application/json"}})),undefined);
  assert.equal(cancelled,true);
  assert.equal(await readAppProtocolFailure(new Response("{}",{status:422,headers:{"content-type":"application/json","content-length":String(2*1024*1024+1)}})),undefined);
});

test("host errors cannot impersonate protocol errors or leak arbitrary details",()=>{
  assert.deepEqual(appRequestError(new Error("opaque do-not-dump transport detail")),{code:-32603,message:"The App host could not complete this request."});
  assert.deepEqual(appRequestError({protocolError:standard}),{code:-32603,message:"The App host could not complete this request."});
});
