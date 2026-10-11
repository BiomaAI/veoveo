import assert from "node:assert/strict";
import test from "node:test";
import {readFile} from "node:fs/promises";
import {runInNewContext} from "node:vm";
import {LiveViewOwnership, replyToTeardown} from "./live-view-ownership.js";

function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return {promise, resolve, reject};
}
const connection = id => ({stream: {liveViewId: id, sessionId: "session-owned", resourceUri: `uav-sim://session/session-owned/live-view/${id}`}});
const closedResult = value => ({closed: true, resourceUri: value.stream.resourceUri});
const turn = () => new Promise(resolve => setImmediate(resolve));

test("teardown waits for a delayed acquisition and its exact remote close", async () => {
  const opening = deferred(), closing = deferred(), closed = [];
  const owner = new LiveViewOwnership(async value => { closed.push(value); await closing.promise; return closedResult(value); });
  const acquisition = owner.acquire(() => opening.promise);
  const replies = [];
  const teardown = replyToTeardown(71, reply => replies.push(reply), () => owner.teardown(), assert.fail);
  await turn();
  assert.equal(replies.length, 0);
  const value = connection("late-owned");
  opening.resolve(value);
  assert.equal(await acquisition, value);
  await turn();
  assert.deepEqual(closed, [value]);
  assert.equal(replies.length, 0);
  closing.resolve();
  await teardown;
  assert.deepEqual(replies, [{jsonrpc: "2.0", id: 71, result: {}}]);
  assert.equal(owner.authorizations.size, 0);
});

test("remote close refusal stays owned and concurrent close waiters retry the same identity", async () => {
  const close = deferred(), calls = [];
  let refuse = true;
  const owner = new LiveViewOwnership(async value => {
    calls.push(value.stream.liveViewId);
    if (refuse) { await close.promise; throw new Error("remote refusal"); }
    return closedResult(value);
  });
  await owner.acquire(async () => connection("retained"));
  const first = owner.close("retained"), second = owner.close("retained");
  assert.equal(first, second);
  close.resolve();
  await assert.rejects(first, /remote refusal/);
  assert.equal(owner.authorizations.has("retained"), true);
  const replies = [];
  await replyToTeardown(72, reply => replies.push(reply), () => owner.teardown(), () => {});
  assert.equal(replies[0].error.code, -32603);
  assert.equal(owner.authorizations.has("retained"), true);
  refuse = false;
  await replyToTeardown(73, reply => replies.push(reply), () => owner.teardown(), assert.fail);
  assert.deepEqual(replies[1], {jsonrpc: "2.0", id: 73, result: {}});
  assert.deepEqual(calls, ["retained", "retained", "retained"]);
});

test("teardown closes all owned identities despite one refusal and never closes unrelated viewers", async () => {
  const calls = [];
  const owner = new LiveViewOwnership(async value => {
    calls.push(value.stream.liveViewId);
    if (value.stream.liveViewId === "first") throw new Error("first refused");
    return closedResult(value);
  });
  await owner.acquire(async () => connection("first"));
  await owner.acquire(async () => connection("second"));
  await assert.rejects(owner.teardown(), /release failed/);
  assert.deepEqual(calls.sort(), ["first", "second"]);
  assert.equal(owner.authorizations.has("first"), true);
  assert.equal(owner.authorizations.has("second"), false);
  await owner.close("unrelated");
  assert.equal(calls.length, 2);
});

test("teardown stops new acquisition dispatch while tool replies can settle pending ownership", async () => {
  const opening = deferred();
  const owner = new LiveViewOwnership(async value => closedResult(value));
  const pending = owner.acquire(() => opening.promise);
  const teardown = owner.teardown();
  let dispatched = false;
  await assert.rejects(owner.acquire(async () => { dispatched = true; return connection("wrong"); }), /closing/);
  assert.equal(dispatched, false);
  opening.resolve(connection("already-dispatched"));
  await pending;
  await teardown;
});

