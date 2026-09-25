// Behavioral acceptance: real React/iframe lifecycle, no graphics assertions.
import test from 'node:test';
import assert from 'node:assert/strict';
import {fileURLToPath} from 'node:url';
import {chromium} from 'playwright';
import {createServer} from 'vite';
import react from '@vitejs/plugin-react';

const frame = `<!doctype html><button>Query</button><output>starting</output><script>
let next=1; const pending=new Map();
const call=(method,params)=>new Promise(resolve=>{const id=next++;pending.set(id,resolve);parent.postMessage({jsonrpc:'2.0',id,method,params},'*');});
addEventListener('message',e=>{if(e.source!==parent)return;const m=e.data;
 if(m.method==='ui/notifications/host-context-changed')document.body.dataset.theme=m.params.theme;
 if(m.id!==undefined&&m.result!==undefined){pending.get(m.id)?.(m.result);pending.delete(m.id);}});
call('ui/initialize',{}).then(()=>{document.querySelector('output').textContent='ready';call('subscriptions/listen',{notifications:{resourceSubscriptions:['map://feature-layers']}});});
document.querySelector('button').onclick=async()=>{document.querySelector('output').textContent='waiting';await call('tools/call',{name:'query_features',arguments:{}});document.querySelector('output').textContent='complete';};
</script>`;

const entry = `import React from 'react';
import {createRoot} from 'react-dom/client';
import {AppFrame} from '/src/apps/AppFrame.tsx';
import {ThemeContext} from '/src/theme.ts';
import {initializeAppSession} from '/src/api.ts';
initializeAppSession('fixture');
const root=createRoot(document.querySelector('#root'));
const app={server:'map',resourceUri:'ui://map/workspace.html',standalonePath:'/apps/map/workspace',name:'Map Explorer',tools:[{name:'query_features',inputSchema:{}}],resourceDependencies:[],toolDependencies:[],agentMessageTargets:[]};
window.renderFrame=(theme='light',title='Map Explorer',visible=true)=>root.render(React.createElement(ThemeContext.Provider,{value:{theme,appTheme:theme,setTheme:()=>{}}},visible?React.createElement(AppFrame,{app:{...app,title},onInternalLink:()=>false}):null));
window.renderFrame();`;

test('catalog identity churn and theme changes preserve pending calls; descriptor changes and remount get fresh frames', {timeout: 60_000}, async () => {
  let frames = 0, opened = 0, unsubscribed = 0;
  let release, callArrived;
  const requested = new Promise(resolve => {callArrived = resolve;});
  const streams = new Set();
  const server = await createServer({root:fileURLToPath(new URL('../',import.meta.url)),configFile:false,base:'/console/',
    optimizeDeps:{include:['react','react-dom/client']},
    server:{host:'127.0.0.1',port:0},plugins:[react(),{name:'fixtures',
      resolveId(id){if(id.endsWith('/fixture-entry.js'))return '\0app-frame-fixture';},
      load(id){if(id==='\0app-frame-fixture')return entry;},
      configureServer(server){
      server.middlewares.use(async (req,res,next)=>{
        const path=new URL(req.url,'http://fixture.test').pathname;
        if(path==='/console/fixture') {res.setHeader('Content-Type','text/html');res.end(await server.transformIndexHtml('/console/fixture','<!doctype html><div id="root"></div><script type="module" src="/console/fixture-entry.js"></script>'));}
        else if(path==='/console/api/apps/frame'){++frames;res.setHeader('Content-Type','text/html');res.end(frame);}
        else if(path==='/console/api/apps/call'){release=()=>{res.setHeader('Content-Type','application/json');res.end('{"content":[]}');};callArrived();}
        else if(path==='/console/api/apps/resource-events'){++opened;res.setHeader('Content-Type','text/event-stream');res.write(': connected\n\n');streams.add(res);res.on('close',()=>streams.delete(res));}
        else if(path==='/console/api/apps/resource-unsubscribe'){++unsubscribed;res.setHeader('Content-Type','application/json');res.end('{}');}
        else next();
      });
    }}]});
  const browser=await chromium.launch({channel:'chrome',headless:true});
  const context=await browser.newContext();context.setDefaultTimeout(10_000);
  try {
    await server.listen();
    const page=await context.newPage();
    await page.goto(`http://127.0.0.1:${server.httpServer.address().port}/console/fixture`);
    const app=page.frameLocator('iframe');
    await app.getByText('ready',{exact:true}).waitFor();
    await app.getByRole('button',{name:'Query'}).click();
    await app.getByText('waiting',{exact:true}).waitFor();
    await requested;
    assert.ok(release);
    await page.evaluate(()=>{window.renderFrame('dark');});
    await app.locator('body[data-theme="dark"]').waitFor();
    release();
    await app.getByText('complete',{exact:true}).waitFor();
    assert.equal(frames,1); assert.equal(opened,1);assert.equal(unsubscribed,0);
    await page.evaluate(()=>window.renderFrame('dark','Changed title'));
    await page.locator('iframe[title="Changed title"]').waitFor();
    await app.getByText('ready',{exact:true}).waitFor();
    assert.equal(frames,2);
    await page.evaluate(()=>window.renderFrame('dark','Changed title',false));
    await page.locator('iframe').waitFor({state:'detached'});
    await page.evaluate(()=>window.renderFrame());
    await app.getByText('ready',{exact:true}).waitFor();
    assert.equal(frames,3);
  }finally{await context.close();await browser.close();for(const res of streams)res.end();await server.close();}
});
