"""Shared build-time and offline runtime admission of the immutable checkpoint."""
from dataclasses import dataclass
from hashlib import file_digest
from pathlib import Path

MODEL = "moondream/parakeet-ultra"
REVISION = "73175eb7aeb0d82f1e2a6b53b3aabc10a90bcd0b"


@dataclass(frozen=True)
class CheckpointFile:
    name: str
    byte_len: int
    sha256: str


REQUIRED_FILES = (
    CheckpointFile("config.json", 1153,
                   "e747b85e1bdfd300c8b8ac63bac8dd5221f8fe9bc275b48d06c735fcd6971b6e"),
    CheckpointFile("tokenizer.json", 1159960,
                   "bd321b096832a3f270bd3b2a88823957920f1a5c5ada71114a26ea729d0cbe91"),
    CheckpointFile("model.safetensors", 1255353386,
                   "c9608f36d0ab956c14bfcc525479b0746b3b42a56f6949ec85c14eb7466717dc"),
)


def verified_checkpoint(*, local_files_only: bool) -> Path:
    """Resolve the exact HF snapshot and admit every required file before loading."""
    from huggingface_hub import constants, snapshot_download

    try:
        cache = Path(constants.HF_HUB_CACHE).resolve()
        snapshot = Path(snapshot_download(
            MODEL, revision=REVISION,
            allow_patterns=[item.name for item in REQUIRED_FILES],
            local_files_only=local_files_only,
            cache_dir=cache,
        )).resolve(strict=True)
        repository = cache / "models--moondream--parakeet-ultra"
        if not snapshot.is_dir() or snapshot != repository / "snapshots" / REVISION:
            raise ValueError("invalid checkpoint snapshot")
        # A cached snapshot can retain optional provider inputs despite allow_patterns.
        # Full-precision Ultra admits only these files, including no dangling extras.
        if {entry.name for entry in snapshot.iterdir()} != {item.name for item in REQUIRED_FILES}:
            raise ValueError("unsupported checkpoint snapshot membership")
        # The maintained Hub cache uses both repository-local and shared Xet-backed
        # blobs. Admit those configured locations without admitting their parent.
        roots = (snapshot, (repository / "blobs").resolve(), (cache / "blobs").resolve())
        if any(not root.is_relative_to(cache) for root in roots):
            raise ValueError("invalid checkpoint blob store")
        for item in REQUIRED_FILES:
            path = (snapshot / item.name).resolve(strict=True)
            if not any(path.is_relative_to(root) for root in roots) or not path.is_file() or path.stat().st_size != item.byte_len:
                raise ValueError("invalid checkpoint file")
            with path.open("rb") as source:
                if file_digest(source, "sha256").hexdigest() != item.sha256:
                    raise ValueError("checkpoint digest mismatch")
    except Exception:
        # Hub and filesystem diagnostics can contain cache paths or credentials.
        raise RuntimeError("Speech checkpoint admission failed") from None
    return snapshot


def main():
    verified_checkpoint(local_files_only=False)


if __name__ == "__main__":
    main()