test("lost or invalid acquisition responses refuse teardown without replay while known owners close", async () => {
  for (const cause of [new Error("transport lost after dispatch"), new Error("Invalid connection response")]) {
    let dispatches = 0;
    const closed = [];
    const owner = new LiveViewOwnership(async value => { closed.push(value.stream.liveViewId); return closedResult(value); });
    await owner.acquire(async () => connection("known"));
    await assert.rejects(owner.acquire(async () => { dispatches++; throw cause; }), error => error === cause);
    const replies = [];
    await replyToTeardown("unknown", reply => replies.push(reply), () => owner.teardown(), () => {});
    assert.deepEqual(closed, ["known"]);
    assert.equal(replies[0].error.code, -32603);
    await assert.rejects(owner.teardown(), /acquisition outcome remains unresolved/);
    assert.equal(dispatches, 1);
    assert.equal(owner.unresolvedAcquisitions.size, 1);
  }
});

test("only closed true for the same resource settles the acknowledged authorization", async () => {
  const value = connection("exact-resource");
  for (const result of [{closed: false, resourceUri: value.stream.resourceUri}, {closed: true, resourceUri: "uav-sim://unrelated"}]) {
    const owner = new LiveViewOwnership(async () => result);
    await owner.acquire(async () => value);
    await assert.rejects(owner.close(value.stream.liveViewId), /does not settle/);
    assert.equal(owner.authorizations.get(value.stream.liveViewId), value);
    await assert.rejects(owner.teardown(), /release failed/);
  }
});

test("actual App teardown responds with standard correlated success despite stale visible recovery error", async () => {
  const listeners = new Map(), messages = [];
  const error = {textContent: "stale recovery failure", hidden: false};
  const nodes = new Map([["error", error]]);
  const element = id => {
    if (!nodes.has(id)) nodes.set(id, {addEventListener() {}, textContent: "", classList: {toggle() {}}});
    return nodes.get(id);
  };
  const parent = {postMessage: message => messages.push(message)};
  const source = (await readFile(new URL("./main.js", import.meta.url), "utf8"))
    .replace(/^import .*;\n/gm, "");
  runInNewContext(source, {
    LiveViewOwnership, replyToTeardown, $: element, parent,
    crypto: {randomUUID: () => "viewer-owned"},
    addEventListener: (name, callback) => listeners.set(name, callback),
    ResizeObserver: class {observe() {}},
    requestAnimationFrame() {}, clearTimeout() {},
  });
  // Initialization remains pending; its reply pump is still available during teardown.
  listeners.get("message")({source: parent, data: {jsonrpc: "2.0", id: "teardown-actual", method: "ui/resource-teardown", params: {}}});
  await turn();
  const result = messages.find(value => value.id === "teardown-actual");
  assert.deepEqual(JSON.parse(JSON.stringify(result)), {jsonrpc: "2.0", id: "teardown-actual", result: {}});
  assert.equal(error.textContent, "stale recovery failure");
});

import {AppProtocolError, provesNonAdmission} from "./contracts.js";
const request = {sessionId:"session-owned",cameraId:"follow",viewerInstanceId:"viewer-owned"};
const refusal = (args=request, type="open") => new AppProtocolError(-32600,"not ready",{
  schema:"veoveo.ai/live-view-non-admission/v1",operation:{type,request:args},
});

test("owner readiness rejection settles only its fully correlated acquisition", async () => {
  const owner=new LiveViewOwnership(async value=>closedResult(value));
  await assert.rejects(owner.acquire(async()=>{throw refusal();},cause=>provesNonAdmission(cause,"open_live_view",request)));
  await owner.teardown();
  assert.equal(owner.unresolvedAcquisitions.size,0);
});

