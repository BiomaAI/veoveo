# Frames MCP Design

This document is the canonical design and operational contract for the
`frames-mcp` crate.

`frames-mcp` owns local spatial frames and bounded coordinate conversion for
robots, sensors, vehicles, simulations, and mission workspaces. Map MCP owns
Earth geography, projected CRS work, geodesics, geofences, and logistics.

## Status

Implemented in this workspace. The canonical surface is:

```text
crate       veoveo-frames-mcp
folder      servers/frames-mcp
slug        frames
URI scheme  frames
endpoint    /frames/mcp
health      /frames/healthz
```

Frames has one canonical server, scheme, gateway namespace, deployment, and
documentation surface.

## Responsibility

Frames answers a focused question: how should bounded positions be expressed
in WGS84, ECEF, ENU, or NED when every local frame has an explicit WGS84
origin? It records the operation and returns its provenance.

Frames does not own:

- projected GIS coordinate reference systems;
- ellipsoidal route distance or bearings;
- places, addresses, borders, facilities, terrain, charts, or airspace;
- geographic geofences or transport restrictions;
- road, rail, off-road, maritime, aviation, or intermodal routing;
- high-rate pose telemetry or a vehicle control loop.

Those Earth-referenced operations belong to `map-mcp`. High-rate observations
belong in governed recordings.

## Tools

| Tool | Execution | Result |
|---|---|---|
| `derive_local_frame` | direct | Creates a durable ENU or NED frame from a validated WGS84 origin. |
| `convert_frame` | direct | Converts a bounded list between WGS84, ECEF, ENU, and NED. |
| `batch_transform` | task required | Runs the same conversion durably and may publish the result through the artifact plane. |

The server rejects direct `batch_transform` calls. Task-capable clients use the
final MCP task extension and can get, subscribe to, cancel, detach from, and
resume the durable task.

## Coordinate Contract

The hosted contract is strongly typed:

```text
CoordinatePoint
  Wgs84(latitude_deg, longitude_deg, height_m)
  Ecef(x_m, y_m, z_m)
  Enu(frame_id, east_m, north_m, up_m)
  Ned(frame_id, north_m, east_m, down_m)
```

WGS84 latitude is within `[-90, 90]`; longitude is within `[-180, 180]`.
Every ordinate must be finite. A local point names its frame, and the stored
frame kind must agree with that point variant.

The built-in durable frames are WGS84 and ECEF. ENU and NED frames are created
with `derive_local_frame`. A local frame carries its origin, axis convention,
handedness, units, and description through the shared Rerun frame definition.

## Resources

```text
frames://frames
frames://frame/{frame_id}
frames://operation/{operation_id}
frames://artifact/{artifact_id}
frames://usage
frames://usage/task/{task_id}
```

Frame and operation resources are owner- and tenant-scoped. Artifact resource
reads use the same task-bound capability and shared artifact authorization path
as other hosted servers. Task usage remains addressable after the MCP session
ends.

Resource lists are paginated. Frame identifiers support completion. Resource
identities retain the `frames://` scheme when gateway tools are projected under
the `frames__` namespace.

## Prompts

| Prompt | Purpose |
|---|---|
| `frames-frame-audit` | Reviews declared frames, axes, origin, and approximation intent before conversion. |
| `frames-local-frame-select` | Prepares a local ENU or NED frame request for a bounded mission workspace. |
| `frames-transform-explain` | Explains operation provenance and frame semantics without inventing missing transforms. |

## Calculation

Frame math is local and deterministic. WGS84/ECEF conversion uses the WGS84
ellipsoid. ENU and NED conversion resolves the declared local origin and applies
the corresponding tangent-frame rotation. The engine does not call Map MCP or
another hosted server.

Approximation permission is explicit in the request. Each result carries a
`CoordinateOperationProvenance` record with its operation identity, source and
target frames, engine, and approximation state.

## Persistence And Recovery

SurrealDB stores frame definitions, operation records, task state, task inputs,
usage, and ownership. The server uses database-scoped credentials and the
shared `TaskRuntime`.

Direct conversions write their operation record before returning. Batch work
uses a resumable durable task with a lease and heartbeat. Restart recovery
claims resumable work through the same task runtime. Cancellation terminates
the worker and produces a terminal task state.

Large batch results use the artifact plane. The task obtains a bounded write
capability, publishes the result, and returns typed metadata. It does not expose
object-store paths or unaudited content URLs.

## Authentication And Isolation

The hosted endpoint requires a gateway-signed internal identity and a forwarded
bearer token. The assertion fixes the server slug, gateway profile, principal,
tenant, labels, scopes, and expiry. Direct unsigned access is rejected.

Frame, operation, task, artifact, and usage reads enforce the same owner,
tenant, profile, and data-label boundaries. Unknown and unauthorized
identifiers are indistinguishable at the resource boundary.

## Limits And Failure Semantics

Configuration bounds direct points, batch points, artifact bytes, task TTL,
task polling, and lease duration. Inputs with non-finite numbers, invalid
ranges, incompatible frame kinds, unknown local origins, or unsupported target
frames fail explicitly.

The server never:

- guesses a local origin or axis convention;
- treats degrees as radians;
- copies a coordinate it could not transform;
- routes a geographic path;
- interprets a projected CRS;
- substitutes a map operation for a frame operation.

## Module Layout

```text
servers/frames-mcp/
  src/
    contract.rs
    engine.rs
    state.rs
    artifacts.rs
    uris.rs
    bin/
      server.rs
      server/
        app_state.rs
        config.rs
        host.rs
        internal_auth.rs
        outputs.rs
        ownership.rs
        prompts.rs
        task_extension.rs
```

The binary wires transport and dependencies. Contract, calculation, durable
state, artifact access, ownership, prompts, and task adaptation remain focused
modules.

## Deployment And Tests

Compose and Helm register `frames-mcp` at port `8793`. The gateway catalog
advertises three tools, the `frames://` resource scheme, three prompts,
completions, notifications, and tasks. Caddy rejects direct public MCP access.

Rust unit tests cover validation and conversion. Contract schemas are emitted
by the conformance binary. The Rust smoke harness exercises host validation,
internal authentication, discovery, frame creation, conversion, durable batch
work, artifact ownership, usage, and resource projection.
