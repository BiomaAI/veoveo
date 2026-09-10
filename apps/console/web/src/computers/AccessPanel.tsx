import { useEffect } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { computerError, readAccessGrants, revokeAccess } from "./api";
import type { ComputerSnapshot } from "../generated/computers";

export function AccessPanel({ computerId, snapshot, stale }: {
  computerId: string;
  snapshot: ComputerSnapshot;
  stale: boolean;
}) {
  const client = useQueryClient();
  const inventory = useQuery({
    queryKey: ["computers", "access", computerId],
    queryFn: ({ signal }) => readAccessGrants(computerId, signal),
    enabled: !stale,
    retry: false,
    refetchOnWindowFocus: false,
  });
  useEffect(() => {
    void client.invalidateQueries({ queryKey: ["computers", "access", computerId] });
  }, [client, computerId, snapshot]);
  const revoke = useMutation({
    mutationFn: (grantId: string) => revokeAccess(computerId, grantId),
    onSuccess: () => client.invalidateQueries({ queryKey: ["computers", "access", computerId] }),
    retry: false,
  });
  return <section className="computer-access" aria-label="Computer access">
    <div className="computers-toolbar">
      <h4>Browser access</h4>
      <button className="button button-secondary" disabled={inventory.isFetching}
        onClick={() => void inventory.refetch()}>Refresh access</button>
    </div>
    <p>Revoke access to disconnect its browser. Your Computer keeps running.</p>
    {(inventory.error || revoke.error) && <p role="alert" className="computers-error">
      {computerError(revoke.error ?? inventory.error)}
    </p>}
    {revoke.isSuccess && <p role="status">Access revoked. Its attachment is closing.</p>}
    {inventory.isPending && <p>Loading access grants…</p>}
    {inventory.data?.grants.length === 0 && <p>No outstanding browser access.</p>}
    {inventory.data?.grants.map(grant => <div className="computer-operation" key={grant.grantId}>
      <div>
        <strong>{grant.currentSession ? "This sign-in" : "Another sign-in"}</strong>
        <p>{grant.redeemed ? "Browser access issued" : "Waiting for attachment"}</p>
        <small>Issued {new Date(grant.issuedAt).toLocaleString()} · Expires by {new Date(grant.expiresAt).toLocaleString()}</small>
      </div>
      <button className="button button-secondary" disabled={revoke.isPending}
        onClick={() => revoke.mutate(grant.grantId)}>Revoke access</button>
    </div>)}
  </section>;
}
