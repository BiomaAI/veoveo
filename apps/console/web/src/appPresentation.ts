import type { AppDescriptor } from "./types";

/** An explicit border opt-out asks the host to dedicate its content area to the App. */
export function isFullBleedApp(
  app: Pick<AppDescriptor, "prefersBorder"> | undefined,
): boolean {
  return app?.prefersBorder === false;
}
