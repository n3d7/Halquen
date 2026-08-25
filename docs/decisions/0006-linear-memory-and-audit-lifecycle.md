# ADR 0006: Linear memory heads and explicit execution lifecycle

Status: accepted

Stored memory kind, creation time, and current revision are authoritative. A new revision must match
the value-derived kind and extend the current head of the same item. SQLite validates the chain and
the exact referenced evidence set in the write transaction, then advances the head with a guarded
update. Restoration copies an old value into a new successor; it never rewinds history.

Audit records distinguish action request, policy evaluation, confirmation/denial, execution start,
and terminal completion/failure/timeout. Blocked decisions never emit execution start. The API is
append-oriented and duplicate IDs cannot overwrite records, but no cryptographic immutability claim
is made.
