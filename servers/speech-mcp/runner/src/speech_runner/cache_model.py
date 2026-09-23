"""Build-time download of the qualified checkpoint; never invoked at runtime."""
from hashlib import file_digest
from pathlib import Path

from huggingface_hub import snapshot_download

from .protocol import MODEL, REVISION

WEIGHTS_SHA256 = "c9608f36d0ab956c14bfcc525479b0746b3b42a56f6949ec85c14eb7466717dc"


def main():
    snapshot = Path(snapshot_download(
        MODEL, revision=REVISION,
        allow_patterns=["config.json", "tokenizer.json", "model.safetensors"],
    ))
    with (snapshot / "model.safetensors").open("rb") as weights:
        if file_digest(weights, "sha256").hexdigest() != WEIGHTS_SHA256:
            raise RuntimeError("Speech checkpoint digest mismatch")


if __name__ == "__main__":
    main()
