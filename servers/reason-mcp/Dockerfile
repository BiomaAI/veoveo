# syntax=docker/dockerfile:1.27.0@sha256:bde3983e9c939224420ddaf6b784cc30e09b035a4dea01f581230c50809f372e
ARG VLLM_IMAGE=vllm/vllm-openai:v0.28.0@sha256:61fc8a896b0a4fbbbdc063bc4b0dbc25ce98e02b5050c24aeb7830ac02039b14
FROM ${VLLM_IMAGE}

ARG SOURCE_DATE_EPOCH
USER root
RUN useradd --system --uid 10001 --create-home \
        --home-dir /var/lib/veoveo veoveo \
    && chage --lastday "$(date -u --date=@${SOURCE_DATE_EPOCH} +%F)" veoveo

# Install only the locked runner additions. The immutable parent supplies the
# compatible vLLM, Torch, Transformers, and CUDA stack. Source edits reuse this layer.
# Propagate cache disabling to pip's isolated build-dependency subprocesses.
RUN --mount=type=bind,from=veoveo-image-assets,source=servers/reason-mcp/runner/pyproject.toml,target=/tmp/reason-package/pyproject.toml,readonly \
    --mount=type=bind,from=veoveo-image-assets,source=servers/reason-mcp/runner/uv.lock,target=/tmp/reason-package/uv.lock,readonly \
    uv export --project /tmp/reason-package --frozen --no-dev --extra gpu \
        --no-emit-project --output-file /tmp/reason-requirements.txt \
    && PIP_NO_CACHE_DIR=1 python3 -m pip install --no-compile --require-hashes -r /tmp/reason-requirements.txt \
    && rm /tmp/reason-requirements.txt

# Zip metadata and member order must not depend on the checkout's timestamps
# or directory enumeration. The executable archive has no build-time bytecode.
RUN --mount=type=bind,from=veoveo-image-assets,source=servers/reason-mcp/runner/src,target=/tmp/reason-source,readonly python3 - <<'PY'
import os
from pathlib import Path
import stat
import time
import zipfile

source = Path('/tmp/reason-source')
target = Path('/usr/local/bin/reason-runner')
stamp = time.gmtime(int(os.environ['SOURCE_DATE_EPOCH']))[:6]
members = {str(path.relative_to(source)): path.read_bytes() for path in source.rglob('*.py')}
members['__main__.py'] = b'from reason_runner.main import main\nmain()\n'
with target.open('wb') as output:
    output.write(b'#!/usr/bin/env python3\n')
    with zipfile.ZipFile(output, 'w') as archive:
        for name, content in sorted(members.items()):
            entry = zipfile.ZipInfo(name, date_time=stamp)
            entry.external_attr = (stat.S_IFREG | 0o444) << 16
            archive.writestr(entry, content, compress_type=zipfile.ZIP_DEFLATED, compresslevel=9)
target.chmod(0o555)
PY

COPY --from=veoveo-rust-artifacts /bin/reason-mcp /usr/local/bin/reason-mcp
ENV NVIDIA_DRIVER_CAPABILITIES=compute,utility,video
USER veoveo
RUN reason-mcp --help >/dev/null && reason-runner --help >/dev/null
EXPOSE 8803
ENTRYPOINT ["/usr/local/bin/reason-mcp"]
