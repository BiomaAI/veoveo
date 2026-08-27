//! Bounded process diagnostics for authenticated Recording ingest.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use chrono::{DateTime, Utc};
use serde::Serialize;

pub const RECORDING_INGEST_DIAGNOSTICS_SCHEMA: &str = "veoveo.io/recording-ingest-diagnostics/v1";

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingIngestDiagnostics {
    pub accepted_batches_total: u64,
    pub accepted_messages_total: u64,
    pub accepted_bytes_total: u64,
    pub duplicate_batches_total: u64,
    pub materialization_backlog_batches: u64,
    pub materialization_backlog_bytes: u64,
    pub reserved_spool_bytes: u64,
    pub spool_headroom_rejections_total: u64,
    pub available_spool_bytes: u64,
    pub minimum_free_spool_bytes: u64,
    pub last_success_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingIngestDiagnosticsDocument {
    pub schema_version: &'static str,
    #[serde(flatten)]
    pub diagnostics: RecordingIngestDiagnostics,
}

#[derive(Clone, Default)]
pub(crate) struct IngestDiagnostics {
    state: Arc<Mutex<IngestDiagnosticsState>>,
}

pub(crate) struct SpoolReservation {
    diagnostics: IngestDiagnostics,
    byte_len: u64,
}

impl Drop for SpoolReservation {
    fn drop(&mut self) {
        let mut state = self.diagnostics.lock();
        state.snapshot.reserved_spool_bytes = state
            .snapshot
            .reserved_spool_bytes
            .saturating_sub(self.byte_len);
    }
}

#[derive(Default)]
struct IngestDiagnosticsState {
    snapshot: RecordingIngestDiagnostics,
    pending_bytes: BTreeMap<(String, u64), u64>,
}

impl IngestDiagnostics {
    pub(crate) fn reserve_spool(
        &self,
        byte_len: u64,
        available_bytes: u64,
        minimum_free_bytes: u64,
    ) -> SpoolReservation {
        let mut state = self.lock();
        state.snapshot.reserved_spool_bytes =
            state.snapshot.reserved_spool_bytes.saturating_add(byte_len);
        state.snapshot.available_spool_bytes = available_bytes;
        state.snapshot.minimum_free_spool_bytes = minimum_free_bytes;
        SpoolReservation {
            diagnostics: self.clone(),
            byte_len,
        }
    }

    pub(crate) fn reject_spool_headroom(&self, available_bytes: u64, minimum_free_bytes: u64) {
        let mut state = self.lock();
        state.snapshot.available_spool_bytes = available_bytes;
        state.snapshot.minimum_free_spool_bytes = minimum_free_bytes;
        state.snapshot.spool_headroom_rejections_total = state
            .snapshot
            .spool_headroom_rejections_total
            .saturating_add(1);
    }

    pub(crate) fn observe_spool_headroom(&self, available_bytes: u64, minimum_free_bytes: u64) {
        let mut state = self.lock();
        state.snapshot.available_spool_bytes = available_bytes;
        state.snapshot.minimum_free_spool_bytes = minimum_free_bytes;
    }

    pub(crate) fn durable_batch(
        &self,
        stream_id: impl ToString,
        sequence: u64,
        message_count: u64,
        byte_len: u64,
        duplicate: bool,
    ) {
        let mut state = self.lock();
        if duplicate {
            state.snapshot.duplicate_batches_total =
                state.snapshot.duplicate_batches_total.saturating_add(1);
        } else {
            state.snapshot.accepted_batches_total =
                state.snapshot.accepted_batches_total.saturating_add(1);
            state.snapshot.accepted_messages_total = state
                .snapshot
                .accepted_messages_total
                .saturating_add(message_count);
            state.snapshot.accepted_bytes_total =
                state.snapshot.accepted_bytes_total.saturating_add(byte_len);
        }
        state
            .pending_bytes
            .entry((stream_id.to_string(), sequence))
            .or_insert(byte_len);
        state.refresh_backlog();
    }

    pub(crate) fn materialized(&self, stream_id: impl ToString, sequence: u64) {
        let mut state = self.lock();
        state
            .pending_bytes
            .remove(&(stream_id.to_string(), sequence));
        state.refresh_backlog();
    }

    pub(crate) fn request_succeeded(&self, at: DateTime<Utc>) {
        self.lock().snapshot.last_success_at = Some(at);
    }

    pub(crate) fn document(&self) -> RecordingIngestDiagnosticsDocument {
        RecordingIngestDiagnosticsDocument {
            schema_version: RECORDING_INGEST_DIAGNOSTICS_SCHEMA,
            diagnostics: self.lock().snapshot.clone(),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, IngestDiagnosticsState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl IngestDiagnosticsState {
    fn refresh_backlog(&mut self) {
        self.snapshot.materialization_backlog_batches =
            u64::try_from(self.pending_bytes.len()).unwrap_or(u64::MAX);
        self.snapshot.materialization_backlog_bytes = self
            .pending_bytes
            .values()
            .fold(0_u64, |total, bytes| total.saturating_add(*bytes));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostics_count_unique_acceptance_and_deduplicate_backlog() {
        let diagnostics = IngestDiagnostics::default();
        diagnostics.durable_batch("stream-a", 7, 11, 1_024, false);
        diagnostics.durable_batch("stream-a", 7, 11, 1_024, true);
        let observed = diagnostics.document();
        assert_eq!(observed.schema_version, RECORDING_INGEST_DIAGNOSTICS_SCHEMA);
        assert_eq!(observed.diagnostics.accepted_batches_total, 1);
        assert_eq!(observed.diagnostics.accepted_messages_total, 11);
        assert_eq!(observed.diagnostics.accepted_bytes_total, 1_024);
        assert_eq!(observed.diagnostics.duplicate_batches_total, 1);
        assert_eq!(observed.diagnostics.materialization_backlog_batches, 1);
        assert_eq!(observed.diagnostics.materialization_backlog_bytes, 1_024);

        let reservation = diagnostics.reserve_spool(4_096, 1_000_000, 100_000);
        assert_eq!(
            diagnostics.document().diagnostics.reserved_spool_bytes,
            4_096
        );
        drop(reservation);
        assert_eq!(diagnostics.document().diagnostics.reserved_spool_bytes, 0);

        diagnostics.materialized("stream-a", 7);
        let observed = diagnostics.document();
        assert_eq!(observed.diagnostics.materialization_backlog_batches, 0);
        assert_eq!(observed.diagnostics.materialization_backlog_bytes, 0);
    }

    #[test]
    fn last_success_uses_the_completed_request_time() {
        let diagnostics = IngestDiagnostics::default();
        let completed_at = Utc::now();
        diagnostics.request_succeeded(completed_at);
        assert_eq!(
            diagnostics.document().diagnostics.last_success_at,
            Some(completed_at)
        );
    }
}
