-- Distinguish successful operation completion from authorization decisions.

DEFINE FIELD OVERWRITE outcome ON TABLE audit_event TYPE
    "allowed" | "denied" | "failed" | "succeeded";
