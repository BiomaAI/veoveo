import { useMemo, useState } from "react";
import { SectionHeader } from "../components/primitives";
import { Toolbar } from "../components/Toolbar";
import { ArtifactTable } from "../components/ArtifactTable";
import type { ArtifactSummary } from "../types";

export function ArtifactsView({
  artifacts,
  onSelect
}: {
  artifacts: ArtifactSummary[];
  onSelect: (artifact: ArtifactSummary) => void;
}) {
  const [query, setQuery] = useState("");
  const [state, setState] = useState("all");
  const rows = useMemo(
    () =>
      artifacts.filter(
        (artifact) =>
          (state === "all" || artifact.releaseState === state) &&
          `${artifact.id} ${artifact.filename} ${artifact.owner} ${artifact.labels.join(" ")}`
            .toLowerCase()
            .includes(query.toLowerCase())
      ),
    [artifacts, query, state]
  );
  return (
    <section className="panel full-panel">
      <SectionHeader
        title="Artifacts"
        count={rows.length}
        actions={<Toolbar query={query} setQuery={setQuery} state={state} setState={setState} placeholder="Search artifacts" />}
      />
      <ArtifactTable artifacts={rows} onSelect={onSelect} />
    </section>
  );
}
