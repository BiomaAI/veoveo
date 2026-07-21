# Reason runtime configuration

`catalog.example.json` is the typed configuration contract for the reason
server. A deployment mounts its real catalog at
`/etc/veoveo/reason/catalog.json` together with the prompt template and the
world-model checkpoint the catalog references.

The catalog names one or more world models and the reasoning pipelines that
use them. Model paths and prompt template paths must be absolute, and the
server fails readiness when any referenced path is missing. The checkpoint is
a site-supplied deployment input in Hugging Face layout, mounted read-only;
runtime optimization such as quantization is the runner image's concern and
never happens at request time. The `model_digest` field is optional and
travels into every result's audit identity, so populate it with the digest of
the deployed checkpoint whenever reproducibility evidence matters.

`prompt_revision` identifies the prompt template contract. Change the
revision whenever the template changes, because results record the revision
and greedy-decoded results are only comparable within one revision.

Each model declares its inference engine budget. The vLLM engine requires a
bounded `gpu_memory_utilization` and `max_model_len`; deployments size both
against the other GPU workloads that must remain resident. The Helm defaults
reserve 70% of device memory and an 8192-token context for the Reason engine.
These are engine limits, not scheduler promises, and a pod still requests a
real `nvidia.com/gpu` device.

The observation block declares the resolution the runner samples frames to
before the model observes them. The example uses the world model's native
640x360 observation size.
