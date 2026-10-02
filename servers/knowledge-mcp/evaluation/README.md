# Retrieval Evaluation

This workload measures Knowledge's indexer and SQL hybrid search with the shared
GPU embedding client. The [service design](../DESIGN.md#retrieval-evaluation) defines
the measurements. Source conformance and installed retrieval have separate harnesses.

## Prerequisites

- Docker with the repository's pinned SurrealDB 3.3.0 image available. The harness
  creates and removes an isolated RocksDB fixture with generated credentials.
- A running CUDA embedding runtime qualified by the
  [runtime verification](../../../platform/runtimes/embedding/verification/README.md).
  Record its checkpoint, image, hardware, memory allocation and observed GPU memory
  beside the report. The harness checks the configured embedding space through model
  discovery but cannot prove the remote process's hardware from HTTP alone.
- A source corpus containing at least eleven members and ten judged queries. Each
  approved collection must be populated and have a relevance judgment. Every query
  must select more than ten candidate members. Build the corpus from the selected source
  owners' current resource shapes, with queries that include paraphrases and relevant
  distractors. Review relevance independently of the model's returned ranking.

## Inputs

`VEOVEO_RETRIEVAL_INPUT` names an absolute JSON path. Its closed configuration has
`space`, `queryTask`, `chunking`, and `corpus` fields. `space` is the shared
`EmbeddingSpace`; `chunking` uses `ChunkSettings`. Use the reference installation's
1,500-character chunks, 150-character overlap and `structure-v1` chunker for its
baseline. Keep that setting and the query instruction identical across model sizes.

The corpus contains:

| Field | Value |
|---|---|
| `registrations` | `CollectionRegistration` entries from source declarations and indexing approvals |
| `members` | Each entry has a `link` with URI and optional title, the exact source `text`, and its complete `observation` |
| `cases` | `RetrievalCase` entries: a stable case ID, natural-language query, optional collection selection, and relevant collection/URI pairs |
| `audience` | Tenant, principal, profile, explicit collection-to-`ResourceSelection` map, active Work Context, readable Work Contexts, group memberships, scopes and clearance |

The corpus must equal the complete visible index under that audience. Hidden members
belong in access-control fixtures; missing visible members invalidate a quality
measurement. Every judged URI must exist in the corpus. The fixture refreshes only
the observation time, leaving source bytes, revision, provenance and access unchanged.
Expired grants and record deadlines still apply. Corpus inputs may contain private
source data; retain them and the output with the same access restrictions.

The model configurations must share both dataset and source-corpus fingerprints.
The dataset binds member revisions, content digests and query judgments. The
source-corpus fingerprint also binds link titles, complete observations, registrations
and the audience. Resource URIs use the source owners' builders when constructing
fixtures.

## Run

Set the absolute input and fresh output paths in `VEOVEO_RETRIEVAL_INPUT` and
`VEOVEO_RETRIEVAL_OUTPUT`. Set `VEOVEO_EMBEDDING_URL` to the runtime origin and
`VEOVEO_EMBEDDING_API_KEY_FILE` to its private key file. Then run:

```sh
cargo test -p veoveo-knowledge-mcp --test gpu_retrieval \
  domain_recall_and_rebuild_with_concurrent_searches -- --ignored --exact --nocapture
```

The deadline is thirty minutes. Failure removes the owned database fixture and does
not write a successful report. The harness never starts a cluster or downloads a model.
Run model sizes sequentially when they share a GPU, preserving their checkpoint and
compilation caches between runs.

The report includes recall at ten, per-query counts and ranks, per-collection indexing
throughput, full rebuild throughput, concurrent search count and latency percentiles.
An empty result scores zero. A failed query fails the run; it is never omitted from
the denominator. The run stores the evaluation beside the measured generation and
checks it through a second database connection before exporting the report. Fixture
cleanup deletes that isolated generation; the report preserves its identity and result.

Use measured retrieval gains and GPU memory together when selecting 0.6B, 4B or 8B.
Synthetic-vector unit tests establish evaluator behavior and do not measure model quality.
