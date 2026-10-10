import test from "node:test";
import assert from "node:assert/strict";
import {accessUnit,captureMetadata,H264DecoderAdmission} from "./h264.js";
const encoder=new TextEncoder();
const uuid=[0xaa,0x71,0xe4,0x8f,0x07,0x11,0x5d,0x80,0xa2,0x47,0xcd,0x31,0xca,0x6f,0xa4,0x9c];
let sequence=0;
const metadata=(frame=++sequence,ns=BigInt(frame)*1000000n)=>`{"publish_sim_time_ns":${ns},"timestamp_iso8601":"2026-01-01T12:00:01.500Z","timestamp":1767268801500000000,"frame_num":${frame}}`;
function sei(text=metadata(),id=uuid){
 const payload=[...id,...encoder.encode(text)],size=[];let remaining=payload.length;
 while(remaining>=255){size.push(255);remaining-=255;}size.push(remaining);
 const raw=[5,...size,...payload,128],escaped=[];let zeros=0;
 for(const byte of raw){if(zeros>=2&&byte<=3){escaped.push(3);zeros=0;}escaped.push(byte);zeros=byte===0?zeros+1:0;}
 return [0,0,0,1,6,...escaped];
}
const au=(level=32,start=[0,0,0,1],text)=>new Uint8Array([...start,0x67,0x4d,0x40,level,0x80,...start,0x68,0x80,...sei(text),...start,0x65,0x80]);
const delta=(text)=>new Uint8Array([...sei(text),0,0,1,0x41,0x80]);
const settle=()=>new Promise(resolve=>setImmediate(resolve));
test("Annex B admits actual Main3.2 and exact stock capture integers with either start code",()=>{
 for(const start of [[0,0,1],[0,0,0,1]]){
  const unit=accessUnit(au(32,start,metadata(42,1500000000n)));
  assert.equal(unit.codec,"avc1.4d4020");assert.equal(unit.reentrant,true);
  assert.deepEqual(unit.capture,{simulationNs:1500000000n,frameNumber:42n});
 }
 assert.deepEqual(captureMetadata(encoder.encode(metadata(1,9007199254740993n))),{simulationNs:9007199254740993n,frameNumber:1n});
 for(const bad of [[0,0,1,0x67],[0,0,0,1,0x67,0x4d,0x40,32],[0,0,1,0xe7,0x4d,0x40,32,0x80],[1,2,3]])assert.throws(()=>accessUnit(new Uint8Array(bad)));
});
test("missing, duplicate, malformed and unrelated SEI cannot supply a capture clock",()=>{
 const text=metadata(1,1n);
 for(const bad of [text.replace('"frame_num":1','"frame_num":1,"frame_num":2'),text.replace('"frame_num":1','"frame_num":1.5'),text.replace('"frame_num":1','"frame_num":1e3'),text.replace('"frame_num":1','"extra":1'),text.replace('"timestamp_iso8601":"2026-01-01T12:00:01.500Z"','"timestamp_iso8601":{}')])assert.throws(()=>captureMetadata(encoder.encode(bad)));
 for(const bytes of [new Uint8Array([0,0,1,0x41,0x80]),new Uint8Array([...sei(text,uuid.map(()=>0)),0,0,1,0x41,0x80]),new Uint8Array([...sei(text),...sei(text),0,0,1,0x41,0x80]),new Uint8Array([0,0,1,6,5,255,0x80]),new Uint8Array([0,0,1,6,0,3,0,0,3]),new Uint8Array([...sei(text)])])assert.throws(()=>accessUnit(bytes));
 // Unknown SEI remains harmless when the same picture contains exactly one stock record.
 assert.equal(accessUnit(new Uint8Array([...sei(text,uuid.map(()=>0)),...delta(text)])).capture.frameNumber,1n);
 const escaped=sei(text,[0,0,1,...uuid.slice(3)]);assert.ok(escaped.some((value,i)=>value===3&&escaped[i-1]===0&&escaped[i-2]===0));
 assert.equal(accessUnit(new Uint8Array([...escaped,...delta(text)])).capture.frameNumber,1n);
});
test("async codec admission bounds pending frames and uses reentrant reset on codec change",async()=>{
 const pending=[],decoded=[],configured=[],failures=[];let resets=0;
 const gate=new H264DecoderAdmission({configure:codec=>new Promise(resolve=>pending.push(()=>resolve(()=>configured.push(codec)))),reset:()=>resets++,decode:(bytes,timestamp,key)=>decoded.push({bytes,timestamp,key}),failure:error=>failures.push(error)});
 gate.push(delta());assert.equal(pending.length,0);
 gate.push(au());for(let i=0;i<100;i++)gate.push(delta());
 const changed=au(40);gate.push(changed);assert.equal(pending.length,1);
 pending.shift()();await settle();assert.deepEqual(configured,[]);assert.equal(pending.length,1);
 pending.shift()();await settle();assert.deepEqual(configured,["avc1.4d4028"]);assert.equal(decoded.length,1);assert.equal(decoded[0].bytes,changed);assert.equal(decoded[0].timestamp,Number(accessUnit(changed).capture.simulationNs/1000n));
 gate.push(delta());assert.equal(decoded.length,2);assert.equal(failures.length,0);assert.ok(resets>=2);
 gate.close();gate.push(au());assert.equal(decoded.length,2);
});
test("teardown and malformed incoming SPS cannot resurrect an async decoder",async()=>{
 for(const malformed of [false,true]){
  let resolve,commits=0,decodes=0,failures=0;
  const gate=new H264DecoderAdmission({configure:()=>new Promise(done=>resolve=done),reset:()=>{},decode:()=>decodes++,failure:()=>failures++});
  gate.push(au());if(malformed)gate.push(new Uint8Array([0,0,1,0x67]));else gate.close();
  resolve(()=>commits++);await settle();assert.equal(commits,0);assert.equal(decodes,0);assert.equal(failures,malformed?1:0);
 }
});
test("lost predicted references during setup require a fresh reentrant frame before deltas",async()=>{
 for(const replacePending of [false,true]){
  let resolve;const decoded=[];
  const gate=new H264DecoderAdmission({configure:()=>new Promise(done=>resolve=done),reset:()=>{},decode:(bytes,timestamp)=>decoded.push({bytes,timestamp}),failure:cause=>{throw cause;}});
  gate.push(au());gate.push(delta());if(replacePending){gate.push(au());gate.push(delta());}
  resolve(()=>{});await settle();const before=decoded.length;gate.push(delta());assert.equal(decoded.length,before);
  gate.push(new Uint8Array([...sei(),0,0,1,0x65,0x80]));assert.equal(decoded.length,before);
  const fresh=au();gate.push(fresh);gate.push(delta());assert.equal(decoded.length,before+2);assert.equal(decoded[before].timestamp,Number(accessUnit(fresh).capture.simulationNs/1000n));gate.close();
 }
});
test("equal simulation time preserves references; backwards or repeated frame counter closes epoch",async()=>{
 const decoded=[],failures=[];
 const make=()=>new H264DecoderAdmission({configure:async()=>()=>{},reset:()=>{},decode:(_bytes,time)=>decoded.push(time),failure:cause=>failures.push(cause.message)});
 let gate=make();gate.push(au(32,undefined,metadata(1,123456789n)));await settle();gate.push(delta(metadata(2,123456789n)));assert.deepEqual(decoded,[123456,123456]);
 gate.push(delta(metadata(3,123455000n)));assert.equal(gate.closed,true);assert.equal(failures.length,1);
 gate=make();gate.push(au(32,undefined,metadata(1,0n)));await settle();gate.push(delta(metadata(1,1n)));assert.equal(gate.closed,true);
 // A genuinely new decoder/connection admits a new source epoch, without offsets.
 gate=make();gate.push(au(32,undefined,metadata(1,0n)));await settle();assert.equal(decoded.at(-1),0);gate.close();
});
