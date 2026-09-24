# Use recordings with Rerun

Veoveo publishes committed recording layers as Rerun datasets. Each recording is a
segment of its dataset. The Console follows an active recording through Veoveo's live
RRD stream, and Rerun clients query committed layers through the Redap service.

## What Veoveo supports with Rerun 0.38.1

| Capability | Support |
|---|---|
| Recording format | Rerun `0.38.1` RRD. Each committed layer uses the dataset UUID as its Rerun application ID and the recording UUID as its recording and segment ID. |
| Native clients | The Rerun Viewer and the Python Catalog SDK (`rr.catalog.CatalogClient`) over HTTP/2 gRPC. |
| Browser client | The Console's embedded Rerun WebViewer over gRPC-Web on the same Redap path. |
| Redap reads | Dataset entries and schemas, segment tables, dataset manifests, RRD manifests, segment assets, dataset queries, chunk fetches, and event watching. Selected `re_redap_tests 0.38.1` assertions cover query filters, manifest scans, chunk completeness, and missing segments. |
| Redap writes | Refused. Data enters through [recording ingest](RECORDING_INGEST.md), which applies each producer's tenant, dataset, labels, retention, and quota. |
| Storage | Archive shards compacted with Rerun's object-store chunk profile; live playback uses Rerun's live profile. |
| Video | H.264 `VideoStream` samples keep their original timeline indices, and keyframes are derived from the encoded access units so archived video stays seekable. |

## Get a catalog grant

A client asks the gateway for access to selected recordings in one dataset. Use an
OAuth access token for a gateway profile that exposes the `recording` server; the
caller's Work Context and current recording policy decide which recordings it may
read. The request names one UUIDv7 dataset and at least one UUIDv7 recording:

```sh
curl --fail-with-body --silent --show-error \
  -H "Authorization: Bearer $VEOVEO_ACCESS_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"dataset_id":"<dataset-uuid>","recording_ids":["<recording-uuid>"]}' \
  "https://$VEOVEO_HOST/recordings/$VEOVEO_PROFILE/catalog-grants"
```

The gateway checks the caller's authority and policy, records the grant, and returns
an `entry_uri`, a `redap_token`, and an `expires_at` time. The Redap token is
read-only, valid for at most five minutes, and accepted only on the installation host
it was issued for. A grant covers only the recordings it names, not the rest of the
dataset. Keep both tokens out of logs, notebooks, and shared files.

## Connect a native Rerun client

Remote native Viewer and Catalog SDK access is **not reachable through the current
infrastructure at `veoveo.bioma.ai`**. Browser playback works through the public
Tunnel, and the installed SDK smoke uses the direct loopback ingress. The native
examples below require an installation route that carries HTTP/2 gRPC.

The native Viewer and Python Catalog SDK require HTTP/2 gRPC. Use version 0.38.1
to match Veoveo's recording service. The installation hostname in `entry_uri` must
resolve to an endpoint that carries native gRPC. Tokens permit that hostname only;
substituting an IP address or another hostname fails Rerun's token host check.
An approved private DNS route can preserve the hostname while selecting a different
network path.

A Python notebook with `rerun-sdk[catalog]==0.38.1` can request a grant and query a
pandas dataframe. Supply the installation origin, admitted profile, dataset and
recording IDs through the environment along with its current OAuth access token:

```python
import json
import os
from urllib.parse import urlsplit
from urllib.request import Request, urlopen

import rerun as rr
from datafusion import col

def request_catalog_grant():
    request = Request(
        f'{os.environ["VEOVEO_ORIGIN"].rstrip("/")}'
        f'/recordings/{os.environ["VEOVEO_PROFILE"]}/catalog-grants',
        data=json.dumps({
            "dataset_id": os.environ["VEOVEO_DATASET_ID"],
            "recording_ids": [os.environ["VEOVEO_RECORDING_ID"]],
        }).encode(),
        headers={
            "Authorization": f'Bearer {os.environ["VEOVEO_ACCESS_TOKEN"]}',
            "Content-Type": "application/json",
        },
        method="POST",
    )
    with urlopen(request, timeout=30) as response:
        return json.load(response)

grant = request_catalog_grant()
entry = urlsplit(grant["entry_uri"])
client = rr.catalog.CatalogClient(
    f"{entry.scheme}://{entry.netloc}", token=grant["redap_token"]
)
dataset = client.get_dataset(id=entry.path.rsplit("/", 1)[-1])
indexes = [column.name for column in dataset.schema().index_columns()]
timeline = "log_time" if "log_time" in indexes else next(iter(indexes), None)
frame = (
    dataset.reader(index=timeline)
    .filter(col("rerun_segment_id") == os.environ["VEOVEO_RECORDING_ID"])
    .limit(1000)
    .to_pandas()
)
```

