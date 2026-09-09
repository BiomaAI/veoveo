import { Search, SlidersHorizontal } from "lucide-react";

export function Toolbar({
  query,
  setQuery,
  state,
  setState,
  placeholder,
  states = ["running", "waiting", "succeeded", "failed", "private", "releasable", "released"]
}: {
  query: string;
  setQuery: (value: string) => void;
  state: string;
  setState: (value: string) => void;
  placeholder: string;
  states?: string[];
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
          {states.map((value) => <option key={value} value={value}>{value.charAt(0).toUpperCase() + value.slice(1)}</option>)}
        </select>
      </label>
    </div>
  );
}
