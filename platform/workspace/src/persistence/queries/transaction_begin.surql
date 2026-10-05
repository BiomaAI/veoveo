BEGIN TRANSACTION;
IF !fn::workspace_authority($authority) { THROW 'workspace_forbidden'; };
