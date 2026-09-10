import { useState } from "react";

export function CliConnect({ computerId, canConnect }: { computerId: string; canConnect: boolean }) {
  const [message, setMessage] = useState<string>();
  const name = `computer-${computerId}`;
  const endpoint = `${window.location.origin}/console/computers/${computerId}`;
  const registration = `openshell gateway add ${endpoint} --name ${name}`;
  const connection = `openshell --gateway ${name} sandbox connect ${computerId}`;
  async function copy(value: string) {
    try { await navigator.clipboard.writeText(value); setMessage("Command copied."); }
    catch { setMessage("Select and copy the command below."); }
  }
  return <details className="computer-cli-connect">
    <summary>Connect from your terminal</summary>
    <p>Use OpenShell CLI 0.0.116. Register this Computer and confirm the code in your browser.</p>
    <pre><code>{registration}</code></pre>
    <button className="button button-secondary" onClick={() => void copy(registration)}>Copy registration command</button>
    <p>After pairing, open a shell:</p>
    <pre><code>{connection}</code></pre>
    <button className="button button-secondary" onClick={() => void copy(connection)}>Copy connection command</button>
    {!canConnect && <p>Start the Computer and confirm your access before connecting.</p>}
    <p>Reconnect using the same command. After logout or access expiry, sign in again with <code>openshell --gateway {name} gateway login</code>.</p>
    {message && <p role="status">{message}</p>}
  </details>;
}
