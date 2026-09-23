import { useState } from "react";
import type { OperationImage } from "./generated/workspace.ts";

export function ResultImages({ images, omitted }: { images: OperationImage[]; omitted: number }) {
  return <>{images.map((image, index) => <ResultImage key={index} image={image} index={index}/>)}
    {omitted > 0 && <p className="muted">{omitted} image {omitted === 1 ? "result is" : "results are"} unavailable here. Activity displays PNG, JPEG and WebP within its image size limit.</p>}
  </>;
}

function ResultImage({ image, index }: { image: OperationImage; index: number }) {
  const [failed, setFailed] = useState<string>();
  const source = `data:${image.mimeType};base64,${image.data}`;
  return failed === source ? <p className="muted">Image result {index + 1} couldn't be displayed.</p>
    : <img className="task-image" src={source} alt={`Image result ${index + 1}`} loading="lazy" onError={() => setFailed(source)}/>;
}
