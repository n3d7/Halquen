# ADR 0003: Unix socket IPC

Status: accepted

The Linux daemon listens only on a private Unix socket below `XDG_RUNTIME_DIR`. Versioned bounded JSON
frames keep the protocol inspectable without exposing TCP. Startup fails when ownership, modes, path
type, or runtime location are insecure.