test("wrong, malformed, lost and earlier unknown acquisition proofs stay unresolved", async () => {
  for(const cause of [new Error("not ready"),refusal({...request,cameraId:"chase"}),refusal(request,"renew"),
    new AppProtocolError(-32600,"not ready",{schema:"veoveo.ai/live-view-non-admission/v2",operation:{type:"open",request}}),
    new AppProtocolError(-32600,"not ready",{schema:"veoveo.ai/live-view-non-admission/v1",operation:{type:"open",request},extra:true})]) {
    const owner=new LiveViewOwnership(async value=>closedResult(value));
    await assert.rejects(owner.acquire(async()=>{throw cause;},failure=>provesNonAdmission(failure,"open_live_view",request)));
    await assert.rejects(owner.teardown(),/unresolved/);
  }
  const owner=new LiveViewOwnership(async value=>closedResult(value));
  await assert.rejects(owner.acquire(async()=>{throw new Error("lost response");}));
  await assert.rejects(owner.acquire(async()=>{throw refusal();},cause=>provesNonAdmission(cause,"open_live_view",request)));
  await assert.rejects(owner.teardown(),/unresolved/);
  assert.equal(owner.unresolvedAcquisitions.size,1);
});


test("renewal refusal matches every canonical field and cannot release an existing authorization",async()=>{
  const args={sessionId:"session-owned",liveViewId:"renew-owned",viewerInstanceId:"viewer-owned"};
  const owner=new LiveViewOwnership(async value=>closedResult(value));
  await owner.acquire(async()=>connection("renew-owned"));
  const error=refusal(args,"renew");
  assert.equal(provesNonAdmission(error,"renew_live_view",args),true);
  for(const key of Object.keys(args))assert.equal(provesNonAdmission(refusal({...args,[key]:"other"},"renew"),"renew_live_view",args),false);
  assert.equal(provesNonAdmission(error,"open_live_view",request),false);
  await assert.rejects(owner.acquire(async()=>{throw error;},cause=>provesNonAdmission(cause,"renew_live_view",args)));
  assert.equal(owner.authorizations.has("renew-owned"),true);
  await owner.teardown();
});

test("actual App reply pump preserves correlated standard error code and owner data",async()=>{
  const listeners=new Map(),messages=[],errors=[];
  const parent={postMessage:message=>messages.push(message)};
  const nodes=new Map();
  const element=id=>{if(!nodes.has(id))nodes.set(id,{addEventListener(){},textContent:"",classList:{toggle(){}}});return nodes.get(id);};
  class RecordedError extends AppProtocolError {constructor(code,message,data){super(code,message,data);errors.push(this);}}
  const source=(await readFile(new URL("./main.js",import.meta.url),"utf8")).replace(/^import .*;\n/gm,"");
  runInNewContext(source,{LiveViewOwnership,replyToTeardown,AppProtocolError:RecordedError,$:element,parent,
    crypto:{randomUUID:()=>"viewer-owned"},addEventListener:(name,callback)=>listeners.set(name,callback),
    ResizeObserver:class {observe(){}},requestAnimationFrame(){},clearTimeout(){}});
  const pending=messages.find(value=>value.method==="ui/initialize");
  const wire={jsonrpc:"2.0",id:pending.id,error:{code:-32600,message:"not ready",data:refusal().data}};
  listeners.get("message")({source:{},data:wire});
  assert.equal(errors.length,0);
  listeners.get("message")({source:parent,data:wire});
  await turn();
  assert.equal(errors.length,1);
  assert.equal(errors[0].code,wire.error.code);
  assert.deepEqual(errors[0].data,wire.error.data);
  assert.equal(provesNonAdmission(errors[0],"open_live_view",request),true);
});

