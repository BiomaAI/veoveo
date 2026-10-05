-- Current owner schema; schema definitions only.

DEFINE ANALYZER platform_search TOKENIZERS CLASS,BLANK FILTERS LOWERCASE, ASCII;

DEFINE TABLE changefeed_checkpoint TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE FIELD OVERWRITE updated_at ON changefeed_checkpoint TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE versionstamp ON changefeed_checkpoint TYPE int ASSERT $value >= 0 PERMISSIONS FULL;
