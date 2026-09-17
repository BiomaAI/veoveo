-- Optional observation of an active tools/call request. Task state stays native.
DEFINE FIELD progress ON workspace_operation TYPE option<object>;
DEFINE FIELD progress.completed ON workspace_operation TYPE float ASSERT $value >= 0;
DEFINE FIELD progress.total ON workspace_operation TYPE option<float> ASSERT $value = NONE OR $value >= 0;
DEFINE FIELD progress.message ON workspace_operation TYPE option<string> ASSERT $value = NONE OR string::len($value) <= 2000;