async function recoveryApp() {
  const listeners=new Map(),timers=new Map(),messages=[],nodes=new Map();let nextTimer=0, rejectUnknown=false;
  class Element {
    constructor(tag){this.tag=tag;this.children=[];this.dataset={};this.classList={toggle(){},add(){}};}
    addEventListener(){} append(...children){this.children.push(...children);} replaceChildren(...children){this.children=children;}
    querySelector(selector){if(selector===".placeholder")return null;if(!this.button)this.button=new Element("button");return this.button;}
    remove(){if(this.id)nodes.delete(this.id);}
  }
  const element=id=>{if(!nodes.has(id))nodes.set(id,new Element("div"));return nodes.get(id);};
  const camera={cameraId:"follow",widthPx:1280,heightPx:720,frameRateMillihertz:16000,streamPolicy:"continuous",rig:{kind:"follow"}};
  const parent={postMessage(message){messages.push(message);if(message.id===undefined)return;queueMicrotask(()=>{
    let result={};let error;
    if(message.method==="resources/read")result=[{sessionId:"session-owned"}];
    if(message.method==="tools/call"){
      if(message.params.name==="list_live_cameras")result=[camera];
      else if(message.params.name==="open_live_view")error={code:-32600,message:"not ready",...(rejectUnknown?{}:{data:{schema:"veoveo.ai/live-view-non-admission/v1",operation:{type:"open",request:message.params.arguments}}})};
      else assert.fail("Unexpected mutation "+message.params.name);
    }
    listeners.get("message")({source:parent,data:{jsonrpc:"2.0",id:message.id,...(error?{error}:{result})}});
  });}};
  const source=(await readFile(new URL("./main.js",import.meta.url),"utf8")).replace(/^import .*;\n/gm,"");
  runInNewContext(source,{LiveViewOwnership,replyToTeardown,AppProtocolError,provesNonAdmission,$:element,parent,structuredClone,
    // Host fixture returns already-admitted observations; the production recovery and bridge execute unchanged.
    resourceJson:value=>value,resourceValue:(_uri,value)=>value,structuredResult:value=>value,toolValue:(_name,value)=>value,
    document:{createElement:tag=>new Element(tag),createTextNode:text=>({text}),documentElement:{dataset:{}}},
    crypto:{randomUUID:()=>"viewer-owned"},addEventListener:(name,callback)=>listeners.set(name,callback),
    ResizeObserver:class {observe(){}},requestAnimationFrame(){},
    setTimeout:callback=>{timers.set(++nextTimer,callback);return nextTimer;},clearTimeout:id=>timers.delete(id)});
  const settle=async()=>{for(let i=0;i<5;i++)await turn();};await settle();
  return {timers,messages,settle,setUnknown:()=>{rejectUnknown=true;},
    opens:()=>messages.filter(value=>value.method==="tools/call"&&value.params.name==="open_live_view").length,
    update:()=>listeners.get("message")({source:parent,data:{jsonrpc:"2.0",method:"ui/notifications/resource-updated",params:{uri:"uav-sim://session/session-owned/live-cameras"}}}),
    select:checked=>{const input=element("choices").children[0].children[0];input.checked=checked;input.onchange();}};
}

test("actual App timers, resource updates and reselect cannot replay unknown Open but readiness remains retryable",async()=>{
  for(const unknown of [true,false]){
    const app=await recoveryApp();assert.equal(app.opens(),1);assert.equal(app.timers.size,1);
    const timer=[...app.timers.values()][0];app.timers.clear();if(unknown)app.setUnknown();timer();await app.settle();
    assert.equal(app.opens(),2);
    const before=app.opens();timer();await app.settle();app.update();await app.settle();
    app.select(false);await app.settle();app.select(true);await app.settle();
    if(unknown){assert.equal(app.opens(),before);assert.equal(app.timers.size,0);}
    else assert.ok(app.opens()>before,"fully correlated readiness remains retryable");
  }
});

