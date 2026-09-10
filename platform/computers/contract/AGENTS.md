# Computers Contract Instructions

Follow root AGENTS.md and this directory's DESIGN.md. Keep this crate independent
of provider and persistence dependencies. Generate Console models from the schema.
Caller input cannot select an owner, tenant, arbitrary image or provider endpoint.
Tokens have no Debug or Display implementation and never appear in resource URLs.
