# Use recordings with Rerun

Veoveo publishes committed recording layers as Rerun datasets. Each recording is a
segment. The Console follows an active recording through Veoveo's live RRD stream;
the Rerun Catalog SDK queries committed layers through the read-only Redap service.

## Get a catalog grant

An authenticated service may request selected recordings from one dataset. Use its
OAuth access token for the `operator` profile and `operations` Work Context. The
request must name a UUIDv7 dataset and at least one UUIDv7 recording admitted by
policy. The response includes an `entry_uri`, `redap_token`, and `expires_at`.

```sh
curl --fail-with-body --silent --show-error \
  -H "Authorization: Bearer $VEOVEO_ACCESS_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"dataset_id":"<dataset-uuid>","recording_ids":["<recording-uuid>"]}' \
  https://veoveo.bioma.ai/recordings/operator/catalog-grants
```

The service checks the caller's OAuth authority and current recording policy, records
the grant, and returns a read-only Redap token valid for at most five minutes. Keep
both tokens out of logs, notebooks, and shared files. A grant names only the admitted
recordings; it does not open the rest of the dataset.

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
follows its authenticated live RRD stream and switches to committed archive playback
when the recording closes. Catalog queries see committed layers of an active recording,
but the native Catalog SDK is not a subscription to Veoveo's live receiver. Clients
that need continuous RRD bytes use the separately authorized
`GET /recordings/{profile}/{recording_id}/live/rrd-stream` endpoint. Its media type is
`application/vnd.veoveo.rerun.rrd-stream; framing=be32; version=2`; each frame has a
four-byte big-endian length followed by one RRD payload. The live route uses the
caller's OAuth authority, rather than a Redap catalog token.

## Ingress requirements

The chart gives Redap its own Ingress and marks `recording-mcp` as an h2c upstream
for Traefik. A different ingress controller must preserve HTTP/2 gRPC to the service;
set `ingress.redapAnnotations` for that controller's gRPC backend protocol. Qualify
native clients and gRPC-Web separately. Terminating HTTP/2 into ordinary HTTP/1.1
does not support the native Viewer or Catalog SDK.
