use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;
use std::time::Duration;

use halquen_audit::{AuditEvent, AuditRecord, ExecutionReceipt, ExecutionStatus, SafeResultCode};
use halquen_domain::{
    AiProposal, AuditId, Correction, Entity, EntityKind, ProposalStatus, QueueStatus, TrustClass,
    UnknownCase,
};
use halquen_memory::{Evidence, MemoryItem, MemoryKind, MemoryRevision};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

use crate::{AuditStats, MemoryStats, StorageError, paths::current_uid};

const MIGRATIONS: &[(i64, &str, &str)] = &[
    (
        1,
        "initial deterministic core",
        include_str!("../../../migrations/0001_initial.sql"),
    ),
    (
        2,
        "desktop interaction and AI control plane",
        include_str!("../../../migrations/0002_desktop_interaction.sql"),
    ),
];

pub struct Database {
    pub(crate) connection: Connection,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self, StorageError> {
        let parent = path.parent().ok_or_else(|| {
            StorageError::InsecureDataPath("database path has no parent directory".to_owned())
        })?;
        let parent_metadata = fs::symlink_metadata(parent)?;
        if parent_metadata.file_type().is_symlink()
            || !parent_metadata.is_dir()
            || parent_metadata.uid() != current_uid()?
            || parent_metadata.permissions().mode() & 0o077 != 0
        {
            return Err(StorageError::InsecureDataPath(format!(
                "{} is not a private user-owned directory",
                parent.display()
            )));
        }
        match fs::symlink_metadata(path) {
            Ok(metadata)
                if metadata.file_type().is_symlink()
                    || !metadata.is_file()
                    || metadata.uid() != current_uid()? =>
            {
                return Err(StorageError::InsecureDataPath(format!(
                    "{} is not a regular database file",
                    path.display()
                )));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let mut connection = Connection::open(path)?;
        if path.exists() {
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        }
        configure_connection(&connection)?;
        let journal_mode: String = connection.pragma_update_and_check(
            None,
            "journal_mode",
            "WAL",
            |row| row.get(0),
        )?;
        if !journal_mode.eq_ignore_ascii_case("wal") {
            return Err(StorageError::WalUnavailable(journal_mode));
        }
        connection.pragma_update(None, "synchronous", "NORMAL")?;
        apply_migrations(&mut connection)?;
        Ok(Self { connection })
    }

    pub fn open_in_memory() -> Result<Self, StorageError> {
        let mut connection = Connection::open_in_memory()?;
        configure_connection(&connection)?;
        apply_migrations(&mut connection)?;
        Ok(Self { connection })
    }

    pub fn schema_version(&self) -> Result<i64, StorageError> {
        self.connection
            .query_row("SELECT COALESCE(MAX(version), 0) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .map_err(Into::into)
    }

    pub fn foreign_keys_enabled(&self) -> Result<bool, StorageError> {
        let enabled: i64 = self
            .connection
            .pragma_query_value(None, "foreign_keys", |row| row.get(0))?;
        Ok(enabled == 1)
    }

    pub fn memory_stats(&self) -> Result<MemoryStats, StorageError> {
        Ok(MemoryStats {
            items: count(&self.connection, "memory_items")?,
            revisions: count(&self.connection, "memory_revisions")?,
            evidence: count(&self.connection, "evidence")?,
            unknown_cases: count(&self.connection, "unknown_cases")?,
        })
    }

    pub fn audit_stats(&self) -> Result<AuditStats, StorageError> {
        Ok(AuditStats {
            records: count(&self.connection, "audit_records")?,
            executions: count(&self.connection, "executions")?,
        })
    }

    pub fn audit_event_kinds(&self, subject_id: &str) -> Result<Vec<String>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT event_kind FROM events WHERE subject_id = ?1 ORDER BY rowid",
        )?;
        let rows = statement.query_map([subject_id], |row| row.get(0))?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn append_audit(&mut self, record: &AuditRecord) -> Result<(), StorageError> {
        let transaction = self.connection.transaction()?;
        insert_audit(&transaction, record)?;
        insert_event_for_audit(&transaction, record)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn record_execution(
        &mut self,
        receipt: &ExecutionReceipt,
        audit_records: &[AuditRecord],
    ) -> Result<(), StorageError> {
        let transaction = self.connection.transaction()?;
        insert_execution(&transaction, receipt)?;
        for record in audit_records {
            insert_audit(&transaction, record)?;
            insert_event_for_audit(&transaction, record)?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn persist_memory_revision(
        &mut self,
        item: &MemoryItem,
        revision: &MemoryRevision,
        evidence: &[Evidence],
    ) -> Result<(), StorageError> {
        let transaction = self.connection.transaction()?;
        if item.id != revision.memory_id {
            return Err(StorageError::InvalidMemoryChange(
                "revision belongs to a different memory item".to_owned(),
            ));
        }
        if item.current_revision_id != revision.id {
            return Err(StorageError::InvalidMemoryChange(
                "memory item does not designate the submitted revision as its new head".to_owned(),
            ));
        }
        if item.kind != revision.value.kind() {
            return Err(StorageError::InvalidMemoryChange(
                "memory value does not match the declared memory kind".to_owned(),
            ));
        }
        if item.updated_at_ms != revision.created_at_ms
            || item.updated_at_ms < item.created_at_ms
        {
            return Err(StorageError::InvalidMemoryChange(
                "memory timestamps are inconsistent".to_owned(),
            ));
        }

        let referenced: BTreeSet<_> = revision.evidence_ids.iter().collect();
        if referenced.is_empty() || referenced.len() != revision.evidence_ids.len() {
            return Err(StorageError::InvalidMemoryChange(
                "revision evidence references must be non-empty and unique".to_owned(),
            ));
        }
        let mut supplied = BTreeMap::new();
        for item in evidence {
            if supplied.insert(&item.id, item).is_some() {
                return Err(StorageError::InvalidMemoryChange(
                    "supplied evidence identifiers must be unique".to_owned(),
                ));
            }
        }
        if supplied.keys().any(|id| !referenced.contains(id)) {
            return Err(StorageError::InvalidMemoryChange(
                "supplied evidence contains an identifier not referenced by the revision"
                    .to_owned(),
            ));
        }

        let existing_item: Option<(String, Option<String>, i64)> = transaction
            .query_row(
                "SELECT memory_kind, current_revision_id, created_at_ms
                 FROM memory_items WHERE id = ?1",
                [item.id.as_str()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        let expected_head = match existing_item {
            None => {
                if revision.previous_revision_id.is_some() {
                    return Err(StorageError::InvalidMemoryChange(
                        "the first memory revision cannot name a predecessor".to_owned(),
                    ));
                }
                transaction.execute(
                    "INSERT INTO memory_items(
                        id, memory_kind, current_revision_id, created_at_ms, updated_at_ms
                     ) VALUES (?1, ?2, NULL, ?3, ?4)",
                    params![
                        item.id.as_str(),
                        memory_kind(item.kind),
                        item.created_at_ms,
                        item.updated_at_ms
                    ],
                )?;
                None
            }
            Some((stored_kind, stored_head, stored_created_at_ms)) => {
                if stored_kind != memory_kind(revision.value.kind())
                    || item.kind != revision.value.kind()
                {
                    return Err(StorageError::InvalidMemoryChange(
                        "stored memory kind is authoritative and cannot be changed".to_owned(),
                    ));
                }
                if item.created_at_ms != stored_created_at_ms {
                    return Err(StorageError::InvalidMemoryChange(
                        "stored memory creation timestamp is authoritative".to_owned(),
                    ));
                }
                let stored_head = stored_head.ok_or_else(|| {
                    StorageError::InvalidMemoryChange(
                        "existing memory item has no current revision".to_owned(),
                    )
                })?;
                if revision.previous_revision_id.as_ref().map(|id| id.as_str())
                    != Some(stored_head.as_str())
                {
                    return Err(StorageError::InvalidMemoryChange(
                        "revision predecessor is not the current stored head".to_owned(),
                    ));
                }
                let predecessor_owner: String = transaction.query_row(
                    "SELECT memory_id FROM memory_revisions WHERE id = ?1",
                    [stored_head.as_str()],
                    |row| row.get(0),
                )?;
                if predecessor_owner != item.id.as_str() {
                    return Err(StorageError::InvalidMemoryChange(
                        "revision predecessor belongs to a different memory item".to_owned(),
                    ));
                }
                Some(stored_head)
            }
        };

        let mut resolved_trust = Vec::with_capacity(revision.evidence_ids.len());
        for evidence_id in &revision.evidence_ids {
            let trust = if let Some(candidate) = supplied.get(evidence_id) {
                insert_or_verify_evidence(&transaction, candidate)?;
                candidate.trust
            } else {
                let stored: Option<String> = transaction
                    .query_row(
                        "SELECT trust_class FROM evidence WHERE id = ?1",
                        [evidence_id.as_str()],
                        |row| row.get(0),
                    )
                    .optional()?;
                parse_trust_class(stored.as_deref().ok_or_else(|| {
                    StorageError::InvalidMemoryChange(
                        "revision references missing evidence".to_owned(),
                    )
                })?)?
            };
            resolved_trust.push(trust);
        }
        if revision.value.kind() == MemoryKind::Procedural
            && !resolved_trust
                .iter()
                .any(|trust| trust.independently_authorizes_procedural_memory())
        {
            return Err(StorageError::InvalidMemoryChange(
                "procedural memory requires referenced independent user authority".to_owned(),
            ));
        }

        transaction.execute(
            "INSERT INTO memory_revisions(
                id, memory_id, previous_revision_id, value_json, created_at_ms, valid_from_ms, valid_until_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                revision.id.as_str(),
                revision.memory_id.as_str(),
                revision.previous_revision_id.as_ref().map(|id| id.as_str()),
                serde_json::to_string(&revision.value)?,
                revision.created_at_ms,
                revision.valid_from_ms,
                revision.valid_until_ms
            ],
        )?;
        for evidence_id in &revision.evidence_ids {
            transaction.execute(
                "INSERT INTO memory_evidence(revision_id, evidence_id) VALUES (?1, ?2)",
                params![revision.id.as_str(), evidence_id.as_str()],
            )?;
        }
        let updated = match expected_head {
            Some(expected) => transaction.execute(
                "UPDATE memory_items SET current_revision_id = ?1, updated_at_ms = ?2
                 WHERE id = ?3 AND current_revision_id = ?4",
                params![
                    revision.id.as_str(),
                    item.updated_at_ms,
                    item.id.as_str(),
                    expected
                ],
            )?,
            None => transaction.execute(
                "UPDATE memory_items SET current_revision_id = ?1, updated_at_ms = ?2
                 WHERE id = ?3 AND current_revision_id IS NULL",
                params![revision.id.as_str(), item.updated_at_ms, item.id.as_str()],
            )?,
        };
        if updated != 1 {
            return Err(StorageError::InvalidMemoryChange(
                "memory head changed while persisting the revision".to_owned(),
            ));
        }
        let memory_audit = AuditRecord {
            id: AuditId::generate(),
            created_at_ms: revision.created_at_ms,
            event: AuditEvent::MemoryRevision {
                memory_id: item.id.clone(),
                revision_id: revision.id.clone(),
            },
        };
        insert_audit(&transaction, &memory_audit)?;
        insert_event_for_audit(&transaction, &memory_audit)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn insert_entity(&mut self, entity: &Entity) -> Result<(), StorageError> {
        self.connection.execute(
            "INSERT INTO entities(
                id, entity_type, canonical_name, created_at_ms, updated_at_ms, valid_from_ms, valid_until_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                entity.id.as_str(),
                entity_kind(entity.kind),
                entity.canonical_name,
                entity.created_at_ms,
                entity.updated_at_ms,
                entity.valid_from_ms,
                entity.valid_until_ms
            ],
        )?;
        Ok(())
    }

    pub fn enqueue_unknown(&mut self, item: &UnknownCase) -> Result<(), StorageError> {
        self.connection.execute(
            "INSERT INTO unknown_cases(id, request_summary, status, created_at_ms) VALUES (?1, ?2, ?3, ?4)",
            params![item.id.as_str(), item.request_summary, queue_status(item.status), item.created_at_ms],
        )?;
        Ok(())
    }

    pub fn record_correction(&mut self, item: &Correction) -> Result<(), StorageError> {
        self.connection.execute(
            "INSERT INTO corrections(id, target_id, correction_summary, created_at_ms) VALUES (?1, ?2, ?3, ?4)",
            params![item.id.as_str(), item.target_id, item.correction_summary, item.created_at_ms],
        )?;
        Ok(())
    }

    pub fn store_ai_proposal(&mut self, proposal: &AiProposal) -> Result<(), StorageError> {
        self.connection.execute(
            "INSERT INTO ai_proposals(
                id, provider_model, created_at_ms, proposal_json, evidence_ids_json, status
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                proposal.id.as_str(),
                proposal.provider_model,
                proposal.created_at_ms,
                serde_json::to_string(&proposal.payload)?,
                serde_json::to_string(&proposal.evidence_ids)?,
                proposal_status(proposal.status)
            ],
        )?;
        Ok(())
    }
}

fn insert_or_verify_evidence(
    transaction: &Transaction<'_>,
    evidence: &Evidence,
) -> Result<(), StorageError> {
    let inserted = transaction.execute(
        "INSERT INTO evidence(id, trust_class, source_reference, created_at_ms)
         VALUES (?1, ?2, ?3, ?4) ON CONFLICT(id) DO NOTHING",
        params![
            evidence.id.as_str(),
            trust_class(evidence.trust),
            evidence.source_reference,
            evidence.created_at_ms
        ],
    )?;
    if inserted == 0 {
        let existing: (String, Option<String>, i64) = transaction.query_row(
            "SELECT trust_class, source_reference, created_at_ms FROM evidence WHERE id = ?1",
            [evidence.id.as_str()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        if existing
            != (
                trust_class(evidence.trust).to_owned(),
                evidence.source_reference.clone(),
                evidence.created_at_ms,
            )
        {
            return Err(StorageError::InvalidMemoryChange(
                "evidence identifier conflicts with existing evidence".to_owned(),
            ));
        }
    }
    Ok(())
}

fn parse_trust_class(value: &str) -> Result<TrustClass, StorageError> {
    match value {
        "user_explicit" => Ok(TrustClass::UserExplicit),
        "local_verified" => Ok(TrustClass::LocalVerified),
        "user_confirmed_result" => Ok(TrustClass::UserConfirmedResult),
        "user_behaviour" => Ok(TrustClass::UserBehaviour),
        "ai_inferred" => Ok(TrustClass::AiInferred),
        "plugin_asserted" => Ok(TrustClass::PluginAsserted),
        "external_content" => Ok(TrustClass::ExternalContent),
        _ => Err(StorageError::InvalidMemoryChange(
            "stored evidence has an unknown trust class".to_owned(),
        )),
    }
}

fn configure_connection(connection: &Connection) -> Result<(), StorageError> {
    connection.busy_timeout(Duration::from_secs(5))?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    let enabled: i64 = connection.pragma_query_value(None, "foreign_keys", |row| row.get(0))?;
    if enabled != 1 {
        return Err(StorageError::ForeignKeysUnavailable);
    }
    Ok(())
}

fn apply_migrations(connection: &mut Connection) -> Result<(), StorageError> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            description TEXT NOT NULL,
            applied_at_ms INTEGER NOT NULL
        );",
    )?;
    for (version, description, sql) in MIGRATIONS {
        let applied = connection
            .query_row(
                "SELECT 1 FROM schema_migrations WHERE version = ?1",
                [version],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if applied {
            continue;
        }
        let transaction = connection.transaction()?;
        transaction
            .execute_batch(sql)
            .map_err(|source| StorageError::Migration {
                version: *version,
                source,
            })?;
        transaction.execute(
            "INSERT INTO schema_migrations(version, description, applied_at_ms)
             VALUES (?1, ?2, unixepoch('subsec') * 1000)",
            params![version, description],
        )?;
        transaction.commit()?;
    }
    Ok(())
}

fn insert_execution(transaction: &Transaction<'_>, receipt: &ExecutionReceipt) -> Result<(), StorageError> {
    transaction.execute(
        "INSERT INTO executions(
            id, capability_id, capability_version, started_at_ms, finished_at_ms, policy_json,
            status, reversible, result_code, error_code, sanitized_error, compensation_reference
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            receipt.execution_id.as_str(),
            receipt.capability_id.as_str(),
            receipt.capability_version,
            receipt.started_at_ms,
            receipt.finished_at_ms,
            serde_json::to_string(&receipt.policy_decision)?,
            execution_status(receipt.status),
            receipt.reversible,
            receipt.result_code.map(result_code),
            receipt.error_code,
            receipt.sanitized_error,
            receipt.compensation_reference
        ],
    )?;
    Ok(())
}

fn insert_audit(transaction: &Transaction<'_>, record: &AuditRecord) -> Result<(), StorageError> {
    transaction.execute(
        "INSERT INTO audit_records(id, created_at_ms, event_json) VALUES (?1, ?2, ?3)",
        params![record.id.as_str(), record.created_at_ms, serde_json::to_string(&record.event)?],
    )?;
    Ok(())
}

fn insert_event_for_audit(
    transaction: &Transaction<'_>,
    record: &AuditRecord,
) -> Result<(), StorageError> {
    let (kind, subject_id) = match &record.event {
        AuditEvent::ActionRequested { execution_id, .. } => {
            ("action_requested", execution_id.as_str())
        }
        AuditEvent::PolicyEvaluated {
            execution_id,
            capability_id,
            ..
        } => (
            "policy_evaluated",
            execution_id
                .as_ref()
                .map_or(capability_id.as_str(), |id| id.as_str()),
        ),
        AuditEvent::ConfirmationRequired { execution_id, .. } => {
            ("confirmation_required", execution_id.as_str())
        }
        AuditEvent::ActionDenied { execution_id, .. } => {
            ("action_denied", execution_id.as_str())
        }
        AuditEvent::ExecutionStarted { execution_id, .. } => {
            ("execution_started", execution_id.as_str())
        }
        AuditEvent::ExecutionCompleted { execution_id, .. } => {
            ("execution_completed", execution_id.as_str())
        }
        AuditEvent::ExecutionFailed { execution_id, .. } => {
            ("execution_failed", execution_id.as_str())
        }
        AuditEvent::ExecutionTimedOut { execution_id, .. } => {
            ("execution_timed_out", execution_id.as_str())
        }
        AuditEvent::MemoryRevision { memory_id, .. } => {
            ("memory_revision", memory_id.as_str())
        }
    };
    transaction.execute(
        "INSERT INTO events(id, event_kind, subject_id, created_at_ms) VALUES (?1, ?2, ?3, ?4)",
        params![record.id.as_str(), kind, subject_id, record.created_at_ms],
    )?;
    Ok(())
}

fn count(connection: &Connection, table: &str) -> Result<u64, StorageError> {
    let sql = match table {
        "memory_items" => "SELECT COUNT(*) FROM memory_items",
        "memory_revisions" => "SELECT COUNT(*) FROM memory_revisions",
        "evidence" => "SELECT COUNT(*) FROM evidence",
        "unknown_cases" => "SELECT COUNT(*) FROM unknown_cases",
        "audit_records" => "SELECT COUNT(*) FROM audit_records",
        "executions" => "SELECT COUNT(*) FROM executions",
        _ => return Err(StorageError::InvalidStaticQuery),
    };
    let value: i64 = connection.query_row(sql, [], |row| row.get(0))?;
    u64::try_from(value).map_err(|_| StorageError::InvalidStaticQuery)
}

fn memory_kind(value: MemoryKind) -> &'static str {
    match value {
        MemoryKind::Semantic => "semantic",
        MemoryKind::Procedural => "procedural",
    }
}

fn entity_kind(value: EntityKind) -> &'static str {
    match value {
        EntityKind::Application => "application",
        EntityKind::Project => "project",
        EntityKind::File => "file",
        EntityKind::Person => "person",
        EntityKind::Device => "device",
        EntityKind::Routine => "routine",
        EntityKind::Generic => "generic",
    }
}

fn queue_status(value: QueueStatus) -> &'static str {
    match value {
        QueueStatus::Pending => "pending",
        QueueStatus::Resolved => "resolved",
        QueueStatus::Dismissed => "dismissed",
    }
}

