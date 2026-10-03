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
import {App} from '/src/App.tsx';
import {demoSnapshot} from '/src/demo.ts';
import {QueryClient,QueryClientProvider} from '@tanstack/react-query';
import {ThemeContext} from '/src/theme.ts';
import {initializeAppSession,loadSnapshot} from '/src/api.ts';
import {configureBrowserApplication} from '/src/browserApp.ts';
configureBrowserApplication('console');
initializeAppSession('fixture');
window.failSnapshot=()=>loadSnapshot().catch(error=>error.message);
const root=createRoot(document.querySelector('#root'));
const client=new QueryClient({defaultOptions:{queries:{retry:false}}});
const app={server:'map',resourceUri:'ui://map/workspace.html',standalonePath:'/apps/map/workspace',name:'Map Explorer',tools:[{name:'query_features',inputSchema:{}}],resourceDependencies:[],toolDependencies:[],agentMessageTargets:[]};
window.renderFrame=(theme='light',title='Map Explorer',visible=true)=>root.render(React.createElement(ThemeContext.Provider,{value:{theme,appTheme:theme,setTheme:()=>{}}},visible?React.createElement(AppFrame,{app:{...app,title},onInternalLink:()=>false}):null));
window.renderCatalog=async(stage)=>{
 const catalog=await fetch('/console/fixture-catalog?stage='+stage,{method:'POST'});
 if(!catalog.ok)throw new Error('catalog fixture failed');
 if(stage==='initial') {
   window.history.replaceState(null,'','#/apps/map/workspace');
   client.setQueryData(['console-session'],{profile:'operator',canReadInstallation:false,canReadAudit:false,installation:demoSnapshot.installation,session:demoSnapshot.session});
   root.render(React.createElement(ThemeContext.Provider,{value:{theme:'light',appTheme:'light',setTheme:()=>{}}},React.createElement(QueryClientProvider,{client},React.createElement(App))));
 }
};
window.renderFrame();`;

test('Console navigation retains live frames through catalog loss and clears them on removal or navigation', {timeout: 60_000}, async () => {
  let frames = 0, opened = 0, unsubscribed = 0;
  let release, callArrived;
  const requested = new Promise(resolve => {callArrived = resolve;});
  const streams = new Set(), catalogs = new Set();
  const app = {server:'map',resourceUri:'ui://map/workspace.html',standalonePath:'/apps/map/workspace',name:'Map Explorer',tools:[{name:'query_features',inputSchema:{}}],resourceDependencies:[],toolDependencies:[],agentMessageTargets:[]};
  let catalog = {apps:[app],degradations:[]};
  const sendCatalog = res => res.write(`event: catalog\ndata: ${JSON.stringify(catalog)}\n\n`);
  const server = await createServer({root:fileURLToPath(new URL('../',import.meta.url)),configFile:false,base:'/console/',
    optimizeDeps:{include:['react','react-dom/client']},
    server:{host:'127.0.0.1',port:0},plugins:[react(),{name:'fixtures',
      resolveId(id){if(id.endsWith('/fixture-entry.js'))return '\0app-frame-fixture';},
      load(id){if(id==='\0app-frame-fixture')return entry;},
      configureServer(server){
      server.middlewares.use(async (req,res,next)=>{
        const path=new URL(req.url,'http://fixture.test').pathname;
        if(path==='/console/fixture') {res.setHeader('Content-Type','text/html');res.end(await server.transformIndexHtml('/console/fixture','<!doctype html><div id="root"></div><script type="module" src="/console/fixture-entry.js"></script>'));}
        else if(path==='/console/fixture-catalog') {
          const stage=new URL(req.url,'http://fixture.test').searchParams.get('stage');
          const pending=stage==='restarting', partial=stage==='tools-pending', removed=stage==='removed';
          catalog={apps:pending||removed?[]:[partial?{...app,tools:[]}:app],degradations:pending?[{server:'map',surface:'resources',code:'upstream_unavailable'}]:partial||removed?[{server:'map',surface:'tools',code:'discovery_pending'}]:[]};
          for(const stream of catalogs)sendCatalog(stream);
          res.end('ok');
        }
        else if(path==='/console/api/apps'){res.setHeader('Content-Type','application/json');res.end(JSON.stringify(catalog));}
        else if(path==='/console/api/apps/events'){res.setHeader('Content-Type','text/event-stream');catalogs.add(res);sendCatalog(res);res.on('close',()=>catalogs.delete(res));}
        else if(path==='/console/api/snapshot'){res.statusCode=503;res.setHeader('Content-Type','application/json');res.end('{"error":"fixture unavailable"}');}
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
    assert.match(await page.evaluate(()=>window.failSnapshot()), /couldn't load the Console/);
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
    await page.evaluate(()=>window.renderCatalog('initial'));
    await app.getByText('ready',{exact:true}).waitFor();
    assert.equal(frames,4);
    await app.locator('output').evaluate(node=>{node.textContent='retained selection';});
    for(const stage of ['restarting','tools-pending','recovered']) {
      await page.evaluate(stage=>window.renderCatalog(stage),stage);
      if(stage==='restarting')await page.locator('.nav-app-unavailable').filter({hasText:'Unavailable'}).waitFor();
      else await page.getByRole('button',{name:'Map Explorer',exact:true}).waitFor();
      await app.getByText('retained selection',{exact:true}).waitFor();
      assert.equal(frames,4,stage);
    }
    await page.getByRole('button',{name:'Apps',exact:true}).click();
    await page.locator('iframe').waitFor({state:'detached'});
    await page.locator('.nav-app').filter({hasText:'Map Explorer'}).click();
    await app.getByText('ready',{exact:true}).waitFor();
    assert.equal(frames,5);
    // A successful resource list that omits this App revokes its frame even
    // when tools from the same server are still discovering.
    await page.evaluate(()=>window.renderCatalog('removed'));
    await page.locator('iframe').waitFor({state:'detached'});
  }finally{await context.close();await browser.close();for(const res of [...streams,...catalogs])res.end();await server.close();}
});
