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

The native Viewer and Python Catalog SDK require HTTP/2 gRPC. Give the Viewer the
returned `entry_uri` and `redap_token`. A Python notebook can query the same entry:

```python
import rerun as rr

grant = request_catalog_grant()  # Send the authenticated POST shown above.
client = rr.catalog.CatalogClient("rerun+https://<native-grpc-host>", token=grant["redap_token"])
dataset = client.get_dataset(id=grant["entry_uri"].rsplit("/", 1)[-1])
print(dataset.segment_ids())
frame = dataset.reader(index="log_time").limit(1000).to_pandas()
```

Select an index returned by `dataset.schema().index_columns()` if the recording has
no `log_time` timeline. Filter by `rerun_segment_id` when the grant covers several
recordings. The pinned acceptance client is in
[`testing/recording-catalog-sdk/`](../testing/recording-catalog-sdk/).

The Bioma public hostname currently reaches Veoveo through a Cloudflare Tunnel public
hostname route. That route does not carry native gRPC. The Console WebViewer works
there through gRPC-Web. Native clients must use a direct HTTP/2 ingress address or an
operator-approved private route to the same recording service. The installed smoke
runner maps `veoveo.bioma.ai` to the local k3d ingress and connects on port 8781, so
the SDK sees the hostname authorized by the grant while exercising the real Redap
Ingress. This direct path is for local verification; it does not make native gRPC
available through the public Tunnel hostname.

## Renew access during a session

Request another catalog grant before `expires_at`, then construct a new Viewer or
`CatalogClient` connection with the returned token. A five-minute token does not
renew an existing connection. Reissue the request with the same selected IDs and
current OAuth token; a new policy decision may narrow or deny access. The installed
SDK smoke scenario performs a second grant and repeats its query after reconnecting.

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
