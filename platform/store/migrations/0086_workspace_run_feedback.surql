-- Shared feedback contains execution facts only. Existing runs acquire the
-- canonical field before gateways publish or read the new contract.
DEFINE FIELD feedback ON workspace_run TYPE object DEFAULT { phase: 'preparing', operations: 0 };
DEFINE FIELD feedback.phase ON workspace_run TYPE 'preparing' | 'responding' | 'calling_tools';
DEFINE FIELD feedback.operations ON workspace_run TYPE int ASSERT $value >= 0 AND $value <= 8;
UPDATE workspace_run SET feedback = { phase: 'preparing', operations: 0 };
