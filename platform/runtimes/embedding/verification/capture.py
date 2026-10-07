"""Current Knowledge capture admission before CUDA comparison or model loading.

These peer values admit the Rust producer's current format. Synthetic receiver
controls establish no embedding or hardware qualification.
"""
from __future__ import annotations

import hashlib
from ipaddress import IPv6Address
import json
import math
import re
from pathlib import Path
from typing import Annotated, Literal

from rfc3986.misc import URI_MATCHER
from rfc3986_validator import URI_RE_COMP

from pydantic import (
    AfterValidator, BaseModel, ConfigDict, Field, StrictFloat, StrictInt, StrictStr,
    StringConstraints, ValidationError, model_validator,
)

# The upstream grammar writes IPvFuture's v literally; RFC ABNF literals are
# ASCII case-insensitive. This changes syntax matching only, never the raw URI.
_URI_GRAMMAR = re.compile(URI_RE_COMP.pattern, (URI_RE_COMP.flags & ~re.UNICODE) | re.ASCII | re.IGNORECASE)

MAX_CAPTURE_BYTES = 512 * 1024 * 1024
Digest = Annotated[StrictStr, StringConstraints(pattern=r"^sha256:[a-f0-9]{64}$")]
def identity(value: str) -> str:
    if (len(value.encode()) > 256 or value.strip() != value
        or any(ord(c) < 32 or 127 <= ord(c) <= 159 for c in value)):
        raise ValueError("invalid execution identity")
    return value


Identity = Annotated[StrictStr, StringConstraints(min_length=1, max_length=256), AfterValidator(identity)]
Precision = Literal["float32", "float16", "bfloat16", "int8", "int4"]
Finite = Annotated[StrictFloat, Field(allow_inf_nan=False)]
Index = Annotated[StrictInt, Field(ge=0)]


class Peer(BaseModel):
    model_config = ConfigDict(strict=True, extra="forbid", hide_input_in_errors=True)

    @model_validator(mode="before")
    @classmethod
    def current_members(cls, value):
        if isinstance(value, dict):
            unknown = set(value) - set(cls.model_fields)
            if unknown:
                raise ValidationError.from_exception_data(cls.__name__, [
                    {"type": "extra_forbidden", "loc": (key,), "input": None}
                    for key in sorted(unknown)
                ])
        return value


class Space(Peer):
    model: Identity
    revision: Identity
    dimension: Annotated[StrictInt, Field(ge=1, le=8192)]
    pooling: Literal["last_token", "mean", "cls"]
    normalization: Literal["l2"]
    precision: Precision
    maxInputTokens: Annotated[StrictInt, Field(ge=1, le=131072)]


class Environment(Peer):
    gpu: Identity
    driver: Identity
    cuda: Identity


class Serving(Peer):
    precision: Precision
    scheduling: Literal["priority"]
    graphAllowance: Literal["graphs_allowed", "enforce_eager"]
    observedGraphExecution: Literal["eager", "cuda_graph"]
    attentionBackend: Identity
    explicitKvCacheBytes: Annotated[StrictInt, Field(ge=1)] | None
    maxInputTokens: Annotated[StrictInt, Field(ge=1, le=131072)]
    maxNumBatchedTokens: Annotated[StrictInt, Field(ge=1, le=1048576)]
    maxNumSequences: Annotated[StrictInt, Field(ge=1, le=1024)]
    gpuMemoryBasisPoints: Annotated[StrictInt, Field(ge=1, le=10000)]


class Contents(Peer):
    space: Space
    runtimeImage: Digest
    checkpointManifest: Digest
    vllmVersion: Identity
    environment: Environment
    serving: Serving

    @model_validator(mode="after")
    def admitted(self):
        if (not self.environment.gpu.startswith("NVIDIA ")
            or self.serving.precision != self.space.precision
            or self.serving.maxInputTokens != self.space.maxInputTokens
            or (self.serving.graphAllowance == "enforce_eager"
                and self.serving.observedGraphExecution != "eager")):
            raise ValueError("profile relationships invalid")
        return self


def compact(value: object) -> bytes:
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"), allow_nan=False).encode()


def fingerprint(value: object) -> str:
    return "sha256:" + hashlib.sha256(compact(value)).hexdigest()


class Profile(Peer):
    id: Digest
    contents: Contents

    @model_validator(mode="after")
    def admitted(self):
        domain = b"veoveo.ai/embedding-execution-profile/v1"
        body = compact(self.contents.model_dump())
        framed = len(domain).to_bytes(8, "big") + domain + len(body).to_bytes(8, "big") + body
        expected = "sha256:" + hashlib.sha256(framed).hexdigest()
        if self.id != expected:
            raise ValueError("profile identity mismatch")
        return self


class Chunking(Peer):
    version: Annotated[StrictStr, StringConstraints(min_length=1, max_length=128)]
    maxCharacters: Annotated[StrictInt, Field(ge=1, le=8192)]
    overlapCharacters: Annotated[StrictInt, Field(ge=0)]

    @model_validator(mode="after")
    def admitted(self):
        if self.overlapCharacters >= self.maxCharacters or any(ord(c) < 32 or 127 <= ord(c) <= 159 for c in self.version):
            raise ValueError("chunk settings invalid")
        return self


