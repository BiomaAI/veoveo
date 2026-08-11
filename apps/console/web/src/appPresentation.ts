import type { AppDescriptor } from "./types";

/** Apps receive the complete content workspace unless they explicitly request host framing. */
export function isFullBleedApp(
  app: Pick<AppDescriptor, "prefersBorder"> | undefined,
): boolean {
  return app !== undefined && app.prefersBorder !== true;
}
