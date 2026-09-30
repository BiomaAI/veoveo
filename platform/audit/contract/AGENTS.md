# Audit Contract

Follow root AGENTS.md. Keep this contract independent of protocol runtimes and storage
engines. Known fields require domain types. Details contain no arbitrary JSON, maps,
credentials, arguments, payload bytes or error text. Builders and deserialization
validate the same relationships. Changes update every producer and reader in the
same implementation pass.
