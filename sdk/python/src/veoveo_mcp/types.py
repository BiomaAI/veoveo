"""Protocol-independent checked names, URI components and owner interfaces."""
from __future__ import annotations

import re
from dataclasses import dataclass
from enum import Enum, unique
from typing import Protocol, Self
from urllib.parse import parse_qsl, unquote

import rfc3986
from rfc3986.exceptions import RFC3986Exception
from rfc3986.validators import Validator
from uri_template import URITemplate
from uri_template import ExpansionInvalidError, ExpansionReservedError, VariableInvalidError
from pydantic_core import core_schema


class CheckedText(str):
    """Nominal string with constructor and Pydantic validation at ingress."""

    def __new__(cls, value: str) -> Self:
        if not isinstance(value, str):
            raise TypeError(f"{cls.__name__} requires text")
        cls._validate(value)
        return super().__new__(cls, value)

    @classmethod
    def _validate(cls, value: str) -> None:
        raise NotImplementedError

    @classmethod
    def __get_pydantic_core_schema__(cls, _source, _handler):
        return core_schema.no_info_after_validator_function(cls, core_schema.str_schema(strict=True))


class ScopeName(CheckedText):
    """External scope name; parsing does not establish a known owner scope."""

    @classmethod
    def _validate(cls, value: str) -> None:
        if (not value or value.strip() != value or len(value.encode()) > 256
                or any(ord(ch) < 32 or ord(ch) == 127 for ch in value)):
            raise ValueError("scope name must be trimmed, printable and 1..256 bytes")


class ScopeDefinition(Protocol):
    def scope_name(self) -> ScopeName: ...


class ScopeEnum(Enum):
    """Base for closed owner vocabularies, checked when the enum is defined."""

    def __init__(self, value: str) -> None:
        self._scope = ScopeName(value)
        if any(not (code == 0x21 or 0x23 <= code <= 0x5B or 0x5D <= code <= 0x7E)
               for code in map(ord, value)):
            raise ValueError("declared scope must follow OAuth scope-token syntax")

    def scope_name(self) -> ScopeName:
        return self._scope

    def __init_subclass__(cls, **kwargs) -> None:
        super().__init_subclass__(**kwargs)
        unique(cls)


class ResourceScheme(CheckedText):
    @classmethod
    def _validate(cls, value: str) -> None:
        if not re.fullmatch(r"[a-z][a-z0-9+.-]*", value):
            raise ValueError("resource scheme requires lowercase RFC 3986 spelling")


class UriAuthority(CheckedText):
    @classmethod
    def _validate(cls, value: str) -> None:
        # Domain resource authorities name a route or an unescaped identity.
        reference = rfc3986.URIReference("example", value, None, None, None)
        try:
            Validator().require_presence_of("host").check_validity_of("host", "port", "userinfo").validate(reference)
        except (RFC3986Exception, ValueError) as error:
            raise ValueError("invalid resource authority") from error
        if not value or reference.host != value or "%" in value:
            raise ValueError("resource authority cannot contain credentials, ports or escapes")


class UriSegment(CheckedText):
    @classmethod
    def _validate(cls, value: str) -> None:
        if not value or value in {".", ".."} or any(ord(ch) < 32 or ord(ch) == 127 for ch in value):
            raise ValueError("resource path segment must be nonempty and cannot be a dot segment or contain controls")


def _reference(value: str):
    reference = rfc3986.uri_reference(value)
    try:
        Validator().require_presence_of("scheme").check_validity_of(
            "scheme", "host", "port", "userinfo", "path", "query", "fragment"
        ).validate(reference)
    except (RFC3986Exception, ValueError) as error:
        raise ValueError("invalid absolute resource URI") from error
    # The parser encodes illegal characters; admission must preserve supplied bytes.
    if reference.unsplit() != value or reference.authority is None:
        raise ValueError("resource URI must be an encoded absolute hierarchical URI")
    if not reference.authority and not reference.path:
        raise ValueError("resource URI requires an authority or path")
    ResourceScheme(reference.scheme)
    return reference


