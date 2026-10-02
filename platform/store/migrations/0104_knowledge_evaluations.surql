DEFINE TABLE knowledge_evaluation SCHEMAFULL PERMISSIONS NONE;
DEFINE FIELD id ON knowledge_evaluation TYPE string;
DEFINE FIELD tenant ON knowledge_evaluation TYPE string READONLY;
DEFINE FIELD generation ON knowledge_evaluation TYPE record<knowledge_generation>
  READONLY
  REFERENCE ON DELETE CASCADE;
DEFINE FIELD document ON knowledge_evaluation TYPE object FLEXIBLE  READONLY;
DEFINE FIELD recorded_at ON knowledge_evaluation TYPE datetime
  DEFAULT time::now()
  READONLY;
DEFINE INDEX knowledge_evaluation_generation ON knowledge_evaluation FIELDS tenant, generation;
