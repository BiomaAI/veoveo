DEFINE TABLE own; DEFINE FIELD value ON own TYPE { link: record<own> } ASSERT record::exists($value.link);
