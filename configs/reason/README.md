# Reason runtime configuration

`catalog.example.json` is the typed configuration contract for the reason
server. A deployment mounts its real catalog at
`/etc/veoveo/reason/catalog.json` together with the prompt template and the
site-compiled TensorRT-LLM engine the catalog references.

The catalog names one or more world models and the reasoning pipelines that
use them. Model paths and prompt template paths must be absolute, and the
server fails readiness when any referenced file is missing. The engine is a
site-supplied deployment input compiled for the deployment GPU; checkpoint
compilation never happens at request time. The `engine_digest` field is
optional and travels into every result's audit identity, so populate it with
the digest of the deployed engine whenever reproducibility evidence matters.

`prompt_revision` identifies the prompt template contract. Change the
revision whenever the template changes, because results record the revision
and greedy-decoded results are only comparable within one revision.

The observation block declares the resolution the runner samples frames to
before the model observes them. The example uses the world model's native
640x360 observation size.
