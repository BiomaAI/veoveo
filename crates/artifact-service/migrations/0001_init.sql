-- Shared artifact plane: durable metadata + grant ledger.
-- Bytes live in the object store under a tenant-scoped, per-tenant-encrypted
-- key; this schema owns everything the byte-level PEP decides against.

create table if not exists artifacts (
    sha256                text primary key,
    tenant_id             text        not null,
    byte_len              bigint      not null,
    mime_type             text,
    filename              text,
    classification        text,
    owner_id              text        not null,
    data_labels           jsonb       not null default '[]'::jsonb,
    retention_expires_at  timestamptz,
    metadata              jsonb       not null default 'null'::jsonb,
    created_at            timestamptz not null default now()
);

create index if not exists artifacts_tenant_idx on artifacts (tenant_id);
create index if not exists artifacts_retention_idx on artifacts (retention_expires_at);

create table if not exists artifact_grants (
    sha256                text not null references artifacts (sha256) on delete cascade,
    subject_kind          text not null check (subject_kind in ('user', 'group')),
    subject_id            text not null,
    level                 text not null check (level in ('read', 'write', 'admin')),
    data_labels           jsonb not null default '[]'::jsonb,
    retention_expires_at  timestamptz,
    tenant_id             text not null,
    primary key (sha256, subject_kind, subject_id)
);

create index if not exists artifact_grants_subject_idx
    on artifact_grants (subject_kind, subject_id);