class MemberIdentity(Peer):
    collection: Annotated[StrictStr, StringConstraints(pattern=r"^[a-z][a-z0-9-]*\.[a-z][a-z0-9-]*$")]
    uri: StrictStr

    @model_validator(mode="after")
    def admitted(self):
        if any(len(part.encode()) > 128 for part in self.collection.split(".")):
            raise ValueError("collection component exceeds owner limit")
        # The whole syntax validator owns RFC 3986, while the existing parser's
        # raw component matcher preserves empty authority/query/fragment markers.
        # URIReference.from_string/unsplit would erase those markers or repair text.
        if (not self.uri.isascii()
            or any(ord(c) < 32 or ord(c) == 127 for c in self.uri)
            or not _URI_GRAMMAR.fullmatch(self.uri)):
            raise ValueError("invalid resource URI")
        parts = URI_MATCHER.fullmatch(self.uri)
        if parts is None:
            raise ValueError("invalid resource URI")
        scheme, authority, path = parts.group("scheme", "authority", "path")
        if (scheme is None or scheme != scheme.lower() or authority is None
            or (not authority and not path)):
            raise ValueError("resource URI requires the current scheme and hierarchy")
        # Whole RFC syntax already admits the authority and its brackets. The
        # upstream IPv6 grammar accepts zero-prefixed embedded IPv4 octets that
        # ResourceUri refuses. Validate only this admitted bracketed IPv6 literal;
        # IPvFuture and digit/dot registered names retain their URI grammar.
        if "[" in authority:
            literal = authority[authority.index("[") + 1:authority.index("]")]
            if not literal.startswith(("v", "V")):
                try:
                    IPv6Address(literal)
                except ValueError:
                    raise ValueError("invalid resource URI") from None
        return self


class Member(Peer):
    member: MemberIdentity
    revision: Annotated[StrictStr, StringConstraints(pattern=r"^[!-~]{1,256}$")]
    contentSha256: Digest


Text = Annotated[StrictStr, StringConstraints(min_length=1, max_length=16384)]
Vector = Annotated[list[Finite], Field(min_length=1, max_length=8192)]


class Chunk(Peer):
    member: Index
    text: Text
    values: Vector


class Query(Peer):
    id: Annotated[StrictStr, StringConstraints(pattern=r"^[a-z0-9-]{1,64}$")]
    text: Text
    values: Vector
    selectedMembers: Annotated[list[Index], Field(min_length=1, max_length=65536)]
    relevantMembers: Annotated[list[Index], Field(min_length=1, max_length=65536)]


class Capture(Peer):
    format: Literal["veoveo.ai/embedding-candidate-capture/v1"]
    profile: Profile
    queryTask: Annotated[StrictStr, StringConstraints(min_length=1, max_length=1024)]
    chunking: Chunking
    datasetRevision: Digest
    sourceCorpusRevision: Digest
    queryTaskRevision: Digest
    chunkingRevision: Digest
    members: Annotated[list[Member], Field(min_length=1, max_length=65536)]
    chunks: Annotated[list[Chunk], Field(min_length=1, max_length=262144)]
    queries: Annotated[list[Query], Field(min_length=1, max_length=1024)]
    minimumRecallAtTen: Annotated[Finite, Field(gt=0, le=1)]

    @model_validator(mode="after")
    def admitted(self):
        if (len(self.queryTask.encode()) > 1024 or not self.queryTask.strip()
            or any(ord(c) < 32 or 127 <= ord(c) <= 159 for c in self.queryTask)
            or self.queryTaskRevision != fingerprint(self.queryTask)
            or self.chunkingRevision != fingerprint(self.chunking.model_dump())):
            raise ValueError("measurement context mismatch")
        identities = [(item.member.collection, item.member.uri) for item in self.members]
        if len(set(identities)) != len(identities) or len({query.id for query in self.queries}) != len(self.queries):
            raise ValueError("repeated measurement identity")
        count = len(self.members)
        dimension = self.profile.contents.space.dimension
        populated = set()
        for chunk in self.chunks:
            if chunk.member >= count:
                raise ValueError("chunk member index invalid")
            populated.add(chunk.member)
        for row in [*self.chunks, *self.queries]:
            if (len(row.text.encode()) > 16384 or not row.text.strip()
                or len(row.values) != dimension
                or abs(math.fsum(value * value for value in row.values) - 1) > .002):
                raise ValueError("measurement text or vector invalid")
        for query in self.queries:
            selected, relevant = query.selectedMembers, query.relevantMembers
            if (len(set(selected)) != len(selected) or len(set(relevant)) != len(relevant)
                or any(index >= count for index in selected + relevant)
                or not set(relevant) <= set(selected) or not set(selected) <= populated):
                raise ValueError("measurement selection invalid")
        return self


def read_capture(path: Path) -> tuple[bytes, Capture]:
    """Bound the read itself, including files that grow after an earlier stat."""
    with path.open("rb") as source:
        raw = source.read(MAX_CAPTURE_BYTES + 1)
    if len(raw) > MAX_CAPTURE_BYTES:
        raise RuntimeError("candidate capture exceeds 512 MiB")
    try:
        capture = Capture.model_validate_json(raw)
    except (ValidationError, ValueError):
        raise RuntimeError("invalid current candidate capture") from None
    return raw, capture
