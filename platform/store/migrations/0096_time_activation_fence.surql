-- Activation writes a fresh token to serialize decisions across both families.
-- This record carries no release selection; retained pointers need no backfill.
DEFINE TABLE time_authority_activation_fence SCHEMAFULL PERMISSIONS NONE;
DEFINE FIELD tenant ON time_authority_activation_fence TYPE record<tenant>
  READONLY;
DEFINE FIELD token ON time_authority_activation_fence TYPE uuid;
