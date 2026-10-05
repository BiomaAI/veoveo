DEFINE TABLE own; DEFINE FIELD value ON own TYPE { link: record<own> } ASSERT array::len($value.link->own) > 0;
