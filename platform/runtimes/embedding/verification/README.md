# Embedding Runtime Qualification

The runtime design owns [acceptance](../DESIGN.md#verification). Native client and Helm
tests do not establish CUDA execution or installed network isolation.

## Reference Vectors

`reference.py` follows the pinned model card's last-token pooling and L2 normalization.
It runs Transformers on CUDA with bfloat16 weights. The padded reference batch requires
cuDNN fused attention because PyTorch Flash Attention rejects its non-null padding
mask. The script requires that backend and records the installed library versions and
GPU. The runtime itself selects its supported fused attention implementation.

The committed `reference.json` is generated in the official pinned vLLM image, using
that image's Transformers and PyTorch packages. This keeps the qualification dependency
set equal to the runtime image. Stage the checkpoint's ten files and verify
`checkpoint.sha256` first. From the repository root:

```sh
embedding_image=vllm/vllm-openai@sha256:8a69ffad015f138d7170c4ddc429e230a3bc1c1719f67e14324749df200a4b90
timeout 240s docker run --rm --gpus all --network none \
  --user "$(id -u):$(id -g)" --shm-size 1g \
  -e HOME=/tmp -e HF_HUB_OFFLINE=1 -e UV_CACHE_DIR=/tmp/uv \
  -v "$(command -v uv):/usr/local/bin/veoveo-uv:ro" \
  -v "$EMBEDDING_CHECKPOINT:/models/checkpoint:ro" \
  -v "$PWD/platform/runtimes/embedding:/qualification" \
  --entrypoint /usr/local/bin/veoveo-uv "$embedding_image" \
  run --offline --no-project --no-managed-python -- \
  python3 /qualification/verification/reference.py \
  --checkpoint /models/checkpoint --manifest /qualification/checkpoint.sha256 \
  --output /qualification/verification/reference.generated.json
```

The script refuses an existing output. Review the generated fixture before replacing
`reference.json`. Reference generation refuses a missing GPU or invalid checkpoint.
No package installation or model download occurs in the container.

## Served Vectors And Scheduling

Start the chart's pinned runtime with a verified checkpoint and installation key. Prove
CUDA access in its container before running these checks. Set `VEOVEO_EMBEDDING_URL` to
that runtime's HTTP origin and `VEOVEO_EMBEDDING_API_KEY_FILE` to a private file containing
the key without a trailing newline. Then run:

```sh
cargo test -p veoveo-embedding-client --test gpu -- --ignored --test-threads=1 --nocapture
```

The reference case checks all four vectors at cosine similarity of at least 0.999.
The scheduling case queues six bulk batches through one shared client and requires an
interactive query to finish ahead of at least four queued batches. It reports query
latency, bulk completion times and total input throughput as JSON. Each case has a
120-second deadline. These small fixtures qualify correctness and client scheduling;
Knowledge's recall and model-size comparison need its own representative evaluation set.

## Refusal And Network Checks

Run the chart's init command against a fixture with one altered checkpoint file and
require failure before vLLM starts. Run its container startup command without a CUDA
device and require refusal. Preserve the accepted checkpoint while creating corruption
fixtures, and remove every owned test container after it stops.

An unauthenticated `/v1/models` request must return 401. In Kubernetes, check the
authenticated route from a platform pod, then try it from a `computer-host` pod and
a pod in a different namespace. Both denied connections must fail before the probe's
deadline, by rejection or timeout according to the installed CNI. Use the same Service
IP and port for allowed and denied probes, and require an allowed request to succeed
in the same run. Confirm that the denied pods ran the client and that the network
policy caused the failure; missing binaries, DNS failures and unavailable endpoints
do not qualify. Remove owned probe pods and namespaces afterward. Test the real CNI
policy; Docker networking and Helm rendering cannot qualify namespace isolation.
