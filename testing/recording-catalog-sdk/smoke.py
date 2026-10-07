"""Exercise an installed Veoveo catalog through the pinned Rerun Python SDK."""

from __future__ import annotations

import argparse
import json
import sys
from datetime import datetime, timezone
from urllib.parse import urlparse
from uuid import UUID
import unicodedata

import rerun as rr
from datafusion import col


def validate_grant(result: dict, dataset: str, recording: str) -> str:
    fields = {"schema", "grantId", "datasetId", "recordingSegmentIds", "catalogRevision", "entryUri", "redapToken", "expiresAt"}
    if not isinstance(result, dict) or set(result) != fields:
        raise ValueError("invalid current Recording catalog grant fields")
    if result["schema"] != "veoveo.ai/recording-catalog-grant/v2":
        raise ValueError("unexpected recording grant schema")
    if not all(isinstance(result[key], str) for key in fields - {"recordingSegmentIds"}):
        raise ValueError("invalid Recording catalog grant field types")
    if (not result["redapToken"].strip() or not result["catalogRevision"].strip()
        or len(result["catalogRevision"].encode()) > 128
        or any(unicodedata.category(char) == "Cc" for char in result["redapToken"] + result["catalogRevision"])):
        raise ValueError("invalid Recording catalog grant authority fields")
    if result["datasetId"] != dataset:
        raise ValueError("recording grant changed the dataset")
    if result["recordingSegmentIds"] != [recording]:
        raise ValueError("recording grant changed the admitted segment set")
    for value in [result["grantId"], dataset, recording]:
        admitted = UUID(value)
        if admitted.version != 7 or str(admitted) != value:
            raise ValueError("invalid current Recording catalog identity")
    dataset_uuid = UUID(dataset).hex
    entry_id = dataset_uuid[:16].upper() + dataset_uuid[16:]
    entry = urlparse(result["entryUri"])
    if (entry.scheme not in {"rerun", "rerun+http"} or not entry.hostname or not entry.port
        or entry.username or entry.password or entry.query or entry.fragment
        or entry.path != "/entry/" + entry_id):
        raise ValueError("Recording catalog address changed its selected dataset")
    expires = datetime.fromisoformat(result["expiresAt"].replace("Z", "+00:00"))
    if expires.utcoffset() is None or expires <= datetime.now(timezone.utc):
        raise ValueError("recording grant has already expired")
    return entry_id


def query(redap_url: str, issued_grant: dict, recording: str, entry_id: str) -> tuple[str, int]:
    client = rr.catalog.CatalogClient(redap_url, token=issued_grant["redapToken"])
    dataset = client.get_dataset(id=entry_id)
    if recording not in dataset.segment_ids():
        raise ValueError("admitted recording is absent from the Redap dataset")
    indexes = [column.name for column in dataset.schema().index_columns()]
    timeline = "log_time" if "log_time" in indexes else (indexes[0] if indexes else None)
    dataframe = (
        dataset.reader(index=timeline)
        .filter(col("rerun_segment_id") == recording)
        .limit(16)
        .to_pandas()
    )
    rows = len(dataframe)
    if rows == 0:
        raise ValueError("Rerun Catalog SDK returned no recording rows")
    if not dataframe["rerun_segment_id"].eq(recording).all():
        raise ValueError("Rerun Catalog SDK returned another recording's rows")
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
    first_entry = validate_grant(first, args.dataset_id, args.recording_id)
    renewed_entry = validate_grant(renewed, args.dataset_id, args.recording_id)
    timeline, rows = query(args.redap_url, first, args.recording_id, first_entry)
    renewed_timeline, renewed_rows = query(args.redap_url, renewed, args.recording_id, renewed_entry)
    if timeline != renewed_timeline or rows != renewed_rows:
        raise ValueError("renewed grant changed the immutable recording query")
    print(json.dumps({"timeline": timeline, "rows": rows, "renewed": True}))


if __name__ == "__main__":
    main()
