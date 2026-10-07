"""Real isolated Hatch wheels, then installed loading with no repository lookup."""
import json
from hashlib import sha256
from pathlib import Path
import subprocess
import sys
from zipfile import ZipFile

import pytest

ROOT = Path(__file__).resolve().parents[3]


@pytest.mark.parametrize("owner,package,server", [
    ("templates/python-mcp", "datasheet_mcp", "datasheet"),
    ("testing/fixtures/fork-workload", "anonymous_simulation_mcp", "anonymous-simulation"),
])
def test_isolated_hatch_wheels_load_exact_profile_and_manual_without_source(tmp_path, owner, package, server):
    # uv's isolated build installs Hatch only; the hook must not import SDK dependencies.
    for name, root in (("sdk", ROOT / "sdk/python"), ("owner", ROOT / owner)):
        output = tmp_path / name
        result = subprocess.run(["uv", "build", "--wheel", "--out-dir", str(output), str(root)],
                                capture_output=True, text=True, timeout=120)
        assert result.returncode == 0, result.stderr[-4000:]
        wheel, = output.glob("*.whl")
        with ZipFile(wheel) as archive:
            archive.extractall(output / "installed")
    sdk_path = tmp_path / "sdk/installed"
    owner_path = tmp_path / "owner/installed"
    manual = (ROOT / owner / "AGENTS.md").read_bytes()
    script = """
import json,sys
from pathlib import Path
repo,sdk,owner,package,server = sys.argv[1:]
# Third-party packages in a repository-managed virtualenv are dependencies,
# not owner source. Preserve those while excluding every source-tree import.
sys.path[:] = [sdk,owner] + [p for p in sys.path
    if p and (repo not in str(p) or "site-packages" in Path(p).parts)]
from veoveo_mcp.contract import server_docs,ContractDeclaration
import veoveo_mcp
assert repo not in veoveo_mcp.__file__
docs=server_docs(server,package)
print(json.dumps({'manual':docs.agent_manual(),'declaration':ContractDeclaration.from_docs(docs).wire()}))
"""
    def load():
        return subprocess.run([sys.executable, "-c", script, str(ROOT), str(sdk_path), str(owner_path), package, server],
                              cwd=tmp_path, capture_output=True, text=True, timeout=30)
    result = load()
    assert result.returncode == 0, result.stderr[-4000:]
    data = json.loads(result.stdout)
    assert data["manual"].encode() == manual
    assert data["declaration"] == json.loads((ROOT / owner / "contract-compliance.json").read_bytes())
    profile_path = owner_path / package / "contract-compliance.json"
    profile = json.loads(profile_path.read_bytes())
    profile["compliance"][0]["note"] = "Changed valid owner explanation"
    profile_path.write_text(json.dumps(profile))
    assert load().returncode != 0
    manifest_path = owner_path / package / "_documents.json"
    manifest = json.loads(manifest_path.read_bytes())
    manifest["profile"] = sha256(profile_path.read_bytes()).hexdigest()
    manifest_path.write_text(json.dumps(manifest))
    # Valid replacement bytes/digests still cannot authorize a stale served manual.
    assert load().returncode != 0
