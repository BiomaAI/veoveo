SELECT tenant, owner, work_context, profile, server, {
                version: 1,
                principal_key: owner_context.principal_key,
                principal_kind: owner_context.principal_kind,
                issuer: owner_context.issuer,
                subject: owner_context.subject,
                invocation_mode: owner_context.authority.provenance.mode,
                initiator: owner_context.authority.provenance.initiator,
                delegation_id: owner_context.authority.provenance.delegation_id,
                data_labels: owner_context.data_labels
             } AS ownership FROM ONLY $source;
