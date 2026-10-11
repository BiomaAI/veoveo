// Exact public lifecycle expression; behavioral evidence, no graphics acceptance.
import assert from 'node:assert/strict';
import test from 'node:test';
import {readFileSync} from 'node:fs';
import {runInNewContext} from 'node:vm';
const script=readFileSync(new URL('../src/browser/browser/app_teardown.js',import.meta.url),'utf8');
async function teardown(mode){
  let listener, timer, removed=false, sent;
  const source={postMessage(message){
    sent=message;
    queueMicrotask(()=>{
      const response={jsonrpc:'2.0',id:message.id,result:{}};
      if(mode==='refuse'){delete response.result;response.error={code:-32000,message:'private upstream detail'};}
      if(mode==='invalid')response.result={closed:true};
      listener({source:mode==='foreign'?{}:source,data:response});
      if(mode==='foreign')timer();
    });
  }};
  const context={document:{querySelectorAll:()=>[{src:'https://fixture.invalid/console/api/apps/frame?uri=ui%3A%2F%2Fuav-sim%2Flive.html',contentWindow:source}],getElementById:()=>({textContent:'Camera recovery is waiting for simulator readiness'})},location:{href:'https://fixture.invalid/apps/uav-sim/live'},URL,crypto:{randomUUID:()=> 'owned-id'},
    window:{addEventListener:(_name,fn)=>{listener=fn;},removeEventListener:()=>{removed=true;}},
    setTimeout:fn=>{timer=fn;return 1;},clearTimeout:()=>{}};
  const result=await runInNewContext(`(${script})(20)`,context);
  assert.equal(sent.method,'ui/resource-teardown');assert.equal(removed,true);
  return JSON.parse(JSON.stringify(result));
}
test('acknowledged close succeeds despite stale readiness error',async()=>assert.deepEqual(await teardown('ack'),{acknowledged:true,failure:null,errorCode:null}));
test('actual remote refusal retains safe code and never reports closed',async()=>assert.deepEqual(await teardown('refuse'),{acknowledged:false,failure:'refused',errorCode:-32000}));
test('foreign-frame response cannot settle ownership and original timer expires',async()=>assert.deepEqual(await teardown('foreign'),{acknowledged:false,failure:'deadline',errorCode:null}));
test('nonstandard result is refused rather than inferred as settlement',async()=>assert.deepEqual(await teardown('invalid'),{acknowledged:false,failure:'invalidResult',errorCode:null}));
