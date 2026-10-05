DEFINE TABLE own SCHEMAFULL;
DEFINE FIELD timestamp ON own TYPE datetime;
DEFINE FIELD token ON own TYPE string VALUE type::string($this.timestamp);
