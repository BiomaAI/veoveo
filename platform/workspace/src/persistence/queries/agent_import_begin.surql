BEGIN TRANSACTION;
IF !fn::agent_catalog_authority($authority) { THROW 'agent_forbidden'; };
