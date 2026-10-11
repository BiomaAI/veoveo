(timeoutMs => new Promise(resolve => {
  const frames = [...document.querySelectorAll('iframe.app-frame')];
  if (frames.length !== 1) return resolve({acknowledged:false, failure:'frameIdentity', errorCode:null});
  const frame = frames[0], source = frame.contentWindow;
  const uri = new URL(frame.src, location.href).searchParams.get('uri');
  if (!source || uri !== 'ui://uav-sim/live.html') return resolve({acknowledged:false, failure:'frameIdentity', errorCode:null});
  const id = `uav-owned-teardown-${crypto.randomUUID()}`;
  let timer;
  const finish = result => {
    clearTimeout(timer);
    window.removeEventListener('message', receive);
    resolve(result);
  };
  const receive = event => {
    const value = event.data;
    if (event.source !== source || !value || value.jsonrpc !== '2.0' || value.id !== id) return;
    if (value.error) return finish({acknowledged:false, failure:'refused', errorCode:Number.isInteger(value.error.code) ? value.error.code : null});
    const result = value.result;
    if (!result || typeof result !== 'object' || Array.isArray(result) || Object.keys(result).length !== 0)
      return finish({acknowledged:false, failure:'invalidResult', errorCode:null});
    finish({acknowledged:true, failure:null, errorCode:null});
  };
  window.addEventListener('message', receive);
  timer = setTimeout(() => finish({acknowledged:false, failure:'deadline', errorCode:null}), timeoutMs);
  source.postMessage({jsonrpc:'2.0', id, method:'ui/resource-teardown', params:{}}, '*');
}))