test("unknown Renew stays fenced until exact owned close, including late completion, while unknown Open survives other closes",async()=>{
  const value=connection("renew-lost"),owner=new LiveViewOwnership(async item=>closedResult(item));
  await owner.acquire(async()=>value);
  const renewal=deferred();const pending=owner.acquire(()=>renewal.promise,()=>false,{cameraIds:["follow","chase"],renewal:value});
  assert.equal(owner.canAcquire("follow"),false);
  await owner.close(value.stream.liveViewId);renewal.reject(new Error("late lost renewal"));await assert.rejects(pending);
  assert.equal(owner.canAcquire("follow"),true);assert.equal(owner.unresolvedAcquisitions.size,1);
  assert.equal([...owner.unresolvedAcquisitions][0].fenced,true);await owner.teardown();
  const other=new LiveViewOwnership(async item=>closedResult(item));await other.acquire(async()=>connection("previous"));
  await assert.rejects(other.acquire(async()=>{throw new Error("lost open");},()=>false,{cameraIds:["follow"]}));
  await other.close("previous");assert.equal(other.canAcquire("follow"),false);await assert.rejects(other.teardown(),/unresolved/);
});

test("failed Renew close keeps recovery blocked and late successful renewal cannot revive closed ownership",async()=>{
  const value=connection("renew-fenced");let refuse=true;
  const owner=new LiveViewOwnership(async item=>{if(refuse)throw new Error("close unknown");return closedResult(item);});
  await owner.acquire(async()=>value);
  await assert.rejects(owner.acquire(async()=>{throw new Error("renew reply lost");},()=>false,{cameraIds:["follow"],renewal:value}));
  await assert.rejects(owner.close(value.stream.liveViewId));assert.equal(owner.canAcquire("follow"),false);
  refuse=false;await owner.close(value.stream.liveViewId);assert.equal(owner.canAcquire("follow"),true);
  assert.equal([...owner.unresolvedAcquisitions][0].fenced,true);
  const other=new LiveViewOwnership(async item=>closedResult(item));await other.acquire(async()=>value);
  const response=deferred();const pending=other.acquire(()=>response.promise,()=>false,{cameraIds:["follow"],renewal:value});
  await other.close(value.stream.liveViewId);response.resolve(value);await pending;
  assert.equal(other.authorizations.size,0);assert.equal(other.canAcquire("follow"),true);await other.teardown();
});

