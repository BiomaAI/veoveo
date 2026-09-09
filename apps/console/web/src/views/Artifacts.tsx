import { useMemo, useState } from "react";
import { SectionHeader } from "../components/primitives";
import { Toolbar } from "../components/Toolbar";
import { ArtifactTable } from "../components/ArtifactTable";
import type { ArtifactSummary } from "../types";

export function ArtifactsView({
  artifacts,
  onSelect,
  onUpload
}: {
  artifacts: ArtifactSummary[];
  onSelect: (artifact: ArtifactSummary) => void;
  onUpload: () => void;
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
        actions={<div className="upload-page-actions"><Toolbar query={query} setQuery={setQuery} state={state} setState={setState} states={["private", "releasable", "released"]} placeholder="Search artifacts" />
          <button className="button button-primary" onClick={onUpload}>Upload files</button></div>}
      />
      {rows.length ? <ArtifactTable artifacts={rows} onSelect={onSelect} /> : <div className="upload-empty">
        <p>{artifacts.length ? "No artifacts match these filters." : "Your artifact library is empty."}</p>
        {artifacts.length ? <button className="button button-secondary" onClick={() => { setQuery(""); setState("all"); }}>Clear filters</button>
          : <button className="button button-primary" onClick={onUpload}>Upload files</button>}
      </div>}
    </section>
  );
}