class ResourceUri(CheckedText):
    """Concrete RFC 3986 reference. Owners apply their route and ID constraints."""

    @classmethod
    def _validate(cls, value: str) -> None:
        _reference(value)

    def components(self) -> ResourceUriParts:
        reference = _reference(self)
        if reference.fragment is not None:
            raise ValueError("domain resource addresses cannot contain fragments")
        authority = UriAuthority(reference.authority)
        path = reference.path or ""
        if path and not path.startswith("/"):
            raise ValueError("resource path must be absolute")
        segments = tuple(UriSegment(unquote(part, errors="strict")) for part in path.split("/")[1:])
        pairs = parse_qsl(reference.query or "", keep_blank_values=True, strict_parsing=True,
                          encoding="utf-8", errors="strict")
        if (len(pairs) != len(dict(pairs)) or any(not name for name, _ in pairs)
                or any(ord(ch) < 32 or ord(ch) == 127 for pair in pairs for value in pair for ch in value)):
            raise ValueError("resource query requires unique nonempty names and no controls")
        return ResourceUriParts(ResourceScheme(reference.scheme), authority, segments, tuple(pairs))


class ResourceTemplateUri(CheckedText):
    """RFC 6570 declaration, distinct from a concrete resource reference."""

    @classmethod
    def _validate(cls, value: str) -> None:
        try:
            template = URITemplate(value)
            variables = tuple(template.variables)
            if any(variable.default is not None or variable.array or variable.name != variable.key
                   for variable in variables):
                raise ValueError("URI template extensions are outside RFC 6570")
            # Require a declared scheme; it cannot depend on expansion arguments.
            ResourceScheme(value.partition(":")[0])
            ResourceUri(template.expand(**{name: "value" for name in template.variable_names}))
        except (TypeError, ValueError, ExpansionInvalidError, ExpansionReservedError, VariableInvalidError) as error:
            raise ValueError("invalid absolute RFC 6570 resource template") from error

    def expand(self, **values: str | list[str] | dict[str, str] | None) -> ResourceUri:
        template = URITemplate(self)
        if set(values) - set(template.variable_names):
            raise ValueError("unknown resource template variable")
        return ResourceUri(template.expand(**values))


@dataclass(frozen=True)
class ResourceUriParts:
    scheme: ResourceScheme
    authority: UriAuthority
    segments: tuple[UriSegment, ...]
    query: tuple[tuple[str, str], ...]


@dataclass(frozen=True)
class ResourceUriBuilder:
    """RFC 6570 owns component encoding; server builders supply typed identities."""

    scheme: ResourceScheme
    authority: UriAuthority
    segments: tuple[UriSegment, ...] = ()
    query: tuple[tuple[str, str], ...] = ()

    def __post_init__(self) -> None:
        if not isinstance(self.scheme, ResourceScheme) or not isinstance(self.authority, UriAuthority):
            raise TypeError("resource builders require checked scheme and authority types")
        if not isinstance(self.segments, tuple) or any(not isinstance(segment, UriSegment) for segment in self.segments):
            raise TypeError("resource builders require checked path segments")
        if not isinstance(self.query, tuple) or any(
            not isinstance(pair, tuple) or len(pair) != 2
            or any(not isinstance(value, str) for value in pair) for pair in self.query
        ):
            raise TypeError("resource query requires immutable pairs of text")
        if len(dict(self.query)) != len(self.query) or any(not name for name, _ in self.query):
            raise ValueError("resource query names must be nonempty and unique")

    def segment(self, value: UriSegment) -> Self:
        return type(self)(self.scheme, self.authority, (*self.segments, value), self.query)

    def query_pair(self, name: str, value: str) -> Self:
        if not isinstance(name, str) or not isinstance(value, str):
            raise TypeError("resource query components require text")
        if not name or name in dict(self.query):
            raise ValueError("resource query names must be nonempty and unique")
        return type(self)(self.scheme, self.authority, self.segments, (*self.query, (name, value)))

    def build(self) -> ResourceUri:
        template = URITemplate("{scheme}://{+authority}{/segments*}{?query*}")
        uri = ResourceUri(template.expand(scheme=self.scheme, authority=self.authority,
                                         segments=list(self.segments), query=dict(self.query)))
        uri.components()
        return uri


class ResourceAddress(Protocol):
    """Implemented in the owner library; syntax alone grants no resource access."""

    def to_uri(self) -> ResourceUri: ...
