-- Every dispatch retains the actual current policy decision that admitted it.
-- Queued operations have no dispatch decision. Prior candidate rows are never
-- backfilled with authority inferred from their owner or elapsed token lifetime.
DEFINE FIELD dispatch_authority ON computer_operation TYPE option<object> FLEXIBLE;