async function mountedRecoveryApp(){
  const listeners=new Map(),nodes=new Map(),messages=[],opens=[],closes=[],sockets=[],timers=new Map();let timerId=0;
  let followRequest,closeRequest;
  class Element {
    constructor(tag){this.tagName=tag.toUpperCase();this.children=[];this.dataset={};this.textContent="";this.classList={toggle(){},add(){}};}
    addEventListener(){}
    append(...children){this.children.push(...children);for(const child of children)if(child.id)nodes.set(child.id,child);}
    replaceChildren(...children){this.children=children;}
    querySelector(selector){
      if(selector===".placeholder")return null;
      this.parts??=new Map();
      if(!this.parts.has(selector))this.parts.set(selector,new Element(selector==="canvas"?"canvas":"div"));
      return this.parts.get(selector);
    }
    getContext(){return {drawImage(){}};}
    remove(){if(nodes.get(this.id)===this)nodes.delete(this.id);}
  }
  for(const [id,tag] of Object.entries({app:"div",decode:"span",status:"span",count:"span",choices:"section",commands:"form",agent:"select",command:"input","command-result":"output",grid:"main",error:"div",session:"span"})){
    const node=new Element(tag);node.id=id;nodes.set(id,node);
  }
  const document={getElementById:id=>nodes.get(id)??null,createElement:tag=>new Element(tag),createTextNode:text=>({text}),documentElement:{dataset:{}}};
  const cameras=["follow","formation"].map(cameraId=>({cameraId,widthPx:1280,heightPx:720,frameRateMillihertz:16000,streamPolicy:"continuous",rig:{kind:cameraId}}));
  const respond=(request,result)=>listeners.get("message")({source:parent,data:{jsonrpc:"2.0",id:request.id,result}});
  const parent={postMessage(message){messages.push(message);if(message.id===undefined||message.method===undefined)return;queueMicrotask(()=>{
    if(message.method==="resources/read")return respond(message,[{sessionId:"session-owned"}]);
    if(message.method==="tools/call"){
      if(message.params.name==="list_live_cameras")return respond(message,cameras);
      if(message.params.name==="open_live_view"){opens.push(message.params.arguments.cameraId);followRequest=message;return;}
      if(message.params.name==="close_live_view"){closes.push(message.params.arguments.liveViewId);closeRequest=message;return;}
      if(message.params.name==="renew_live_view")return;
      assert.fail("Unexpected tool "+message.params.name);
    }
    respond(message,{});
  });}};
  const main=(await readFile(new URL("./main.js",import.meta.url),"utf8")).replace(/^import .*;\n/gm,"");
  const dom=(await readFile(new URL("./dom.js",import.meta.url),"utf8")).replace(/^export /gm,"");
  runInNewContext("const $=(()=>{"+dom+";return element;})();\n"+main,{LiveViewOwnership,replyToTeardown,AppProtocolError,provesNonAdmission,parent,document,structuredClone,
    resourceJson:value=>value,resourceValue:(_uri,value)=>value,structuredResult:value=>value,toolValue:(_name,value)=>value,
    crypto:{randomUUID:()=>"viewer-owned"},addEventListener:(name,callback)=>listeners.set(name,callback),
    ResizeObserver:class {observe(){}},requestAnimationFrame(){},setTimeout(callback,delay){timers.set(++timerId,{callback,delay});return timerId;},clearTimeout(id){timers.delete(id);},
    H264DecoderAdmission:class {close(){}},WebSocket:class {constructor(){sockets.push(this);}close(){}},URL});
  const settle=async()=>{for(let i=0;i<5;i++)await turn();};await settle();
  return {Element,listeners,nodes,messages,opens,closes,sockets,timers,respond,parent,settle,
    followRequest:()=>followRequest,closeRequest:()=>closeRequest,
    refuse:request=>listeners.get("message")({source:parent,data:{jsonrpc:"2.0",id:request.id,error:{code:-32603,message:"fixture transport unavailable"}}}),
    update:()=>listeners.get("message")({source:parent,data:{jsonrpc:"2.0",method:"ui/notifications/resource-updated",params:{uri:"uav-sim://session/session-owned/live-cameras"}}}),
    select:(cameraId,checked)=>{const input=nodes.get("choices").children[cameraId==="follow"?0:1].children[0];input.checked=checked;input.onchange();},
    fire:()=>{const [id,timer]=[...timers][0]??[];assert.ok(timer,"expected an owned timer");timers.delete(id);timer.callback();},
    teardown:id=>listeners.get("message")({source:parent,data:{jsonrpc:"2.0",id,method:"ui/resource-teardown"}})};
}

test("actual strict DOM permits deselecting an unmounted camera while follow acquisition is pending",async()=>{
  const app=await mountedRecoveryApp();
  const {Element,listeners,nodes,messages,opens,closes,respond,parent,settle}=app;
  assert.deepEqual(opens,["follow"]);assert.ok(nodes.has("view-follow"));assert.equal(nodes.has("view-formation"),false);
  const formation=nodes.get("choices").children[1].children[0];formation.checked=false;formation.onchange();await settle();
  assert.equal(nodes.get("error").textContent,"");
  const stream={sessionId:"session-owned",cameraId:"follow",liveViewId:"follow-authority",streamProductId:"camera-atlas",widthPx:1280,heightPx:720,codedWidthPx:3840,codedHeightPx:1440,frameRateMillihertz:16000,sourceRegion:{xPx:0,yPx:0,widthPx:1280,heightPx:720},endpoint:{streamUrl:"wss://example.invalid/stream"},resourceUri:"uav-sim://session/session-owned/live-view/follow-authority"};
  respond(app.followRequest(),{stream,accessToken:"fixture-not-a-credential"});await settle();
  assert.deepEqual(opens,["follow"]);assert.equal(nodes.get("error").textContent,"");
  assert.equal(nodes.get("view-follow").dataset.liveViewId,"follow-authority");
  listeners.get("message")({source:parent,data:{jsonrpc:"2.0",id:900,method:"ui/resource-teardown"}});await settle();
  assert.deepEqual(closes,["follow-authority"]);
  assert.equal(messages.some(message=>message.id===900&&message.result),false);
  // An old close completing after a replacement mounts must remove only its captured section.
  const replacement=new Element("section");replacement.id="view-follow";nodes.set(replacement.id,replacement);
  respond(app.closeRequest(),{closed:true,resourceUri:"uav-sim://session/session-owned/live-view/follow-authority"});await settle();
  assert.equal(nodes.get("view-follow"),replacement);
  assert.ok(messages.some(message=>message.id===900&&message.result));
  assert.equal(nodes.get("error").textContent,"");
});