fn proposal_status(value: ProposalStatus) -> &'static str {
    match value {
        ProposalStatus::Pending => "pending",
        ProposalStatus::Accepted => "accepted",
        ProposalStatus::Rejected => "rejected",
        ProposalStatus::Superseded => "superseded",
    }
}

fn trust_class(value: halquen_domain::TrustClass) -> &'static str {
    match value {
        halquen_domain::TrustClass::UserExplicit => "user_explicit",
        halquen_domain::TrustClass::LocalVerified => "local_verified",
        halquen_domain::TrustClass::UserConfirmedResult => "user_confirmed_result",
        halquen_domain::TrustClass::UserBehaviour => "user_behaviour",
        halquen_domain::TrustClass::AiInferred => "ai_inferred",
        halquen_domain::TrustClass::PluginAsserted => "plugin_asserted",
        halquen_domain::TrustClass::ExternalContent => "external_content",
    }
}

fn execution_status(value: ExecutionStatus) -> &'static str {
    match value {
        ExecutionStatus::DryRunSucceeded => "dry_run_succeeded",
        ExecutionStatus::Failed => "failed",
        ExecutionStatus::TimedOut => "timed_out",
        ExecutionStatus::NotExecuted => "not_executed",
    }
}

fn result_code(value: SafeResultCode) -> &'static str {
    match value {
        SafeResultCode::Simulated => "simulated",
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use halquen_domain::{EntityId, EvidenceId, MemoryId, MemoryRevisionId, TrustClass};
    use halquen_memory::MemoryValue;

    use super::*;

    struct TempDatabasePath {
        directory: PathBuf,
        database: PathBuf,
    }

    impl TempDatabasePath {
        fn new() -> Self {
            static SEQUENCE: AtomicU64 = AtomicU64::new(0);
            let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let directory = std::env::temp_dir().join(format!(
                "halquen-storage-test-{}-{sequence}",
                std::process::id()
            ));
            fs::create_dir(&directory).unwrap();
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
            let database = directory.join("halquen.sqlite3");
            Self {
                directory,
                database,
            }
        }
    }

    impl Drop for TempDatabasePath {
        fn drop(&mut self) {
            for suffix in ["", "-wal", "-shm"] {
                let path = PathBuf::from(format!("{}{}", self.database.display(), suffix));
                if let Err(error) = fs::remove_file(path)
                    && error.kind() != std::io::ErrorKind::NotFound
                {
                    panic!("failed to remove test database artifact: {error}");
                }
            }
            fs::remove_dir(&self.directory).unwrap();
        }
    }

    fn evidence(trust: TrustClass, at_ms: i64) -> Evidence {
        Evidence {
            id: EvidenceId::generate(),
            trust,
            source_reference: None,
            created_at_ms: at_ms,
        }
    }

    fn preference(value: &str) -> MemoryValue {
        MemoryValue::Preference {
            key: "editor".to_owned(),
            value: value.to_owned(),
        }
    }

    fn procedure(name: &str) -> MemoryValue {
        MemoryValue::Procedure {
            name: name.to_owned(),
            capability_ids: vec!["system.open_app".to_owned()],
        }
    }

    fn first_change(
        kind: MemoryKind,
        value: MemoryValue,
        evidence_ids: Vec<EvidenceId>,
        at_ms: i64,
    ) -> (MemoryItem, MemoryRevision) {
        let memory_id = MemoryId::generate();
        let revision_id = MemoryRevisionId::generate();
        (
            MemoryItem {
                id: memory_id.clone(),
                kind,
                current_revision_id: revision_id.clone(),
                created_at_ms: at_ms,
                updated_at_ms: at_ms,
            },
            MemoryRevision {
                id: revision_id,
                memory_id,
                previous_revision_id: None,
                value,
                evidence_ids,
                created_at_ms: at_ms,
                valid_from_ms: Some(at_ms),
                valid_until_ms: None,
            },
        )
    }

    fn successor(
        previous_item: &MemoryItem,
        previous_revision_id: MemoryRevisionId,
        value: MemoryValue,
        evidence_ids: Vec<EvidenceId>,
        at_ms: i64,
    ) -> (MemoryItem, MemoryRevision) {
        let revision_id = MemoryRevisionId::generate();
        (
            MemoryItem {
                id: previous_item.id.clone(),
                kind: previous_item.kind,
                current_revision_id: revision_id.clone(),
                created_at_ms: previous_item.created_at_ms,
                updated_at_ms: at_ms,
            },
            MemoryRevision {
                id: revision_id,
                memory_id: previous_item.id.clone(),
                previous_revision_id: Some(previous_revision_id),
                value,
                evidence_ids,
                created_at_ms: at_ms,
                valid_from_ms: Some(at_ms),
                valid_until_ms: None,
            },
        )
    }

    #[test]
    fn fresh_database_runs_migration_and_enables_foreign_keys() {
        let database = Database::open_in_memory().unwrap();
        assert_eq!(database.schema_version().unwrap(), 2);
        assert!(database.foreign_keys_enabled().unwrap());
    }

    #[test]
    fn file_database_reopens_with_wal_busy_timeout_and_all_migrations_once() {
        let path = TempDatabasePath::new();
        {
            let database = Database::open(&path.database).unwrap();
            let journal: String = database
                .connection
                .pragma_query_value(None, "journal_mode", |row| row.get(0))
                .unwrap();
            let busy_ms: i64 = database
                .connection
                .pragma_query_value(None, "busy_timeout", |row| row.get(0))
                .unwrap();
            assert_eq!(journal.to_ascii_lowercase(), "wal");
            assert_eq!(busy_ms, 5_000);
            assert_eq!(database.schema_version().unwrap(), 2);
        }
        {
            let database = Database::open(&path.database).unwrap();
            let migrations: i64 = database
                .connection
                .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| row.get(0))
                .unwrap();
            assert_eq!(migrations, 2);
        }
        assert_eq!(
            fs::metadata(&path.database)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }

    #[test]
    fn foreign_key_constraints_are_enforced() {
        let database = Database::open_in_memory().unwrap();
        let result = database.connection.execute(
            "INSERT INTO aliases(entity_id, alias, trust_class, created_at_ms)
             VALUES ('missing', 'alias', 'user_explicit', 1)",
            [],
        );
        assert!(result.is_err());
    }

    #[test]
    fn transaction_rolls_back_all_rows_after_constraint_failure() {
        let mut database = Database::open_in_memory().unwrap();
        {
            let transaction = database.connection.transaction().unwrap();
            transaction
                .execute(
                    "INSERT INTO entities(id, entity_type, canonical_name, created_at_ms, updated_at_ms)
                     VALUES ('entity:first', 'generic', 'First', 1, 1)",
                    [],
                )
                .unwrap();
            assert!(
                transaction
                    .execute(
                        "INSERT INTO entities(id, entity_type, canonical_name, created_at_ms, updated_at_ms)
                         VALUES ('entity:first', 'generic', 'Duplicate', 1, 1)",
                        [],
                    )
                    .is_err()
            );
        }
        let count: i64 = database
            .connection
            .query_row("SELECT COUNT(*) FROM entities", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn entity_constraints_reject_empty_names() {
        let mut database = Database::open_in_memory().unwrap();
        let entity = Entity {
            id: EntityId::new("entity:test").unwrap(),
            kind: EntityKind::Generic,
            canonical_name: String::new(),
            created_at_ms: 1,
            updated_at_ms: 1,
            valid_from_ms: None,
            valid_until_ms: None,
        };
        assert!(database.insert_entity(&entity).is_err());
    }

    #[test]
    fn migration_is_applied_exactly_once() {
        let mut database = Database::open_in_memory().unwrap();
        apply_migrations(&mut database.connection).unwrap();
        let count: i64 = database
            .connection
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn audit_api_is_append_only_and_duplicate_ids_do_not_overwrite() {
        let mut database = Database::open_in_memory().unwrap();
        let record = AuditRecord {
            id: AuditId::generate(),
            created_at_ms: 1,
            event: AuditEvent::MemoryRevision {
                memory_id: MemoryId::generate(),
                revision_id: MemoryRevisionId::generate(),
            },
        };
        database.append_audit(&record).unwrap();
        assert!(database.append_audit(&record).is_err());
        assert_eq!(database.audit_stats().unwrap().records, 1);
        let stored: String = database
            .connection
            .query_row(
                "SELECT event_json FROM audit_records WHERE id = ?1",
                [record.id.as_str()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stored, serde_json::to_string(&record.event).unwrap());
    }

    #[test]
    fn untrusted_evidence_cannot_persist_procedural_memory() {
        let mut database = Database::open_in_memory().unwrap();
        let memory_id = MemoryId::generate();
        let revision_id = MemoryRevisionId::generate();
        let evidence = Evidence {
            id: EvidenceId::generate(),
            trust: TrustClass::ExternalContent,
            source_reference: Some("document:untrusted".to_owned()),
            created_at_ms: 1,
        };
        let item = MemoryItem {
            id: memory_id.clone(),
            kind: MemoryKind::Procedural,
            current_revision_id: revision_id.clone(),
            created_at_ms: 1,
            updated_at_ms: 1,
        };
        let revision = MemoryRevision {
            id: revision_id,
            memory_id,
            previous_revision_id: None,
            value: MemoryValue::Procedure {
                name: "Untrusted routine".to_owned(),
                capability_ids: vec!["system.open_app".to_owned()],
            },
            evidence_ids: vec![evidence.id.clone()],
            created_at_ms: 1,
            valid_from_ms: Some(1),
            valid_until_ms: None,
        };
        assert!(database
            .persist_memory_revision(&item, &revision, &[evidence])
            .is_err());
        assert_eq!(database.memory_stats().unwrap().items, 0);
    }

    #[test]
    fn unrelated_trusted_evidence_cannot_authorize_a_procedure() {
        for trust in [TrustClass::ExternalContent, TrustClass::AiInferred] {
            let mut database = Database::open_in_memory().unwrap();
            let referenced = evidence(trust, 1);
            let unrelated = evidence(TrustClass::UserExplicit, 1);
            let (item, revision) = first_change(
                MemoryKind::Procedural,
                procedure("untrusted"),
                vec![referenced.id.clone()],
                1,
            );
            assert!(database
                .persist_memory_revision(&item, &revision, &[referenced, unrelated])
                .is_err());
            assert_eq!(database.memory_stats().unwrap().items, 0);
        }
    }

    #[test]
    fn referenced_trusted_evidence_authorizes_a_procedure() {
        let mut database = Database::open_in_memory().unwrap();
        let trusted = evidence(TrustClass::UserExplicit, 1);
        let (item, revision) = first_change(
            MemoryKind::Procedural,
            procedure("trusted"),
            vec![trusted.id.clone()],
            1,
        );
        database
            .persist_memory_revision(&item, &revision, &[trusted])
            .unwrap();
        assert_eq!(database.memory_stats().unwrap().items, 1);
    }

    #[test]
    fn stored_evidence_can_be_resolved_by_exact_reference() {
        let mut database = Database::open_in_memory().unwrap();
        let trusted = evidence(TrustClass::UserExplicit, 1);
        let (first_item, first_revision) = first_change(
            MemoryKind::Procedural,
            procedure("first"),
            vec![trusted.id.clone()],
            1,
        );
        database
            .persist_memory_revision(&first_item, &first_revision, &[trusted.clone()])
            .unwrap();
        let (second_item, second_revision) = successor(
            &first_item,
            first_revision.id.clone(),
            procedure("second"),
            vec![trusted.id],
            2,
        );
        database
            .persist_memory_revision(&second_item, &second_revision, &[])
            .unwrap();
        assert_eq!(database.memory_stats().unwrap().revisions, 2);
    }

    #[test]
    fn rejects_value_kind_spoofing_on_create_and_update() {
        let mut database = Database::open_in_memory().unwrap();
        let trusted = evidence(TrustClass::UserExplicit, 1);
        let (spoofed_item, spoofed_revision) = first_change(
            MemoryKind::Semantic,
            procedure("spoofed"),
            vec![trusted.id.clone()],
            1,
        );
        assert!(database
            .persist_memory_revision(&spoofed_item, &spoofed_revision, &[trusted])
            .is_err());

        let semantic_evidence = evidence(TrustClass::ExternalContent, 2);
        let (first_item, first_revision) = first_change(
            MemoryKind::Semantic,
            preference("Zed"),
            vec![semantic_evidence.id.clone()],
            2,
        );
        database
            .persist_memory_revision(&first_item, &first_revision, &[semantic_evidence])
            .unwrap();
        let update_evidence = evidence(TrustClass::UserExplicit, 3);
        let (mut changed_item, changed_revision) = successor(
            &first_item,
            first_revision.id,
            procedure("changed kind"),
            vec![update_evidence.id.clone()],
            3,
        );
        changed_item.kind = MemoryKind::Procedural;
        assert!(database
            .persist_memory_revision(&changed_item, &changed_revision, &[update_evidence])
            .is_err());
        assert_eq!(database.memory_stats().unwrap().revisions, 1);
    }

    #[test]
    fn accepts_linear_revisions_and_restore_as_a_new_head() {
        let mut database = Database::open_in_memory().unwrap();
        let first_evidence = evidence(TrustClass::ExternalContent, 1);
        let (first_item, first_revision) = first_change(
            MemoryKind::Semantic,
            preference("VS Code"),
            vec![first_evidence.id.clone()],
            1,
        );
        database
            .persist_memory_revision(&first_item, &first_revision, &[first_evidence])
            .unwrap();

        let second_evidence = evidence(TrustClass::UserExplicit, 2);
        let (second_item, second_revision) = successor(
            &first_item,
            first_revision.id.clone(),
            preference("Zed"),
            vec![second_evidence.id.clone()],
            2,
        );
        database
            .persist_memory_revision(&second_item, &second_revision, &[second_evidence])
            .unwrap();

        let restore_evidence = evidence(TrustClass::UserExplicit, 3);
        let (restored_item, restored_revision) = successor(
            &second_item,
            second_revision.id,
            first_revision.value,
            vec![restore_evidence.id.clone()],
            3,
        );
        database
            .persist_memory_revision(&restored_item, &restored_revision, &[restore_evidence])
            .unwrap();
        assert_ne!(restored_revision.id, first_revision.id);
        assert_eq!(database.memory_stats().unwrap().revisions, 3);
    }

    #[test]
    fn rejects_stale_branches_and_item_revision_mismatch() {
        let mut database = Database::open_in_memory().unwrap();
        let first_evidence = evidence(TrustClass::ExternalContent, 1);
        let (first_item, first_revision) = first_change(
            MemoryKind::Semantic,
            preference("one"),
            vec![first_evidence.id.clone()],
            1,
        );
        database
            .persist_memory_revision(&first_item, &first_revision, &[first_evidence])
            .unwrap();
        let second_evidence = evidence(TrustClass::ExternalContent, 2);
        let (second_item, second_revision) = successor(
            &first_item,
            first_revision.id.clone(),
            preference("two"),
            vec![second_evidence.id.clone()],
            2,
        );
        database
            .persist_memory_revision(&second_item, &second_revision, &[second_evidence])
            .unwrap();

        let stale_evidence = evidence(TrustClass::ExternalContent, 3);
        let (stale_item, stale_revision) = successor(
            &second_item,
            first_revision.id,
            preference("stale"),
            vec![stale_evidence.id.clone()],
            3,
        );
        assert!(database
            .persist_memory_revision(&stale_item, &stale_revision, &[stale_evidence])
            .is_err());

        let mismatch_evidence = evidence(TrustClass::ExternalContent, 4);
        let (mismatch_item, mut mismatch_revision) = first_change(
            MemoryKind::Semantic,
            preference("mismatch"),
            vec![mismatch_evidence.id.clone()],
            4,
        );
        mismatch_revision.memory_id = MemoryId::generate();
        assert!(database
            .persist_memory_revision(
                &mismatch_item,
                &mismatch_revision,
                &[mismatch_evidence],
            )
            .is_err());
        assert_eq!(database.memory_stats().unwrap().revisions, 2);
    }

    #[test]
    fn rejects_cross_item_predecessor_even_if_head_is_corrupted() {
        let mut database = Database::open_in_memory().unwrap();
        let evidence_a = evidence(TrustClass::ExternalContent, 1);
        let (item_a, revision_a) = first_change(
            MemoryKind::Semantic,
            preference("a"),
            vec![evidence_a.id.clone()],
            1,
        );
        database
            .persist_memory_revision(&item_a, &revision_a, &[evidence_a])
            .unwrap();
        let evidence_b = evidence(TrustClass::ExternalContent, 1);
        let (item_b, revision_b) = first_change(
            MemoryKind::Semantic,
            preference("b"),
            vec![evidence_b.id.clone()],
            1,
        );
        database
            .persist_memory_revision(&item_b, &revision_b, &[evidence_b])
            .unwrap();
        database
            .connection
            .execute(
                "UPDATE memory_items SET current_revision_id = ?1 WHERE id = ?2",
                params![revision_b.id.as_str(), item_a.id.as_str()],
            )
            .unwrap();

        let next_evidence = evidence(TrustClass::ExternalContent, 2);
        let (next_item, next_revision) = successor(
            &item_a,
            revision_b.id,
            preference("cross"),
            vec![next_evidence.id.clone()],
            2,
        );
        assert!(database
            .persist_memory_revision(&next_item, &next_revision, &[next_evidence])
            .is_err());
    }
}
