-- Explicit delivery acknowledgements can pin terminal task results past their
-- ordinary TTL. Consumers remove their own pin only after durable consumption.

DEFINE FIELD IF NOT EXISTS retention_pins ON TABLE task TYPE array<string> DEFAULT [] VALUE $value.distinct() ASSERT $value.all(|$pin| string::len($pin) > 0 AND string::len($pin) <= 256);
UPDATE task SET retention_pins = [] WHERE retention_pins = NONE;
