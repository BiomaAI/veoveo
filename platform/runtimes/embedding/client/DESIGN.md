# Embedding Client

## Standards And Protocols

| Standard or format | Supported profile |
|---|---|
| HTTP and JSON | Internal authenticated `GET /v1/models` and `POST /v1/embeddings`; no redirects or proxy routing |
| [vLLM 0.30.0 pooling API](https://github.com/vllm-project/vllm/blob/v0.30.0/vllm/entrypoints/pooling/base/protocol.py) | String batches, native-dimension float output, `use_activation` and scalar request priority |
| [Qwen3 Embedding model card](https://huggingface.co/Qwen/Qwen3-Embedding-0.6B/blob/97b0c614be4d77ee51c0cef4e5f07c00f9eb65b3/README.md) | Document text unchanged; queries use the model's instruction prefix and `Query:` without an added space |
| [Veoveo embedding contract](../contract/DESIGN.md) | Typed input bounds, model identity, embedding spaces and finite normalized vectors |

## Configuration And Identity

The library accepts an `EmbeddingEndpoint`, a secret API key and the expected
`EmbeddingSpace`. The endpoint permits HTTP or HTTPS origins without userinfo,
paths, queries or fragments. URL parsing and fixed-route resolution use `url`.
The hosting process installs its rustls crypto provider before connecting. A missing
provider produces a configuration error. The client uses the workspace's HTTP and
TLS dependencies and never changes the process's provider.

Connection requires `/v1/models` to advertise the configured model exactly once.
The response does not attest the checkpoint revision, dimensions or runtime image.
Those fields come from installation configuration, whose checkpoint init check and
image pin must agree with the serving Deployment. A runtime/model cut requires drained
consumers and a newly constructed client with the new space. Knowledge's generation
transition must complete before serving vectors from the new space.

## Requests And Scheduling

`embed_documents` preserves text. `embed_query` embeds one formatted query at
interactive priority; `embed_queries` supports batches with a shared typed task.
Callers choose `Interactive` or `Bulk` for batch operations. The client maps them to
vLLM priorities 0 and 10. The server must run its priority scheduler.

The contract admits 1–32 texts, at most 16 KiB each and 128 KiB combined. Tasks contain
1–1,024 printable UTF-8 bytes. The client also checks the 128 KiB bound after adding
query instructions. It requests native-dimension float output and enables the model's
pooling activation. vLLM reserves the `dimensions` request parameter for models whose
configuration advertises Matryoshka resizing; the pinned checkpoint does not advertise
it. The client omits that parameter and checks every returned vector against the
configured dimension. It never requests input truncation.

One shared client serves each replica. Clones share four request permits, with at most
one occupied by bulk work. Configuration may change these limits while reserving at
least one permit from bulk admission. Bulk callers acquire their bulk permit before a
general permit, allowing interactive calls to pass queued indexing work. This limits
client admission; GPU scheduling and latency under load require runtime qualification.

A 60-second deadline includes permit acquisition, connection, headers, body and response
validation. Configuration may select a positive deadline up to 120 seconds. Requests
use a three-second connection limit. Dropping a request releases its permits, while
upstream cancellation depends on the server noticing the disconnected request. The
client never retries a request automatically.

## Response Admission And Errors

Model discovery accepts at most 128 KiB and embedding responses at most 8 MiB. Both
limits apply while streaming even when Content-Length is absent. The adapter ignores
provider fields outside its supported response subset. It rejects a wrong model or
object kind, an incorrect vector count, repeated or missing input indexes, wrong
dimensions, nonfinite components and vectors outside the contract's L2 norm tolerance.
Input indexes determine returned order. Each vector carries its admitted space.

The authorization header is marked sensitive. Errors expose fixed diagnostics and HTTP
status codes, without provider bodies, input text, API keys or request URLs. Redirects
are refused before another request can send the key to a different destination.

## Verification

`cargo test -p veoveo-embedding-client -p veoveo-embedding-contract` runs native HTTP
fixtures with synthetic vectors. Fixtures bind ephemeral loopback ports and abort their
servers on drop. They check query formatting, model and vector admission, streamed byte
limits, secret-safe errors, redirect refusal, shared request limits and deadlines.
These checks provide no GPU, pooling or retrieval-quality acceptance. Those requirements
belong to the [runtime verification](../DESIGN.md#verification).
