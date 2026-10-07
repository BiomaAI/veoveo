"""Catalog-bound profile admission and rendering; stdlib only for isolated Hatch builds."""
from __future__ import annotations

from dataclasses import dataclass
from enum import Enum
import json
import re
from pathlib import Path
from typing import Mapping

CONTRACT_REVISION = 4
CATALOG_REVISION = 2
START_MARKER = "<!-- veoveo:contract-compliance:start -->"
END_MARKER = "<!-- veoveo:contract-compliance:end -->"



class ProfileError(ValueError):
    """Image/package-owned contract data failed admission."""


def decode_json(data: bytes | str):
    def unique(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                raise ProfileError(f"duplicate JSON field: {key}")
            result[key] = value
        return result
    try:
        if isinstance(data, bytes):
            data = data.decode("utf-8")
        return json.loads(data, object_pairs_hook=unique,
                          parse_constant=lambda value: (_ for _ in ()).throw(ProfileError("non-JSON number")))
    except (ValueError, UnicodeError) as error:
        raise ProfileError("invalid contract JSON") from error


def fields(value, required: set[str], optional: set[str] = frozenset()):
    if not isinstance(value, dict) or not required <= value.keys() or value.keys() - required - optional:
        raise ProfileError("invalid contract object fields")


@dataclass(frozen=True)
class Requirement:
    id: str
    level: str
    text: str
    applicability: str | None


class RequirementCatalog:
    def __init_subclass__(cls, **kwargs):
        raise TypeError("requirement catalogs cannot bypass admission through subclassing")

    __slots__ = ("requirements", "ids", "note_whitespace")

    def __init__(self, data: bytes | str):
        value = decode_json(data)
        fields(value, {"contractRevision", "catalogRevision", "requirements", "noteWhitespace"})
        if type(value["contractRevision"]) is not int or value["contractRevision"] != CONTRACT_REVISION:
            raise ProfileError("unsupported contract revision")
        if type(value["catalogRevision"]) is not int or value["catalogRevision"] != CATALOG_REVISION:
            raise ProfileError("unsupported catalog revision")
        canonical = decode_json(Path(__file__).with_name("catalog").joinpath("requirements.json").read_bytes())
        if value != canonical:
            raise ProfileError("catalog differs from the generated revision; upgrade the consumer")
        if not isinstance(value["requirements"], list) or not value["requirements"]:
            raise ProfileError("empty requirement catalog")
        whitespace = value["noteWhitespace"]
        if not isinstance(whitespace, str) or not whitespace:
            raise ProfileError("missing generated note whitespace rule")
        object.__setattr__(self, "note_whitespace", whitespace)
        requirements = []
        for entry in value["requirements"]:
            fields(entry, {"id", "level", "text"}, {"applicability"})
            if not isinstance(entry["id"], str) or not re.fullmatch(r"C\d{2}", entry["id"]):
                raise ProfileError("invalid requirement identity")
            if any(not isinstance(entry[key], str) or not entry[key].strip() for key in ("level", "text")):
                raise ProfileError("empty requirement metadata")
            condition = entry.get("applicability")
            if condition is not None and condition != "knowledge_source":
                raise ProfileError("unsupported applicability condition")
            requirements.append(Requirement(entry["id"], entry["level"], entry["text"], condition))
        ids = tuple(item.id for item in requirements)
        if len(set(ids)) != len(ids) or ids != tuple(sorted(ids)):
            raise ProfileError("duplicate or unordered requirement catalog")
        object.__setattr__(self, "requirements", tuple(requirements))
        object.__setattr__(self, "ids", ids)

    def __setattr__(self, name, value):
        raise AttributeError("requirement catalogs are immutable")


class ComplianceStatus(str, Enum):
    MET = "met"
    PENDING = "pending"
    NOT_APPLICABLE = "not_applicable"


@dataclass(frozen=True)
class ComplianceItem:
    id: str
    status: ComplianceStatus
    note: str | None = None

    def __post_init__(self):
        if not isinstance(self.id, str) or not re.fullmatch(r"C\d{2}", self.id):
            raise ProfileError("invalid requirement identity")
        if not isinstance(self.status, ComplianceStatus):
            raise ProfileError("invalid requirement status")
        if self.note is not None and (not isinstance(self.note, str) or "\n" in self.note or "\r" in self.note):
            raise ProfileError("requirement notes must be strings without CR or LF")
        if self.status is not ComplianceStatus.MET and self.note is None:
            raise ProfileError("pending and not_applicable require a note")

    def wire(self):
        return {"id": self.id, "status": self.status.value, **({"note": self.note} if self.note is not None else {})}


class ComplianceProfile:
    def __init_subclass__(cls, **kwargs):
        raise TypeError("compliance profiles cannot bypass admission through subclassing")

    __slots__ = ("server", "compliance", "_catalog")

    def __init__(self, data: bytes | str | Mapping, catalog: RequirementCatalog):
        if not isinstance(catalog, RequirementCatalog):
            raise ProfileError("profile requires an admitted requirement catalog")
        if not isinstance(data, (bytes, str, Mapping)):
            raise ProfileError("profile requires a JSON object")
        value = decode_json(data) if isinstance(data, (bytes, str)) else dict(data)
        fields(value, {"server", "contractRevision", "catalogRevision", "compliance"})
        if not isinstance(value["server"], str) or not re.fullmatch(r"[a-z0-9_-]+", value["server"]):
            raise ProfileError("invalid server identity")
        if type(value["contractRevision"]) is not int or value["contractRevision"] != CONTRACT_REVISION:
            raise ProfileError("unsupported contract revision")
        if type(value["catalogRevision"]) is not int or value["catalogRevision"] != CATALOG_REVISION:
            raise ProfileError("unsupported catalog revision")
        if not isinstance(value["compliance"], list):
            raise ProfileError("invalid requirement entries")
        entries = {}
        conditions = {item.id: item.applicability for item in catalog.requirements}
        for entry in value["compliance"]:
            fields(entry, {"id", "status"}, {"note"})
            try:
                item = ComplianceItem(entry["id"], ComplianceStatus(entry["status"]), entry.get("note"))
            except (ValueError, TypeError) as error:
                raise ProfileError("invalid compliance entry") from error
            if item.note is not None and not item.note.strip(catalog.note_whitespace):
                raise ProfileError("requirement note must be nonblank")
            if "note" in entry and entry["note"] is None:
                raise ProfileError("present requirement note cannot be null")
            if item.id not in conditions or item.id in entries:
                raise ProfileError("unknown or duplicate requirement identity")
            if item.status is ComplianceStatus.NOT_APPLICABLE and conditions[item.id] is None:
                raise ProfileError("requirement has no applicability condition")
            entries[item.id] = item
        if set(entries) != set(catalog.ids):
            raise ProfileError("incomplete compliance profile")
        object.__setattr__(self, "server", value["server"])
        object.__setattr__(self, "compliance", tuple(entries[key] for key in catalog.ids))
        object.__setattr__(self, "_catalog", catalog)

    def __setattr__(self, name, value):
        raise AttributeError("compliance profiles are immutable")

    @property
    def contract_revision(self):
        return CONTRACT_REVISION

    @property
    def catalog_revision(self):
        return CATALOG_REVISION

    def wire(self):
        return {"server": self.server, "contractRevision": CONTRACT_REVISION,
                "catalogRevision": CATALOG_REVISION, "compliance": [item.wire() for item in self.compliance]}

    def check_applicability(self, *, knowledge_source: bool):
        if type(knowledge_source) is not bool:
            raise ProfileError("applicability requires actual discovery state")
        metadata = {item.id: item.applicability for item in self._catalog.requirements}
        for item in self.compliance:
            if metadata[item.id] == "knowledge_source" and (item.status is ComplianceStatus.NOT_APPLICABLE) == knowledge_source:
                raise ProfileError("knowledge-source applicability contradicts discovery")

    def render(self):
        return (f"Contract revision: {CONTRACT_REVISION}\nCatalog revision: {CATALOG_REVISION}\n\n" +
                "".join(f"- {item.id}: {item.status.value}" + (f" — {item.note}" if item.note is not None else "") + "\n" for item in self.compliance))

    def check_manual(self, manual: str):
        if manual.count(START_MARKER) != 1 or manual.count(END_MARKER) != 1:
            raise ProfileError("manual requires one compliance marker pair")
        if manual.index(END_MARKER) < manual.index(START_MARKER):
            raise ProfileError("reversed compliance markers")
        before, section = manual.split(START_MARKER)
        body, after = section.split(END_MARKER)
        if not before.endswith("## Contract Compliance\n\n"):
            raise ProfileError("compliance markers require Contract Compliance section")
        if body != "\n" + self.render() or START_MARKER in after:
            raise ProfileError("manual compliance section differs from admitted profile")
