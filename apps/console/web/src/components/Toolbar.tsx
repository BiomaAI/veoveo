import { Search, SlidersHorizontal } from "lucide-react";

export function Toolbar({
  query,
  setQuery,
  state,
  setState,
  placeholder
}: {
  query: string;
  setQuery: (value: string) => void;
  state: string;
  setState: (value: string) => void;
  placeholder: string;
}) {
  return (
    <div className="toolbar">
      <label className="search-control">
        <Search size={15} />
        <input value={query} onChange={(event) => setQuery(event.target.value)} placeholder={placeholder} />
      </label>
      <label className="filter-control">
        <SlidersHorizontal size={15} />
        <select value={state} onChange={(event) => setState(event.target.value)} aria-label="State filter">
          <option value="all">All states</option>
          <option value="running">Running</option>
          <option value="waiting">Waiting</option>
          <option value="succeeded">Succeeded</option>
          <option value="failed">Failed</option>
          <option value="private">Private</option>
          <option value="releasable">Releasable</option>
          <option value="released">Released</option>
        </select>
      </label>
    </div>
  );
}
