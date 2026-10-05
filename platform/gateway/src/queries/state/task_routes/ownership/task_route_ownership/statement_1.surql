SELECT tenant, owner, work_context, profile, server, {
                version: 1,
                principal_key: request.owner.principal_key,
                principal_kind: request.owner.principal_kind,
                issuer: request.owner.issuer,
                subject: request.owner.subject,
                invocation_mode: request.owner.authority.provenance.mode,
                initiator: request.owner.authority.provenance.initiator,
                delegation_id: request.owner.authority.provenance.delegation_id,
                data_labels: request.owner.data_labels
             } AS ownership FROM ONLY $source;
