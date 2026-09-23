# Downstream Migrations

A fork appends migrations to `catalog.rs` and places its SQL files in this directory.
Versions start at zero in this separate lane. Keep deployed entries unchanged and
set `requires_upstream` to the upstream migration version the SQL depends on.

```rust
DownstreamMigration {
    migration: Migration {
        version: 0,
        name: "domain_catalog",
        filename: "0000_domain_catalog.surql",
        sql: include_str!("0000_domain_catalog.surql"),
    },
    requires_upstream: 92,
}
```

Import `Migration` from `crate::migrations` alongside `DownstreamMigration` when adding
entries. The normal store migration command runs both catalogs. Review and test
upstream SQL compatibility during each merge; separate version numbers do not remove
that responsibility. The [runner design](../src/migrations/DESIGN.md) defines ordering,
transaction behavior and recovery.
