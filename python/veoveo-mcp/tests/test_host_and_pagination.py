import pytest

from veoveo_mcp.deployment import DeploymentError, PublicDeployment, public_allowed_hosts
from veoveo_mcp.host import (
    HostAuthority,
    host_authority_is_allowed,
    parse_allowed_host_authority,
    parse_request_host_authority,
)
from veoveo_mcp.pagination import PaginationError, paginate


def test_request_authority_parsing_is_strict():
    assert parse_request_host_authority("127.0.0.1:8788") == HostAuthority(
        "127.0.0.1", 8788
    )
    assert parse_request_host_authority("veoveo.example") == HostAuthority(
        "veoveo.example", None
    )
    assert parse_request_host_authority("[::1]:8788") == HostAuthority("::1", 8788)
    assert parse_request_host_authority("[::1]") == HostAuthority("::1", None)
    assert parse_request_host_authority("") is None
    assert parse_request_host_authority("127.0.0.1:not-a-port") is None
    assert parse_request_host_authority("127.0.0.1:") is None
    assert parse_request_host_authority("127.0.0.1:999999") is None
    assert parse_request_host_authority("::1") is None
    assert parse_request_host_authority("[::1") is None
    assert parse_request_host_authority("[::1]:not-a-port") is None
    assert parse_request_host_authority("veoveo.example/path") is None
    assert parse_request_host_authority("veoveo.example?x=1") is None
    assert parse_request_host_authority("user@veoveo.example") is None
    assert parse_request_host_authority("veoveo .bioma.ai") is None


def test_allowed_authority_with_port_requires_same_port():
    allowed = ["veoveo.example:8443"]
    assert host_authority_is_allowed(HostAuthority("veoveo.example", 8443), allowed)
    assert not host_authority_is_allowed(HostAuthority("veoveo.example", 443), allowed)
    assert not host_authority_is_allowed(HostAuthority("veoveo.example", None), allowed)


def test_allowed_authority_without_port_allows_any_port():
    allowed = ["127.0.0.1"]
    assert host_authority_is_allowed(HostAuthority("127.0.0.1", 18799), allowed)
    assert host_authority_is_allowed(HostAuthority("127.0.0.1", None), allowed)


def test_allowed_ipv6_literal_can_be_configured_without_brackets():
    assert parse_allowed_host_authority("::1") == HostAuthority("::1", None)


def test_public_host_allowlist_uses_loopback_only_when_explicit():
    deployment = PublicDeployment.new("https://veoveo.example")
    assert public_allowed_hosts(deployment, False) == ["veoveo.example"]
    assert public_allowed_hosts(deployment, True) == [
        "veoveo.example",
        "localhost",
        "127.0.0.1",
        "::1",
    ]


def test_public_deployment_mounts_servers_under_their_slug():
    deployment = PublicDeployment.new("https://veoveo.example")
    endpoint = deployment.server("datasheet")
    assert endpoint.mount_path == "/datasheet"
    assert endpoint.public_url == "https://veoveo.example/datasheet"
    assert endpoint.path("mcp") == "/datasheet/mcp"
    with pytest.raises(DeploymentError):
        PublicDeployment.new("veoveo.example")
    with pytest.raises(DeploymentError):
        PublicDeployment.new("https://veoveo.example/subpath")
    with pytest.raises(DeploymentError):
        deployment.server("bad slug")


def test_paginate_returns_next_cursor():
    page = paginate([1, 2, 3], None, 2)
    assert page.items == [1, 2]
    assert page.next_cursor == "v1:2"


def test_paginate_uses_cursor():
    page = paginate([1, 2, 3], "v1:2", 2)
    assert page.items == [3]
    assert page.next_cursor is None


def test_paginate_rejects_unknown_cursor_shape():
    with pytest.raises(PaginationError):
        paginate([1, 2, 3], "2", 2)
    with pytest.raises(PaginationError):
        paginate([1], None, 0)
