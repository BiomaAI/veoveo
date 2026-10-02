# Knowledge Retrieval Model Comparison

The workload uses source commit `f91ed99b03bdfaf3d329dc83257fe5162b39db0d`.
Runs execute on 2026-10-02 UTC with the cluster and BuildKit stopped. The
[evaluation workload](../../../../servers/knowledge-mcp/evaluation/README.md)
owns corpus construction and the production indexing/search path being measured.

## Workload And Hardware

Twelve fictional inspection scenarios and five actual source designs produce 153
members across nineteen collections. The 78 fixed queries include Spanish
paraphrases, output discovery, temporal records and Map catalog roles. Every query
searches the whole caller-visible corpus. The `structure-v1` chunker uses 1,500
characters with 150-character overlap, producing 1,197 chunks for every model.

The dataset revision is
`sha256:4f19467ad0c58775f1c869eadd3a197fd63d67d472d132d6b887598399fd724f`.
The source-corpus revision is
`sha256:6b783300661cc4ce03d619127674f16b49f1348a0af75b35cd5628b2a88fab05`.
All models use identical source bytes, judgments, audience and collection settings.
The query instruction is `Given a web search query, retrieve relevant passages that answer the query`.
The shared client applies Qwen's instruction wrapper.

The host has an NVIDIA RTX 4090 with 24,564 MiB reported memory, driver 595.91.07,
and an AMD Ryzen 9 5950X CPU. The official vLLM 0.30.0 image is
`sha256:8a69ffad015f138d7170c4ddc429e230a3bc1c1719f67e14324749df200a4b90`.
Its PyTorch 2.13.0, Transformers 5.17.0 and CUDA 13.0 packages serve every model.
The runtime requires CUDA and uses BF16 weights with the pooling runner. All three
runtime logs select the `FLASH_ATTN` attention backend. Checkpoint
revisions and file manifests are in the [verification guide](README.md#retrieval-model-comparison).

The command preserves full 32,768-token context and uses these settings for every model:

```text
--runner pooling --scheduling-policy priority
--max-model-len 32768 --max-num-batched-tokens 8192 --max-num-seqs 32
--kv-cache-memory-bytes 4966055936 --gpu-memory-utilization 0.90
```

The explicit 4.625 GiB KV-cache allocation overrides the memory-fraction setting for
cache sizing. A common allocation exposes the model's additional memory cost. It is
a comparison setting; the reference installation uses a 25% memory fraction for 0.6B.
Checkpoint downloads, manifest verification and runtime startup finish before timing.
The runtime loads checkpoints offline and preserves its compilation cache between runs.

Each run owns a fresh SurrealDB 3.3.0 RocksDB container, limited to two CPUs and
2 GiB RAM, using image
`sha256:681c6c22c287421b5c7d99e0fde79b6e0d32c36c1ddeaab2762a1661cb04cd20`.
The native harness builds an initial generation and warms all judged queries. It then
rebuilds a second generation while one search can run at a time against the initial
generation. Searches have a 100 ms arrival interval with missed ticks skipped; this
does not create an open-loop ten-query-per-second load. Rebuild timing includes
generation preparation, draining the last search and generation activation. Final
recall measures the second generation. Each report is persisted and read through a
second database connection before export.

## Results

All three runs pass all 78 judgments, with every judged relevant member present in
the first ten results. Recall is the mean of each query's retrieved-relevant fraction.
Every run rebuilds the same 1,197 chunks. GPU memory is the largest observed sample
through the full native run, including its initial build and query warmup.

| Model | Dimensions | Recall at 10 | Rebuild seconds | Chunks/second | vLLM peak MiB | Device peak MiB |
|---|---:|---:|---:|---:|---:|---:|
| Qwen3-Embedding-0.6B | 1,024 | 1.000 | 12.172 | 98.34 | 6,962 | 8,406 |
| Qwen3-Embedding-4B | 2,560 | 1.000 | 23.703 | 50.50 | 14,456 | 15,900 |
| Qwen3-Embedding-8B | 4,096 | 1.000 | 40.151 | 29.81 | 20,964 | 22,436 |

| Model | Searches during rebuild | Search p50 ms | Search p95 ms | Search maximum ms |
|---|---:|---:|---:|---:|
| 0.6B | 18 | 640.852 | 906.748 | 906.748 |
| 4B | 23 | 993.873 | 1,289.665 | 1,313.609 |
| 8B | 30 | 1,236.639 | 1,964.942 | 2,333.872 |

The [three native reports](retrieval-2026-10-02.jsonl) preserve generation identities,
source digests, judgments, returned ranks, query timings and per-collection rebuild
measurements. Each line uses the harness's existing report format. The
[GPU samples](retrieval-gpu-2026-10-02.csv) preserve 147, 270 and 334 observations
respectively; device totals and vLLM process memory are separate columns. The model
column identifies the corresponding report. Peak vLLM memory excludes RustDesk's
497 MiB compute allocation.

Keep Qwen3-Embedding-0.6B for the reference installation. Both larger checkpoints
reach the same recall on this corpus, while the smaller checkpoint uses less memory
and rebuilds faster. This result supplies no measured retrieval gain that justifies
replacing it. All three checkpoints run on CUDA at full configured context without
quantization or CPU offload. The owned runtime and database containers are removed
after each run.

## Interpretation Limits

This workload measures Knowledge's hybrid retrieval, including lexical and vector
search. It cannot isolate embedding-model quality from the lexical contribution.
The small constructed corpus does not establish production-wide recall. Judgments
were fixed before model execution and are unchanged by the returned rankings.

One run per model supplies no variance estimate. Models execute sequentially in
0.6B, 4B, 8B order, without a claim that host scheduling was identical. Concurrent
search latencies measure one client during this rebuild; they do not establish load
capacity or an installed latency objective. The runs do not qualify live source
mutation, restart behavior, or simultaneous simulation and Reason workloads.

## Reproduction

Use the source commit above to preserve the embedded design bytes. Generate all three
configurations together with the [corpus writer](../../../../servers/knowledge-mcp/evaluation/README.md#inputs).
Follow the verification guide's checkpoint and CUDA requirements, then run its
three pinned checkpoints sequentially with the common settings above. For each model,
set `VEOVEO_RETRIEVAL_INPUT`, a fresh `VEOVEO_RETRIEVAL_OUTPUT`,
`VEOVEO_EMBEDDING_URL` and a private `VEOVEO_EMBEDDING_API_KEY_FILE`, then run:

```sh
cargo test -p veoveo-knowledge-mcp --test gpu_retrieval \
  domain_recall_and_rebuild_with_concurrent_searches -- --ignored --exact --nocapture
```

Sample `nvidia-smi` device and compute-process memory once per second through each
native run. Sum only `VLLM::EngineCore` rows for model-process memory. Device totals
also include the desktop, browser and RustDesk. Sampling starts after runtime
readiness, covers initialization and both generations, and does not measure startup
peaks or guarantee capture of subsecond peaks. Stop and remove each owned runtime
before starting the next model, remove the temporary API key after the final run,
and preserve checkpoint and compilation caches.
