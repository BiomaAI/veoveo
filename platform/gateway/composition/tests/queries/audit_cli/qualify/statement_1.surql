DEFINE FIELD OVERWRITE draft ON audit_record TYPE object FLEXIBLE;
            DEFINE FIELD OVERWRITE recorded_at ON audit_record TYPE datetime DEFAULT ALWAYS time::now();
            UPDATE $record SET draft.latency_ms = 17;
