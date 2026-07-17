-- Separate capture lifecycle from publication lifecycle. Rows left open by
-- the former direct Rerun ingest path are interrupted until new data resumes
-- them; capture end and seal publication now have distinct timestamps.

DEFINE FIELD OVERWRITE state ON TABLE recording TYPE "open" | "live" | "ready" | "sealing" | "sealed" | "interrupted" | "failed";
DEFINE FIELD IF NOT EXISTS last_data_at ON TABLE recording TYPE datetime DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS sealed_at ON TABLE recording TYPE option<datetime>;

UPDATE recording SET
    last_data_at = updated_at,
    sealed_at = IF state = "sealed" THEN ended_at ELSE NONE END;

UPDATE recording SET
    state = "interrupted",
    ended_at = updated_at
WHERE state = "open";

DEFINE FIELD OVERWRITE state ON TABLE recording TYPE "live" | "ready" | "sealing" | "sealed" | "interrupted" | "failed";
