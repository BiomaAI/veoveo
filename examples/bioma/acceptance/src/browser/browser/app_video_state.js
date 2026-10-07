(() => {
  const canvas=document.querySelector(".view canvas");
  const view=canvas?.closest(".view");
  const decodedFrames=Number(canvas?.dataset.decodedFrames ?? 0);
  const frame={meanLuma:0,lumaStandardDeviation:0,minimumLuma:0,maximumLuma:0,
    lumaCells:Array.from({length:16},()=>({minimum:0,maximum:0,standardDeviation:0})),pixelSampleError:""};
  try {
    if(!canvas?.width||!canvas?.height) throw new Error("video canvas dimensions are unavailable");
    const sample=document.createElement("canvas");
    sample.width=64;sample.height=36;
    const context=sample.getContext("2d",{willReadFrequently:true});
    if(!context) throw new Error("2D frame sampler is unavailable");
    context.drawImage(canvas,0,0,sample.width,sample.height);
    const pixels=context.getImageData(0,0,sample.width,sample.height).data;
    let sum=0,sumSquares=0,minimum=255,maximum=0,count=0;
    const cells=Array.from({length:16},()=>({sum:0,squares:0,minimum:255,maximum:0,count:0}));
    for(let index=0;index<pixels.length;index+=4){
      const luma=0.2126*pixels[index]+0.7152*pixels[index+1]+0.0722*pixels[index+2];
      sum+=luma;sumSquares+=luma*luma;minimum=Math.min(minimum,luma);maximum=Math.max(maximum,luma);count++;
      const pixel=index/4,x=pixel%sample.width,y=Math.floor(pixel/sample.width);
      const cell=cells[Math.floor(y/(sample.height/4))*4+Math.floor(x/(sample.width/4))];
      cell.sum+=luma;cell.squares+=luma*luma;
      cell.minimum=Math.min(cell.minimum,luma);cell.maximum=Math.max(cell.maximum,luma);cell.count++;
    }
    frame.meanLuma=sum/count;
    frame.lumaStandardDeviation=Math.sqrt(Math.max(0,sumSquares/count-frame.meanLuma*frame.meanLuma));
    frame.minimumLuma=Math.round(minimum);
    frame.maximumLuma=Math.round(maximum);
    frame.lumaCells=cells.map(cell=>({
      minimum:Math.round(cell.minimum),maximum:Math.round(cell.maximum),
      standardDeviation:Math.sqrt(Math.max(0,cell.squares/cell.count-(cell.sum/cell.count)**2))
    }));
  }catch(error){frame.pixelSampleError=String(error?.message||error);}
  return {
    documentEpochMs:performance.timeOrigin,
    cameraId:view?.id?.startsWith("view-") ? view.id.slice(5) : "",
    viewerInstanceId:view?.dataset.viewerInstanceId ?? "",
    liveViewId:view?.dataset.liveViewId ?? "",
    streamProductId:view?.dataset.streamProductId ?? "",
    readyState:decodedFrames>0 ? 2 : 0,
    videoWidth:canvas?.width ?? 0,
    videoHeight:canvas?.height ?? 0,
    currentTime:Number(canvas?.dataset.mediaTimeSeconds ?? 0),
    sampledAtMs:performance.now(),
    totalVideoFrames:decodedFrames,
    droppedVideoFrames:Number(canvas?.dataset.droppedFrames ?? 0),
    declaredFrameRateHz:Number(canvas?.dataset.frameRate ?? 0),
    observedFrameRateHz:0,
    ...frame,
    decodeLabel:document.getElementById("decode")?.textContent ?? "",
    status:document.getElementById("status")?.textContent ?? "",
    error:document.getElementById("error")?.hidden === false
      ? document.getElementById("error").textContent : "",
    bodyText:document.body?.innerText ?? ""
  };
})()
