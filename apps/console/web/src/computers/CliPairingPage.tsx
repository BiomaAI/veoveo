import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { consoleIdentityScope, useConsoleBootstrap } from "../bootstrap";
import { consoleJson } from "../consoleHttp";
import { parseComputer } from "../generatedContracts";
import { pairCli, pairingLocation, PairingFailure } from "./pairing";
import type { PairingLocation } from "./pairing";
import "./computers.css";

export function CliPairingPage() {
  const bootstrap = useConsoleBootstrap();
  const [location] = useState(() => pairingLocation(new URL(window.location.href)));
  return <main className="computer-pairing-page">
    <a href="/console/#/computers">Veoveo · Computers</a>
    <h1>Connect your CLI</h1>
    {!location ? <p role="alert">This pairing link is invalid. Start login from your CLI again.</p>
      : bootstrap.data ? <PairingForm key={consoleIdentityScope(bootstrap.data)} location={location}
        identity={bootstrap.data.session.displayName} scope={consoleIdentityScope(bootstrap.data)} />
        : <p role="status">{bootstrap.error ? "Sign-in could not be confirmed. Reload to try again." : "Checking your sign-in…"}</p>}
  </main>;
}
function PairingForm({ location, identity, scope }: { location: PairingLocation; identity: string; scope: string }) {
  const [name, setName] = useState("My CLI");
  const [matches, setMatches] = useState(false);
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState(false);
  const [failure, setFailure] = useState<string>();
  const computer = useQuery({
    queryKey: ["cli-pairing-computer", scope, location.computerId],
    queryFn: async ({ signal }) => parseComputer("computer", await consoleJson(`computers/${location.computerId}`, undefined, signal)),
    retry: false,
  });
  async function connect(event: React.SubmitEvent<HTMLFormElement>) {
    event.preventDefault();
    if (busy || done || !matches || !computer.data?.canConnect || failure) return;
    setBusy(true);
    try { await pairCli(location, name); setDone(true); }
    catch (error) { setFailure(error instanceof PairingFailure ? error.message : "Pairing could not be completed. Review this Computer’s access before trying again."); }
    finally { setBusy(false); }
  }
  return <>
    <p>Signed in as {identity}.</p>
    <p>Computer {location.computerId.slice(-8)}</p>
    {done ? <div role="status"><h2>CLI connected</h2><p>Return to your terminal. Closing this tab keeps the CLI connected.</p></div> :
      <form onSubmit={event => void connect(event)}>
        <p>Compare this code with the code in the terminal where you started login.</p>
        <output className="computer-pairing-code" aria-label="Confirmation code">{location.code}</output>
        <label className="computer-pairing-check"><input type="checkbox" checked={matches} disabled={busy || !!failure}
          onChange={event => setMatches(event.target.checked)} /> The codes match and I started this login.</label>
        <label className="computer-pairing-name">Name this CLI<input value={name} maxLength={64} required disabled={busy || !!failure} autoComplete="off"
          onChange={event => setName(event.target.value)} /></label>
        <p>This CLI can open a shell on this Computer. Logging out ends its access. You can revoke it from Computer access.</p>
        {computer.isPending && <p role="status">Checking Computer access…</p>}
        {(computer.error || (computer.data && !computer.data.canConnect)) && <p role="alert">This Computer cannot be connected with your current access. Open it in Veoveo to check its status.</p>}
        {failure && <p role="alert" className="computers-error">{failure}</p>}
        <button className="button button-primary" type="submit" disabled={busy || !!failure || !matches || !name.trim() || !computer.data?.canConnect}>
          {busy ? "Connecting CLI…" : "Connect this CLI"}
        </button>
      </form>}
    <p><a href={`/console/#/computers/${location.computerId}`}>Open Computer and review access</a></p>
  </>;
}
