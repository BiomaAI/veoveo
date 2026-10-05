"""Cached package assets and the Task owner's finite statement selections."""

from enum import Enum
from functools import cache
from importlib.resources import files


@cache
def query(name: str) -> str:
    return files(__package__).joinpath(name).read_text(encoding="utf-8")


class OwnerStatement(Enum):
    GET = "get"
    PAGE = "page"
    INPUTS = "inputs"
    CURRENT = "current"
    USAGE_PAGE = "usage_page"
    USAGE_GET = "usage_get"
    USAGE_COMPLETE = "usage_complete"
    INPUT_RESPONSES = "input_responses"
    TRANSITION = "transition"


def owner_statement(
    kind: OwnerStatement, *, context: bool, types: bool, after: bool = False,
) -> str:
    suffix = ("_context" if context else "") + ("_types" if types else "")
    if after:
        if kind not in (OwnerStatement.PAGE, OwnerStatement.USAGE_PAGE):
            raise ValueError("Only page statements accept an after cursor")
        suffix += "_after"
    return query(f"owner/{kind.value}{suffix}.surql")
