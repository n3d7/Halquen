# ADR 0001: Core crate boundaries

Status: accepted

The domain crate is dependency-light and independent of runtime services. Policy, capabilities,
memory, audit, storage, and protocol form a directed acyclic graph, while the daemon is the only
composition root. This keeps security decisions testable without IPC or SQLite and prevents UI or
provider concerns from entering authority-bearing types.