function followConnection(id){return {stream:{sessionId:"session-owned",cameraId:"follow",liveViewId:id,streamProductId:"camera-atlas",widthPx:1280,heightPx:720,codedWidthPx:3840,codedHeightPx:1440,frameRateMillihertz:16000,sourceRegion:{xPx:0,yPx:0,widthPx:1280,heightPx:720},endpoint:{streamUrl:"wss://example.invalid/stream"},resourceUri:`uav-sim://session/session-owned/live-view/${id}`,expiresAt:new Date(Date.now()+60000).toISOString()},accessToken:"fixture-not-a-credential"};}
async function admittedRecoveryApp(){const app=await mountedRecoveryApp();app.select("formation",false);await app.settle();app.respond(app.followRequest(),followConnection("original-authority"));await app.settle();return app;}
function acknowledgeClose(app){const request=app.closeRequest();app.respond(request,{closed:true,resourceUri:`uav-sim://session/session-owned/live-view/${request.params.arguments.liveViewId}`});}

test("actual App coalesces failed known close and only reopens after exact ACK, including unknown renewal",async()=>{
  for(const trigger of ["socket","renew"]){
    const app=await admittedRecoveryApp();
    if(trigger==="renew"){
      app.fire();await app.settle();
      const renewal=app.messages.findLast(message=>message.params?.name==="renew_live_view");assert.ok(renewal);app.refuse(renewal);
    }else{app.sockets[0].onclose();app.sockets[0].onclose();}
    await app.settle();assert.deepEqual(app.closes,["original-authority"]);
    const first=app.closeRequest();app.update();app.update();await app.settle();assert.equal(app.closes.length,1);
    app.refuse(first);await app.settle();assert.equal(app.opens.length,1);
    app.fire();await app.settle();assert.deepEqual(app.closes,["original-authority","original-authority"]);
    assert.deepEqual(app.closeRequest().params.arguments,first.params.arguments);
    app.update();app.update();await app.settle();assert.equal(app.closes.length,2);assert.equal(app.opens.length,1);
    acknowledgeClose(app);await app.settle();app.fire();await app.settle();assert.equal(app.opens.length,2);
    app.respond(app.followRequest(),followConnection("replacement-authority"));await app.settle();
    assert.equal(app.nodes.get("view-follow").dataset.liveViewId,"replacement-authority");
    app.teardown(901);await app.settle();acknowledgeClose(app);await app.settle();
    assert.deepEqual(app.closes,["original-authority","original-authority","replacement-authority"]);
    assert.ok(app.messages.some(message=>message.id===901&&message.result));assert.equal(app.timers.size,0);
  }
});

