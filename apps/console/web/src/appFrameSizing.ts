const MAX_FRAME_CONTENT_HEIGHT = 1400;
const MIN_FRAME_CONTENT_HEIGHT = 180;

export function appFrameOuterHeight(
  reportedContentHeight: number,
  nonContentHeight: number,
): number {
  const contentHeight = Math.min(
    MAX_FRAME_CONTENT_HEIGHT,
    Math.max(MIN_FRAME_CONTENT_HEIGHT, reportedContentHeight),
  );
  return contentHeight + Math.max(0, nonContentHeight);
}
