# Workspace Application API

## Standards And Protocols

`/workspace-api/{profile}` is a Veoveo HTTP JSON application contract. It uses the
gateway's current OAuth access-token, session-family and Work Context admission.
Rust DTOs are defined in `mcp/contract/src/workspace.rs`. Change notifications use
Server-Sent Events with committed chat sequence cursors. The endpoint is not MCP;
capability execution continues through the platform's existing MCP profile.

## Authority

Handlers admit direct human sessions. The authenticated actor determines the
author; requests cannot supply an author or arbitrary tenant. Current stored Work
Context rules determine membership using authenticated principal, group, role and
OAuth-client selectors. The store binds that decision to its context digest and
checks chat membership in every operation. Installation administration is not
required to participate.

The browser edge fixes the profile and upstream origin, retains tokens in its
encrypted cookie, and enforces CSRF on mutations. JSON responses are bounded and
marked `no-store`. Stream lifetimes must be bounded by session/token validity and
recheck authority before each wake. A wake carries the committed sequence only;
clients then fetch their authorized state.

People search is tenant-scoped and bounded. An invitation is addressed to an
installation-local person ID. Acceptance requires that person's current Work
Context authority; an invitation cannot create it. No global principal inventory,
policy catalog, credential or arbitrary proxy destination is exposed.

The API remains in implementation. Public registration, browser integration and
stream acceptance are required before this boundary is considered deployed.