test("actual App deselect and teardown cancel reopening while retained close can still settle",async()=>{
  for(const action of ["deselect","teardown"]){
    const app=await admittedRecoveryApp();app.sockets[0].onclose();await app.settle();app.refuse(app.closeRequest());await app.settle();
    if(action==="deselect")app.select("follow",false);else app.teardown(902);
    await app.settle();assert.equal(app.opens.length,1);assert.equal(app.closes.length,2);
    acknowledgeClose(app);await app.settle();
    if(action==="deselect"){app.update();await app.settle();}else assert.ok(app.messages.some(message=>message.id===902&&message.result));
    assert.equal(app.opens.length,1);assert.equal(app.timers.size,0);
  }
});

test("actual App unknown replacement Open stays blocked after known retirement settled",async()=>{
  const app=await admittedRecoveryApp();app.sockets[0].onclose();await app.settle();acknowledgeClose(app);await app.settle();app.fire();await app.settle();
  assert.equal(app.opens.length,2);app.refuse(app.followRequest());await app.settle();
  app.update();await app.settle();app.select("follow",false);await app.settle();app.select("follow",true);await app.settle();
  assert.equal(app.opens.length,2);assert.equal(app.timers.size,0);
  app.teardown(903);await app.settle();assert.ok(app.messages.some(message=>message.id===903&&message.error));
});

test("actual App retains a failed redundant atlas authorization before any replacement Open",async()=>{
  const app=await mountedRecoveryApp();
  app.respond(app.followRequest(),followConnection("follow-owned"));await app.settle();
  assert.deepEqual(app.opens,["follow","formation"]);
  const formation=followConnection("formation-redundant");formation.stream.cameraId="formation";
  app.respond(app.followRequest(),formation);await app.settle();
  assert.deepEqual(app.closes,["formation-redundant"]);
  const failed=app.closeRequest();app.refuse(failed);await app.settle();
  assert.equal(app.opens.length,2);
  app.fire();await app.settle();assert.deepEqual(app.closes,["formation-redundant","formation-redundant"]);
  assert.deepEqual(app.closeRequest().params.arguments,failed.params.arguments);
  app.update();app.update();await app.settle();assert.equal(app.closes.length,2);assert.equal(app.opens.length,2);
  acknowledgeClose(app);await app.settle();
  // Coherent product retirement also fences its original shared authority.
  assert.deepEqual(app.closes,["formation-redundant","formation-redundant","follow-owned"]);
  assert.equal(app.opens.length,2);
  acknowledgeClose(app);await app.settle();
  app.fire();await app.settle();app.fire();await app.settle();
  assert.deepEqual(app.opens,["follow","formation","follow","formation"]);
});

test("actual App resumes a newly selected camera after final retirement without another health update",async()=>{
  const app=await admittedRecoveryApp();app.sockets[0].onclose();await app.settle();
  app.select("follow",false);app.select("formation",true);await app.settle();
  assert.deepEqual(app.opens,["follow"]);assert.deepEqual(app.closes,["original-authority"]);
  acknowledgeClose(app);await app.settle();app.fire();await app.settle();
  assert.deepEqual(app.opens,["follow","formation"]);
  assert.equal(app.timers.size,0);
});

test("actual App retirement also owns redundant closes already pending when the socket fails",async()=>{
  const app=await mountedRecoveryApp();app.respond(app.followRequest(),followConnection("follow-owned"));await app.settle();
  const formation=followConnection("pending-redundant");formation.stream.cameraId="formation";
  app.respond(app.followRequest(),formation);await app.settle();const redundant=app.closeRequest();
  app.sockets[0].onclose();await app.settle();assert.deepEqual(app.closes,["pending-redundant","follow-owned"]);
  acknowledgeClose(app);await app.settle();assert.equal(app.opens.length,2);
  app.refuse(redundant);await app.settle();assert.equal(app.opens.length,2);
  app.fire();await app.settle();assert.deepEqual(app.closes,["pending-redundant","follow-owned","pending-redundant"]);
  acknowledgeClose(app);await app.settle();app.fire();await app.settle();app.fire();await app.settle();
  assert.deepEqual(app.opens,["follow","formation","follow","formation"]);
});
