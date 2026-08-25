# ADR 0002: SQLite is the core source of truth

Status: accepted

Halquen uses one local SQLite database for authoritative entities, evidence, revisions, events,
executions, queues, proposals, and permission grants. Foreign keys, transactions, a busy timeout, and
verified WAL mode are explicit. Search indexes, embeddings, or classifier artifacts may be derived
later but are not authoritative memory.
