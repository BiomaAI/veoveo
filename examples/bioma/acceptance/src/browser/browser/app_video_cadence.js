(async()=>{
  const canvas=document.querySelector(".view canvas");
  if(!canvas)throw new Error("shared H.264 video canvas is unavailable");
  const waitForFrame=(deadline)=>new Promise((resolve,reject)=>{
    const before=Number(canvas.dataset.decodedFrames??0),remaining=deadline-Date.now();
    if(remaining<=0){reject(new Error("reactive video cadence sample did not complete"));return;}
    const observer=new MutationObserver(()=>{
      const decoded=Number(canvas.dataset.decodedFrames??0);
      if(decoded<=before)return;
      clearTimeout(timeout);observer.disconnect();resolve({now:performance.now(),decoded,mediaTime:Number(canvas.dataset.mediaTimeSeconds??0),receiveToDisplayMs:Number(canvas.dataset.receiveToDisplayMs??0)});
    });
    const timeout=setTimeout(()=>{observer.disconnect();reject(new Error("reactive video cadence sample did not complete"));},remaining);
    observer.observe(canvas,{attributes:true,attributeFilter:["data-decoded-frames"]});
  });
  const warmupFrames=12,sampleIntervals=48,deadline=Date.now()+15000;
  const p95=values=>{const ordered=[...values].sort((a,b)=>a-b);return ordered.length?ordered[Math.max(0,Math.ceil(ordered.length*0.95)-1)]:-1;};
  for(let index=0;index<warmupFrames;index++)await waitForFrame(deadline);
  const first={now:performance.now(),mediaTime:Number(canvas.dataset.mediaTimeSeconds??0),decoded:Number(canvas.dataset.decodedFrames??0),dropped:Number(canvas.dataset.droppedFrames??0)};
  const delivery=[];let last=null;
  for(let index=0;index<sampleIntervals;index++){last=await waitForFrame(deadline);delivery.push(last.receiveToDisplayMs);}
  const intervalSeconds=(last.now-first.now)/1000,receiveToDisplayP95Ms=p95(delivery);
  return {
    intervalSeconds,
    mediaTimeSeconds:last.mediaTime-first.mediaTime,
    presentedFrameIntervals:sampleIntervals,
    decodedVideoFrames:last.decoded-first.decoded,
    droppedVideoFrames:Math.max(0,Number(canvas.dataset.droppedFrames??0)-first.dropped),
    deliverySamples:delivery.length,
    receiveToDisplayP95Ms
  };
})()
