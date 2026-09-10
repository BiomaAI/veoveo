# Console Session Projection

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| HTTP, RFC 9110 | Authenticated `GET /console-api/{profile}/session`, JSON, `no-store` |
| Veoveo OAuth and Work Context | Existing gateway authentication, exact profile audience, current session family and invocation authority |
| JSON Schema 2020-12 | Closed `ConsoleBootstrap`, branding and identity DTOs owned by `mcp/contract/src/gateway/console.rs` |

The session response opens the Console without installation-inventory authority.
It contains current identity, one current tenant, Work Context and public branding.
It never loads principal inventories, Tasks, servers or control-plane secrets.
`canReadInstallation` reflects the current catalog's `AdminRead` decision against
the Gateway target. It is a presentation hint; administrative handlers independently
authorize and audit their requests. Bootstrap does not execute an administrative action.

The existing authentication middleware selects the profile from `console-api` and
establishes current subject authority before projection. An unknown profile returns
404. `presentation.rs` is shared with the administrator snapshot. That snapshot can
prefer a stored human display name when authenticated display metadata is absent;
bootstrap uses authenticated metadata or the compact subject fallback.

Catalog tests cover ordinary users, permission changes and inventory exclusion.
The authenticated public route still requires installed ingress qualification.
