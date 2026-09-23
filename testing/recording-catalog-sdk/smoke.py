"""Exercise an installed Veoveo catalog through the pinned Rerun Python SDK."""

from __future__ import annotations

import argparse
import json
import sys
from datetime import datetime, timezone
from urllib.parse import urlparse

import rerun as rr


def validate_grant(result: dict, dataset: str, recording: str) -> None:
    if result["schema"] != "veoveo.io/recording-catalog-grant/v1":
        raise ValueError("unexpected recording grant schema")
    if result["dataset_id"] != dataset:
        raise ValueError("recording grant changed the dataset")
    if result["recording_segment_ids"] != [recording]:
        raise ValueError("recording grant changed the admitted segment set")
    if datetime.fromisoformat(result["expires_at"].replace("Z", "+00:00")) <= datetime.now(
        timezone.utc
    ):
        raise ValueError("recording grant has already expired")


def query(redap_url: str, issued_grant: dict, recording: str) -> tuple[str, int]:
    entry_id = urlparse(issued_grant["entry_uri"]).path.rsplit("/", 1)[-1]
    client = rr.catalog.CatalogClient(redap_url, token=issued_grant["redap_token"])
    dataset = client.get_dataset(id=entry_id)
    if recording not in dataset.segment_ids():
        raise ValueError("admitted recording is absent from the Redap dataset")
    indexes = [column.name for column in dataset.schema().index_columns()]
    timeline = "log_time" if "log_time" in indexes else (indexes[0] if indexes else None)
    dataframe = dataset.reader(index=timeline).limit(16)
    rows = sum(batch.num_rows for batch in dataframe.collect())
    if rows == 0:
        raise ValueError("Rerun Catalog SDK returned no recording rows")
    return timeline or "static", rows


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--redap-url", required=True)
    parser.add_argument("--dataset-id", required=True)
    parser.add_argument("--recording-id", required=True)
    args = parser.parse_args()
    grants = json.load(sys.stdin)
    first = grants["first"]
    renewed = grants["renewed"]
    validate_grant(first, args.dataset_id, args.recording_id)
    validate_grant(renewed, args.dataset_id, args.recording_id)
    timeline, rows = query(args.redap_url, first, args.recording_id)
    renewed_timeline, renewed_rows = query(args.redap_url, renewed, args.recording_id)
    if timeline != renewed_timeline or rows != renewed_rows:
        raise ValueError("renewed grant changed the immutable recording query")
    print(json.dumps({"timeline": timeline, "rows": rows, "renewed": True}))


if __name__ == "__main__":
    main()
