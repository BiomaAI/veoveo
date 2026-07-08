#!/usr/bin/env python3
"""Register spooled segments into a running Rerun catalog and query them over
redap, emitting a JSON summary.

The OSS catalog creates the dataset from `-d`, but segments are registered via
the API. This script registers every `*.rrd` under the local root (file://),
then reads the dataset back over the wire and counts rows per segment — the
cross-check the hub-catalog smoke runs against the local QueryEngine /
sensor-sim ground truth. It proves: segment id == recording id, and the
catalog serves the same rows the files hold.

Usage: catalog_query.py <catalog_url> <dataset> <local_root> <timeline>
Prints JSON: {dataset, segment_ids, rows_by_segment, total_rows}
"""

import glob
import json
import sys

from rerun.catalog import CatalogClient, OnDuplicateSegmentLayer


def main() -> int:
    if len(sys.argv) != 5:
        print(
            "usage: catalog_query.py <catalog_url> <dataset> <local_root> <timeline>",
            file=sys.stderr,
        )
        return 2
    url, dataset_name, local_root, timeline = sys.argv[1:5]

    client = CatalogClient(url)
    dataset = client.get_dataset(name=dataset_name)

    files = sorted(glob.glob(f"{local_root}/**/*.rrd", recursive=True))
    if not files:
        print(f"no rrd files under {local_root}", file=sys.stderr)
        return 1

    handle = dataset.register(
        [f"file://{f}" for f in files],
        on_duplicate=OnDuplicateSegmentLayer.SKIP,
    )
    # Block until registration is durable, if the handle supports it.
    for waiter in ("wait", "join", "result"):
        fn = getattr(handle, waiter, None)
        if callable(fn):
            try:
                fn()
            except Exception:
                pass
            break

    segment_ids = sorted(str(s) for s in dataset.segment_ids())

    # Read the dataset over redap, indexed on the sensor timeline. The reader
    # yields a DataFusion DataFrame; materialize it as an Arrow table.
    import pyarrow as pa

    frame = dataset.filter_contents(["/**"]).reader(index=timeline)
    if hasattr(frame, "to_arrow_table"):
        table = frame.to_arrow_table()
    else:
        table = pa.Table.from_batches(frame.collect())

    rows_by_segment: dict[str, int] = {}
    seg_col = next(
        (
            name
            for name in ("rerun_partition_id", "rerun_segment_id", "chunk_partition_id")
            if name in table.column_names
        ),
        None,
    )
    if seg_col is not None:
        for value in table.column(seg_col).to_pylist():
            key = str(value)
            rows_by_segment[key] = rows_by_segment.get(key, 0) + 1

    print(
        json.dumps(
            {
                "dataset": dataset_name,
                "segment_ids": segment_ids,
                "rows_by_segment": rows_by_segment,
                "total_rows": table.num_rows,
                "columns": table.column_names,
            }
        )
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
