DEFINE TABLE own SCHEMAFULL;
DEFINE FIELD payload ON own TYPE record<own> ASSERT type::is_string($value.mode);
