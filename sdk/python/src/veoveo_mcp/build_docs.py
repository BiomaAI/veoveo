"""Isolated Hatch hook: package exact owner bytes and the Rust-generated catalog."""
from hashlib import sha256
import importlib.util
import json
from pathlib import Path
import re
import sys
from tempfile import TemporaryDirectory
from hatchling.builders.hooks.plugin.interface import BuildHookInterface

# Loading this sibling deliberately avoids importing the SDK dependency graph.
_spec = importlib.util.spec_from_file_location("_veoveo_build_compliance", Path(__file__).with_name("_compliance.py"))
_compliance = importlib.util.module_from_spec(_spec)
sys.modules[_spec.name] = _compliance
_spec.loader.exec_module(_compliance)


class DocumentBuildHook(BuildHookInterface):
    def initialize(self, version, build_data):
        package = self.config["package"]
        if not re.fullmatch(r"[a-z_][a-z0-9_]*(\.[a-z_][a-z0-9_]*)*", package):
            raise ValueError("document build hook requires a Python package name")
        includes = build_data.setdefault("force_include", {})
        destination = package.replace(".", "/")
        catalog_root = Path(__file__).with_name("catalog")
        catalog = _compliance.RequirementCatalog((catalog_root / "requirements.json").read_bytes())
        schema = catalog_root / "compliance-profile.schema.json"
        if not isinstance(_compliance.decode_json(schema.read_bytes()), dict):
            raise ValueError("invalid generated compliance schema")
        if self.config.get("catalog_only", False):
            for filename in ("requirements.json", "compliance-profile.schema.json"):
                includes[str(catalog_root / filename)] = f"{destination}/catalog/{filename}"
            return
        root = Path(self.root)
        profile = _compliance.ComplianceProfile((root / "contract-compliance.json").read_bytes(), catalog)
        profile.check_manual((root / "AGENTS.md").read_bytes().decode("utf-8"))
        profile.check_applicability(knowledge_source=True)
        artifacts = {"agents": root / "AGENTS.md", "design": root / "DESIGN.md",
                     "profile": root / "contract-compliance.json", "catalog": catalog_root / "requirements.json",
                     "schema": schema}
        manifest = {}
        for key, source in artifacts.items():
            data = source.read_bytes()
            if not data.decode("utf-8").strip():
                raise ValueError(f"empty artifact: {source.name}")
            manifest[key] = sha256(data).hexdigest()
            includes[str(source)] = f"{destination}/{source.name}"
        self._documents = TemporaryDirectory(prefix="veoveo-documents-")
        path = Path(self._documents.name) / "_documents.json"
        path.write_text(json.dumps(manifest, sort_keys=True) + "\n", encoding="utf-8")
        includes[str(path)] = f"{destination}/_documents.json"

    def finalize(self, version, build_data, artifact_path):
        if hasattr(self, "_documents"):
            self._documents.cleanup()