The query selects an available timeline, filters the requested segment, and limits
the result before converting it to pandas. The pinned acceptance client is in
[`testing/recording-catalog-sdk/`](../testing/recording-catalog-sdk/).

To open the same entry in a locally installed Rerun Viewer 0.38.1, pass the token
through its `REDAP_TOKEN` environment variable. This uses the grant already held
in memory and keeps the token out of command arguments:

```python
import subprocess

viewer = subprocess.Popen(
    ["rerun", grant["entry_uri"]],
    env={**os.environ, "REDAP_TOKEN": grant["redap_token"]},
)
```

Rerun's [connection registry](https://github.com/rerun-io/rerun/blob/b08c599e934b0dedee1e95fd74a989a1582ce3d5/crates/data_flow/re_redap_client/src/connection_registry.rs)
tries saved credentials for a server before this environment token.
Clear a stale server credential in the Viewer before reconnecting with a fresh grant.
Check `rerun --version` before launching; the SDK environment and a separately
installed Viewer can have different versions.

The Bioma public hostname currently reaches Veoveo through a Cloudflare Tunnel public
hostname route. [Cloudflare documents native gRPC support for private subnet
routing, while public Tunnel hostname routes are unsupported](https://developers.cloudflare.com/network/grpc-connections/).
The Console WebViewer works
there through gRPC-Web. Native clients must use a direct HTTP/2 ingress address or an
operator-approved private route to the same recording service. The installed smoke
runner maps `veoveo.bioma.ai` to the local k3d ingress and connects on port 8781, so
the SDK sees the hostname authorized by the grant while exercising the real Redap
Ingress. The runner uses `rerun+http://veoveo.bioma.ai:8781` only on that loopback
path. Use TLS for remote clients. This direct path is for local verification; it
does not make native gRPC available through the public Tunnel hostname.
Remote native access is a separate infrastructure follow-up. Its acceptance must
run from a remote client over the selected route, including grant renewal and the
installation hostname check.

The installed SDK scenario requires the operator service client's private-key file
and registered key ID in `VEOVEO_SERVICE_CLIENT_PRIVATE_KEY_FILE` and
`VEOVEO_SERVICE_CLIENT_KEY_ID`. Supply those from the installation's credential
store before running `cargo xtask smoke recording-catalog-sdk` with the admitted
dataset and recording IDs. A Console browser login does not provide service-client
credentials to this command. The scenario sends issued grants to the SDK through
stdin and reports only query counts and renewal success.

## Renew access during a session

Request another catalog grant before `expires_at`, then construct a new Viewer or
`CatalogClient` connection with the returned token. A five-minute token does not
renew an existing connection. Reissue the request with the same selected IDs and
current OAuth token; a new policy decision may narrow or deny access. The installed
SDK smoke scenario performs a second grant and repeats its dataframe query after
reconnecting. A native Viewer launched with `REDAP_TOKEN` must be relaunched with
the new token; changing the parent process's environment does not update it.

## Follow an active recording

Open **Recordings** in the Console and select the active recording. The Console
follows its live RRD stream and switches to archive playback when the recording
closes. Catalog queries see the committed layers of an active recording; the Catalog
SDK does not subscribe to live data. A client that needs continuous RRD bytes reads
`GET /recordings/{profile}/{recording_id}/live/rrd-stream` with its OAuth token. The
response type is `application/vnd.veoveo.rerun.rrd-stream; framing=be32; version=2`,
and each frame is a four-byte big-endian length followed by one RRD payload.

## Ingress requirements

Native Rerun clients need HTTP/2 gRPC from the client to the recording service. The
chart gives Redap its own Ingress on `/rerun.cloud.v1alpha1.RerunCloudService` and
marks `recording-mcp` as an h2c upstream for Traefik. For another ingress controller,
set its gRPC backend protocol through `ingress.redapAnnotations`. A proxy or tunnel
that converts HTTP/2 to HTTP/1.1 carries only the browser's gRPC-Web traffic, so test
native clients and the WebViewer separately on each route an installation exposes.

In the Bioma reference installation, the public hostname uses a Cloudflare Tunnel
route that carries gRPC-Web only. Its SDK smoke scenario maps `veoveo.bioma.ai` to the
local k3d ingress on port 8781, so the SDK presents the hostname the grant was issued
for while reaching the real Redap Ingress.
