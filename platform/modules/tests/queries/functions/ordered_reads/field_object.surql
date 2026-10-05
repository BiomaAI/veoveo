DEFINE TABLE own SCHEMAFULL;
DEFINE FIELD payload ON own TYPE object ASSERT type::is_string($value.mode) AND string::contains($value.mode, 'native');
DEFINE FIELD poll ON own TYPE int ASSERT $value = math::floor($value);
