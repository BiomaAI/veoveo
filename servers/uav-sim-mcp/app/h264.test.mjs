import test from "node:test";
import assert from "node:assert/strict";
import {accessUnit,H264DecoderAdmission} from "./h264.js";
const au=(level=32,start=[0,0,0,1])=>new Uint8Array([...start,0x67,0x4d,0x40,level,0x80,...start,0x68,0x80,...start,0x65,0x80]);
const delta=new Uint8Array([0,0,1,0x41,0x80]);
const settle=()=>new Promise(resolve=>setImmediate(resolve));
test("Annex B admits actual Main3.2 codec with either start code and refuses malformed SPS",()=>{
 for(const start of [[0,0,1],[0,0,0,1]])assert.deepEqual(accessUnit(au(32,start)),{codec:"avc1.4d4020",key:true,reentrant:true});
 for(const bad of [[0,0,1,0x67],[0,0,0,1,0x67,0x4d,0x40,32],[0,0,1,0xe7,0x4d,0x40,32,0x80],[1,2,3],[0,0,1,0x67,0x4d,0x43,32,0x80]])assert.throws(()=>accessUnit(new Uint8Array(bad)));
 assert.equal(accessUnit(delta).reentrant,false);
});
test("async codec admission bounds pending frames and uses reentrant reset on codec change",async()=>{
 const pending=[],decoded=[],configured=[],failures=[];let resets=0;
 const gate=new H264DecoderAdmission({configure:codec=>new Promise(resolve=>pending.push(()=>resolve(()=>configured.push(codec)))),reset:()=>resets++,decode:(bytes,timestamp,key)=>decoded.push({bytes,timestamp,key}),failure:error=>failures.push(error)});
 gate.push(delta,0);assert.equal(pending.length,0);
 const first=au();gate.push(first,1);for(let i=0;i<100;i++)gate.push(delta,i+2);
 const changed=au(40);gate.push(changed,103);assert.equal(pending.length,1);
 pending.shift()();await settle();assert.deepEqual(configured,[]);assert.equal(pending.length,1);
 pending.shift()();await settle();assert.deepEqual(configured,["avc1.4d4028"]);assert.equal(decoded.length,1);assert.equal(decoded[0].bytes,changed);assert.equal(decoded[0].timestamp,103);
 gate.push(delta,104);assert.equal(decoded.length,2);assert.equal(failures.length,0);assert.ok(resets>=2);
 gate.close();gate.push(first,105);assert.equal(decoded.length,2);
});
test("teardown and malformed incoming SPS cannot resurrect an async decoder",async()=>{
 for(const malformed of [false,true]){
  let resolve,commits=0,decodes=0,failures=0;
  const gate=new H264DecoderAdmission({configure:()=>new Promise(done=>resolve=done),reset:()=>{},decode:()=>decodes++,failure:()=>failures++});
  gate.push(au(),1);
  if(malformed)gate.push(new Uint8Array([0,0,1,0x67]),2);else gate.close();
  resolve(()=>commits++);await settle();assert.equal(commits,0);assert.equal(decodes,0);assert.equal(failures,malformed?1:0);
 }
});

test("lost predicted references during setup require a fresh reentrant frame before deltas",async()=>{
 for(const replacePending of [false,true]){
  let resolve;const decoded=[];
  const gate=new H264DecoderAdmission({configure:()=>new Promise(done=>resolve=done),reset:()=>{},decode:(_bytes,timestamp)=>decoded.push(timestamp),failure:cause=>{throw cause;}});
  gate.push(au(),1);gate.push(delta,2);
  if(replacePending){gate.push(au(),3);gate.push(delta,4);}
  resolve(()=>{});await settle();
  const before=decoded.slice();gate.push(delta,5);assert.deepEqual(decoded,before);
  // An IDR without SPS/PPS cannot independently restore the admitted chain.
  gate.push(new Uint8Array([0,0,1,0x65,0x80]),6);assert.deepEqual(decoded,before);
  gate.push(au(),7);gate.push(delta,8);assert.deepEqual(decoded,[...before,7,8]);
  gate.close();
 }
});
