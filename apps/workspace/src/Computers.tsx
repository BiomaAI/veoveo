import { ComputersPage } from "../../console/web/src/computers/ComputersPage.tsx";
import type { QueueState } from "../../console/web/src/uploads/queue.ts";
import type { WorkspaceBootstrap } from "./generated/workspace.ts";
import "./computers.css";

export function Computers({ session, uploads, onUpload }: {
  session: WorkspaceBootstrap; uploads: QueueState; onUpload: () => void;
}) {
  const scope = JSON.stringify(["workspace", session.tenantId, session.principalId, session.workContext]);
  return <div className="workspace-computers"><ComputersPage key={scope} scope={scope}
    profile="workspace" canReadInstallation={false}
    principals={[{ id: session.principalId, displayName: session.person.displayName }]}
    artifacts={[]} uploads={uploads} onUpload={onUpload}/></div>;
}
