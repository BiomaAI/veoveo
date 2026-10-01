"""Hatch custom build hook for immutable packaged MCP documentation.

Load this file with Hatch's custom-hook path; it uses only Hatch and the stdlib.
"""
from hashlib import sha256
import json
from pathlib import Path
import re
from tempfile import TemporaryDirectory
from hatchling.builders.hooks.plugin.interface import BuildHookInterface


class DocumentBuildHook(BuildHookInterface):
    def initialize(self, version, build_data):
        package = self.config["package"]
        if not re.fullmatch(r"[a-z_][a-z0-9_]*(\.[a-z_][a-z0-9_]*)*", package):
            raise ValueError("document build hook requires a Python package name")
        manifest = {}
        includes = build_data.setdefault("force_include", {})
        destination = package.replace(".", "/")
        for doc_id, filename in (("agents", "AGENTS.md"), ("design", "DESIGN.md")):
            source = Path(self.root) / filename
            data = source.read_bytes()
            if not data.decode("utf-8").strip():
                raise ValueError(f"empty document: {filename}")
            manifest[doc_id] = sha256(data).hexdigest()
            includes[str(source)] = f"{destination}/{filename}"
        self._documents = TemporaryDirectory(prefix="veoveo-documents-")
        path = Path(self._documents.name) / "_documents.json"
        path.write_text(json.dumps(manifest, sort_keys=True), encoding="utf-8")
        includes[str(path)] = f"{destination}/_documents.json"

    def finalize(self, version, build_data, artifact_path):
        self._documents.cleanup()
