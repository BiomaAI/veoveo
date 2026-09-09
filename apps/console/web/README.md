# Console Development

Run the Console frontend with Vite while its Rust BFF and gateway remain running.
A frontend edit then uses React refresh without building an image or restarting a Pod.

```sh
npm --prefix apps/console/web ci
npm --prefix apps/console/web run dev
```

Open `http://127.0.0.1:4173/console/`. Vite binds only to loopback and fails when
port 4173 is occupied. Its routes preserve the browser's cookies and request headers:

| Browser path | Local service |
|---|---|
| `/console/` and its assets | Vite, port 4173 |
| `/console/api` and `/auth` | Console BFF, port 8786 |
| `/oauth` and `/.well-known` | Gateway, port 8788 |

## Authentication Origin

Use `http://127.0.0.1:4173` as the BFF's `PUBLIC_BASE_URL` and the local gateway's
public origin. Set `VEOVEO_GATEWAY_URL=http://127.0.0.1:8788` for the BFF's internal
requests. Its OAuth resource must identify the local gateway's admitted admin profile;
for that origin it is `http://127.0.0.1:4173/mcp/admin`.

Set `VEOVEO_CONSOLE_MCP_TRANSPORT_URL=http://127.0.0.1:8788/mcp/admin` for the
BFF's MCP connection. The OAuth resource remains the public identity above; the
transport reaches the gateway directly because Vite does not proxy `/mcp`.

The local gateway control plane must register the BFF client's exact
`http://127.0.0.1:4173/auth/callback` redirect. Its configured identity provider must
also admit the gateway's `http://127.0.0.1:4173/oauth/callback`. Keep client identifiers,
scopes, issuer and resource identities consistent with that control plane. Supply the
BFF's 32-byte base64 session key through the normal private environment. Start the BFF
with `cargo run -p veoveo-console-bff --bin console-bff` after loading its environment.

A port-forward to a BFF configured for another public origin does not configure local
login: its callback still returns to the registered origin. Use a BFF and gateway
configured for this development origin. Vite leaves redirects and cookie policy intact.

## Verification And Publication

`npm --prefix apps/console/web run build` checks TypeScript and builds production
assets. Visual verification additionally requires a headed browser and hardware-backed
WebGPU or WebGL; probe both exposed APIs before interacting with the Console.

The Artifacts upload panel requires an enabled Gateway profile upload policy and the
`artifact:upload` scope. Its controller lives in the authenticated application shell;
closing the panel and changing pages preserve active transfers. Reload reconciles
saved sessions and asks for the original file only when bytes remain to transfer.
The upload component's [`DESIGN.md`](src/uploads/DESIGN.md) describes its memory,
identity, and recovery boundaries. Queue tests run with the existing `npm test` command.

Publish accepted changes with `cargo xtask image stage --target console-bff` and the
installation's registry arguments. The Cargo-derived image source boundary reuses the
compiled BFF when only frontend files change. Follow the
[iteration runbook](../../../docs/DEVELOPMENT_ITERATION.md) for staging and GitOps rollout.
