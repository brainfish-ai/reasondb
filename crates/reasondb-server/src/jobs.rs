//! Durable background ingestion job queue
//!
//! Jobs are persisted to redb so they survive server restarts.
//! Clients poll `/v1/jobs/:id` for status updates.

use crate::routes::ingest::{IngestResponse, IngestTextRequest, IngestUrlRequest};
use crate::state::AppState;
use chrono::{DateTime, Utc};
use reasondb_core::llm::ReasoningEngine;
use reasondb_core::store::NodeStore;
use reasondb_ingest::{IngestPipeline, PipelineConfig};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, info, warn};
use utoipa::ToSchema;

const JOB_EXPIRY_SECS: i64 = 3600; // 1 hour

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "status")]
pub enum JobStatus {
    #[serde(rename = "queued")]
    Queued,
    #[serde(rename = "processing")]
    Processing {
        #[serde(skip_serializing_if = "Option::is_none")]
        progress: Option<String>,
    },
    #[serde(rename = "completed")]
    Completed { result: IngestResponse },
    #[serde(rename = "failed")]
    Failed { error: String },
}

impl JobStatus {
    fn is_queued(&self) -> bool {
        matches!(self, JobStatus::Queued)
    }

    fn is_terminal(&self) -> bool {
        matches!(self, JobStatus::Completed { .. } | JobStatus::Failed { .. })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum JobRequest {
    Text(IngestTextRequest),
    Url(IngestUrlRequest),
}

impl JobRequest {
    pub fn title(&self) -> &str {
        match self {
            JobRequest::Text(r) => &r.title,
            JobRequest::Url(r) => &r.url,
        }
    }

    pub fn table_id(&self) -> &str {
        match self {
            JobRequest::Text(r) => &r.table_id,
            JobRequest::Url(r) => &r.table_id,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    pub status: JobStatus,
    pub request: JobRequest,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct JobStatusResponse {
    pub job_id: String,
    #[serde(flatten)]
    pub status: JobStatus,
    pub created_at: String,
    pub updated_at: String,
}

impl From<&Job> for JobStatusResponse {
    fn from(job: &Job) -> Self {
        Self {
            job_id: job.id.clone(),
            status: job.status.clone(),
            created_at: job.created_at.to_rfc3339(),
            updated_at: job.updated_at.to_rfc3339(),
        }
    }
}

/// Durable job queue backed by redb
pub struct JobQueue {
    store: Arc<NodeStore>,
    notify_tx: mpsc::Sender<String>,
}

impl JobQueue {
    pub fn new(store: Arc<NodeStore>) -> (Arc<Self>, mpsc::Receiver<String>) {
        let (tx, rx) = mpsc::channel(256);
        let queue = Arc::new(Self {
            store,
            notify_tx: tx,
        });
        (queue, rx)
    }

    fn serialize_job(job: &Job) -> Vec<u8> {
        serde_json::to_vec(job).expect("Job serialization should not fail")
    }

    fn deserialize_job(data: &[u8]) -> Option<Job> {
        serde_json::from_slice(data).ok()
    }

    pub fn enqueue(&self, request: JobRequest) -> Result<String, String> {
        let id = format!("job_{}", uuid::Uuid::new_v4().simple());
        let now = Utc::now();

        let job = Job {
            id: id.clone(),
            status: JobStatus::Queued,
            request,
            created_at: now,
            updated_at: now,
        };

        let data = Self::serialize_job(&job);
        self.store
            .insert_job(&id, &data)
            .map_err(|e| format!("Failed to persist job: {}", e))?;

        let _ = self.notify_tx.try_send(id.clone());
        Ok(id)
    }

    pub fn get_status(&self, id: &str) -> Option<JobStatusResponse> {
        self.store
            .get_job(id)
            .ok()
            .flatten()
            .and_then(|data| Self::deserialize_job(&data))
            .map(|job| JobStatusResponse::from(&job))
    }

    pub fn list_jobs(&self, limit: usize) -> Vec<JobStatusResponse> {
        self.store
            .list_jobs(limit)
            .unwrap_or_default()
            .iter()
            .filter_map(|data| Self::deserialize_job(data))
            .map(|job| JobStatusResponse::from(&job))
            .collect()
    }

    pub fn update_status(&self, id: &str, status: JobStatus) {
        if let Ok(Some(data)) = self.store.get_job(id) {
            if let Some(mut job) = Self::deserialize_job(&data) {
                job.status = status;
                job.updated_at = Utc::now();
                let new_data = Self::serialize_job(&job);
                if let Err(e) = self.store.update_job(id, &new_data) {
                    error!("Failed to persist job status update for {}: {}", id, e);
                }
            }
        }
    }

    /// Atomically claim the next queued job (sets it to Processing).
    pub fn claim_next_queued(&self) -> Option<Job> {
        // Build a "Processing" status job as template for the claim
        let is_queued = |data: &[u8]| -> bool {
            Self::deserialize_job(data)
                .map(|j| j.status.is_queued())
                .unwrap_or(false)
        };

        // We'll do the atomic claim in two phases:
        // 1. Find and claim in redb atomically
        // 2. Return the original job data
        match self.store.claim_next_job(is_queued, &[]) {
            Ok(Some((job_id, old_data))) => {
                // Deserialize the original job, update status, persist
                if let Some(mut job) = Self::deserialize_job(&old_data) {
                    job.status = JobStatus::Processing { progress: None };
                    job.updated_at = Utc::now();
                    let new_data = Self::serialize_job(&job);
                    if let Err(e) = self.store.update_job(&job_id, &new_data) {
                        error!("Failed to persist claimed job {}: {}", job_id, e);
                    }
                    Some(job)
                } else {
                    None
                }
            }
            Ok(None) => None,
            Err(e) => {
                error!("Failed to claim next job: {}", e);
                None
            }
        }
    }

    /// Resume incomplete jobs on startup.
    /// Resets any Processing jobs back to Queued and returns the count of recovered jobs.
    pub fn resume_incomplete_jobs(&self) -> usize {
        let all_jobs = match self.store.get_all_jobs() {
            Ok(jobs) => jobs,
            Err(e) => {
                error!("Failed to read jobs on startup: {}", e);
                return 0;
            }
        };

        let mut recovered = 0;
        for (id, data) in all_jobs {
            if let Some(mut job) = Self::deserialize_job(&data) {
                match &job.status {
                    JobStatus::Processing { .. } => {
                        // Was interrupted — reset to Queued
                        info!("Recovering interrupted job {}: {}", id, job.request.title());
                        job.status = JobStatus::Queued;
                        job.updated_at = Utc::now();
                        let new_data = Self::serialize_job(&job);
                        if let Err(e) = self.store.update_job(&id, &new_data) {
                            error!("Failed to recover job {}: {}", id, e);
                        } else {
                            let _ = self.notify_tx.try_send(id);
                            recovered += 1;
                        }
                    }
                    JobStatus::Queued => {
                        // Was waiting — re-notify the worker
                        let _ = self.notify_tx.try_send(id);
                        recovered += 1;
                    }
                    _ => {}
                }
            }
        }
        recovered
    }

    /// Clean up completed/failed jobs older than the expiry threshold.
    pub fn cleanup_expired_jobs(&self) -> usize {
        let all_jobs = match self.store.get_all_jobs() {
            Ok(jobs) => jobs,
            Err(e) => {
                error!("Failed to read jobs for cleanup: {}", e);
                return 0;
            }
        };

        let now = Utc::now();
        let mut to_delete = Vec::new();

        for (id, data) in all_jobs {
            if let Some(job) = Self::deserialize_job(&data) {
                if job.status.is_terminal()
                    && (now - job.updated_at).num_seconds() > JOB_EXPIRY_SECS
                {
                    to_delete.push(id);
                }
            }
        }

        if to_delete.is_empty() {
            return 0;
        }

        match self.store.delete_jobs(&to_delete) {
            Ok(deleted) => {
                if deleted > 0 {
                    info!("Cleaned up {} expired jobs", deleted);
                }
                deleted
            }
            Err(e) => {
                error!("Failed to delete expired jobs: {}", e);
                0
            }
        }
    }
}

/// Spawn N worker tasks that process queued ingestion jobs concurrently.
/// Each worker atomically claims jobs via `claim_next_queued()`, preventing duplicates.
pub async fn run_worker<R: ReasoningEngine + Clone + Send + Sync + 'static>(
    state: Arc<AppState<R>>,
    mut rx: mpsc::Receiver<String>,
) {
    let worker_count = std::env::var("REASONDB_WORKER_COUNT")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(4)
        .max(1);

    info!("Ingestion worker pool starting ({} workers)", worker_count);

    // Resume any incomplete jobs from previous run
    let recovered = state.job_queue.resume_incomplete_jobs();
    if recovered > 0 {
        info!("Recovered {} incomplete jobs from previous run", recovered);
    }

    // Spawn background cleanup task
    let cleanup_queue = state.job_queue.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
        loop {
            interval.tick().await;
            cleanup_queue.cleanup_expired_jobs();
        }
    });

    // Shared notification: broadcast to all workers when new jobs arrive
    let notify = Arc::new(tokio::sync::Notify::new());

    // Spawn worker tasks
    for worker_id in 0..worker_count {
        let w_state = state.clone();
        let w_notify = notify.clone();
        tokio::spawn(async move {
            info!("Worker {} started", worker_id);
            loop {
                w_notify.notified().await;

                // Only process jobs on the leader node (single-node always returns true)
                if !w_state.is_leader().await {
                    continue;
                }

                // Drain all available jobs (atomic claim prevents conflicts)
                while let Some(job) = w_state.job_queue.claim_next_queued() {
                    info!(
                        "Worker {} processing job {}: {}",
                        worker_id,
                        job.id,
                        job.request.title()
                    );

                    let result = process_job(&w_state, &job).await;

                    match result {
                        Ok(response) => {
                            info!(
                                "Worker {} completed job {}: {} nodes",
                                worker_id, job.id, response.total_nodes
                            );
                            w_state
                                .job_queue
                                .update_status(&job.id, JobStatus::Completed { result: response });
                        }
                        Err(err) => {
                            error!("Worker {} failed job {}: {}", worker_id, job.id, err);
                            w_state.job_queue.update_status(
                                &job.id,
                                JobStatus::Failed {
                                    error: err.to_string(),
                                },
                            );
                        }
                    }
                }
            }
        });
    }

    // Dispatcher: receive notifications from enqueue and broadcast to workers
    while let Some(_job_id) = rx.recv().await {
        notify.notify_waiters();
    }

    warn!("Ingestion worker pool shutting down — channel closed");
}

async fn process_job<R: ReasoningEngine + Clone + Send + Sync + 'static>(
    state: &Arc<AppState<R>>,
    job: &Job,
) -> Result<IngestResponse, String> {
    let generate_summaries = match &job.request {
        JobRequest::Text(r) => r
            .generate_summaries
            .unwrap_or(state.config.generate_summaries),
        JobRequest::Url(r) => r
            .generate_summaries
            .unwrap_or(state.config.generate_summaries),
    };

    let config = PipelineConfig {
        generate_summaries,
        store_in_db: true,
        ..Default::default()
    };

    let pipeline = IngestPipeline::new((*state.reasoner).clone())
        .with_config(config)
        .with_plugins(state.plugin_manager.clone());

    let result = match &job.request {
        JobRequest::Text(req) => {
            let mut result = pipeline
                .ingest_text_and_store(&req.title, &req.table_id, &req.content, &state.store)
                .await
                .map_err(|e| e.to_string())?;

            let mut doc = result.document.clone();
            let mut needs_update = false;

            if let Some(tags) = &req.tags {
                doc.tags = tags.clone();
                needs_update = true;
            }
            if let Some(metadata) = &req.metadata {
                doc.metadata = metadata.clone();
                needs_update = true;
            }
            if needs_update {
                state
                    .store
                    .update_document(&doc)
                    .map_err(|e| e.to_string())?;
                result.document = doc;
            }

            result
        }
        JobRequest::Url(req) => pipeline
            .ingest_url_and_store(&req.url, &req.table_id, &state.store)
            .await
            .map_err(|e| e.to_string())?,
    };

    index_document_nodes(
        &state.text_index,
        &state.store,
        &result.document.id,
        &result.document.table_id,
        &result.document.tags,
    )?;

    Ok(IngestResponse {
        document_id: result.document.id.clone(),
        title: result.document.title.clone(),
        total_nodes: result.document.total_nodes,
        max_depth: result.document.max_depth as usize,
        stats: result.stats.into(),
    })
}

fn index_document_nodes(
    text_index: &reasondb_core::text_index::TextIndex,
    store: &reasondb_core::store::NodeStore,
    document_id: &str,
    table_id: &str,
    tags: &[String],
) -> Result<(), String> {
    let nodes = store
        .get_nodes_for_document(document_id)
        .map_err(|e| format!("Failed to get document nodes: {}", e))?;

    for node in &nodes {
        let content = match &node.content {
            Some(c) => c.as_str(),
            None => continue,
        };

        text_index
            .index_node(document_id, &node.id, table_id, &node.title, content, tags)
            .map_err(|e| format!("Failed to index node: {}", e))?;
    }

    text_index
        .commit()
        .map_err(|e| format!("Failed to commit text index: {}", e))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routes::ingest::IngestTextRequest;
    use reasondb_core::store::NodeStore;
    use tempfile::tempdir;

    fn create_test_queue() -> (Arc<JobQueue>, mpsc::Receiver<String>, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test_queue.db");
        let store = Arc::new(NodeStore::open(&db_path).unwrap());
        let (queue, rx) = JobQueue::new(store);
        (queue, rx, dir)
    }

    fn make_text_request(title: &str, table_id: &str) -> JobRequest {
        JobRequest::Text(IngestTextRequest {
            title: title.to_string(),
            table_id: table_id.to_string(),
            content: "Test content".to_string(),
            tags: None,
            metadata: None,
            generate_summaries: None,
        })
    }

    #[test]
    fn test_enqueue_returns_job_id() {
        let (queue, _rx, _dir) = create_test_queue();
        let id = queue.enqueue(make_text_request("Test", "tbl_1")).unwrap();
        assert!(id.starts_with("job_"));
    }

    #[test]
    fn test_enqueue_and_get_status() {
        let (queue, _rx, _dir) = create_test_queue();
        let id = queue.enqueue(make_text_request("My Doc", "tbl_1")).unwrap();

        let status = queue.get_status(&id).unwrap();
        assert_eq!(status.job_id, id);
        assert!(matches!(status.status, JobStatus::Queued));
    }

    #[test]
    fn test_get_status_nonexistent() {
        let (queue, _rx, _dir) = create_test_queue();
        let status = queue.get_status("job_nonexistent");
        assert!(status.is_none());
    }

    #[test]
    fn test_list_jobs() {
        let (queue, _rx, _dir) = create_test_queue();

        queue.enqueue(make_text_request("Doc 1", "tbl_1")).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        queue.enqueue(make_text_request("Doc 2", "tbl_1")).unwrap();

        let jobs = queue.list_jobs(10);
        assert_eq!(jobs.len(), 2);
    }

    #[test]
    fn test_list_jobs_respects_limit() {
        let (queue, _rx, _dir) = create_test_queue();

        for i in 0..5 {
            queue
                .enqueue(make_text_request(&format!("Doc {}", i), "tbl_1"))
                .unwrap();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        let jobs = queue.list_jobs(3);
        assert_eq!(jobs.len(), 3);
    }

    #[test]
    fn test_update_status() {
        let (queue, _rx, _dir) = create_test_queue();
        let id = queue.enqueue(make_text_request("Test", "tbl_1")).unwrap();

        queue.update_status(
            &id,
            JobStatus::Processing {
                progress: Some("50%".to_string()),
            },
        );

        let status = queue.get_status(&id).unwrap();
        assert!(
            matches!(status.status, JobStatus::Processing { progress } if progress == Some("50%".to_string()))
        );
    }

    #[test]
    fn test_claim_next_queued() {
        let (queue, _rx, _dir) = create_test_queue();

        let id1 = queue.enqueue(make_text_request("First", "tbl_1")).unwrap();
        let _id2 = queue.enqueue(make_text_request("Second", "tbl_1")).unwrap();

        let claimed = queue.claim_next_queued();
        assert!(claimed.is_some());

        let job = claimed.unwrap();
        assert_eq!(job.id, id1);
        assert!(matches!(job.status, JobStatus::Processing { .. }));

        // Verify persisted status is Processing
        let status = queue.get_status(&id1).unwrap();
        assert!(matches!(status.status, JobStatus::Processing { .. }));
    }

    #[test]
    fn test_claim_returns_none_when_empty() {
        let (queue, _rx, _dir) = create_test_queue();
        let claimed = queue.claim_next_queued();
        assert!(claimed.is_none());
    }

    #[test]
    fn test_claim_skips_processing_jobs() {
        let (queue, _rx, _dir) = create_test_queue();

        let id1 = queue.enqueue(make_text_request("First", "tbl_1")).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        let id2 = queue.enqueue(make_text_request("Second", "tbl_1")).unwrap();

        // Claim first job
        let first = queue.claim_next_queued().unwrap();
        assert_eq!(first.id, id1);

        // Second claim should get second job (first is now Processing)
        let second = queue.claim_next_queued().unwrap();
        assert_eq!(second.id, id2);

        // No more queued jobs
        assert!(queue.claim_next_queued().is_none());
    }

    #[test]
    fn test_resume_incomplete_jobs() {
        let (queue, _rx, _dir) = create_test_queue();

        // Enqueue and claim to simulate an interrupted job
        let id = queue
            .enqueue(make_text_request("Interrupted", "tbl_1"))
            .unwrap();
        queue.claim_next_queued(); // Now it's Processing

        // Resume should reset it to Queued
        let recovered = queue.resume_incomplete_jobs();
        assert_eq!(recovered, 1);

        // Should be claimable again
        let status = queue.get_status(&id).unwrap();
        assert!(matches!(status.status, JobStatus::Queued));
    }

    #[test]
    fn test_resume_does_not_touch_completed() {
        let (queue, _rx, _dir) = create_test_queue();

        let id = queue.enqueue(make_text_request("Done", "tbl_1")).unwrap();
        queue.update_status(
            &id,
            JobStatus::Completed {
                result: crate::routes::ingest::IngestResponse {
                    document_id: "doc_1".to_string(),
                    title: "Done".to_string(),
                    total_nodes: 5,
                    max_depth: 2,
                    stats: crate::routes::ingest::IngestStats {
                        chars_extracted: 100,
                        chunks_created: 3,
                        nodes_created: 5,
                        summaries_generated: 2,
                        total_time_ms: 65,
                    },
                },
            },
        );

        let recovered = queue.resume_incomplete_jobs();
        assert_eq!(recovered, 0);
    }

    #[test]
    fn test_cleanup_expired_jobs_does_nothing_for_recent() {
        let (queue, _rx, _dir) = create_test_queue();

        let id = queue
            .enqueue(make_text_request("Recent Fail", "tbl_1"))
            .unwrap();
        queue.update_status(
            &id,
            JobStatus::Failed {
                error: "oops".to_string(),
            },
        );

        let cleaned = queue.cleanup_expired_jobs();
        assert_eq!(cleaned, 0, "Recent jobs should not be cleaned up");
    }

    #[test]
    fn test_job_request_title_and_table_id() {
        let text_req = make_text_request("My Title", "tbl_test");
        assert_eq!(text_req.title(), "My Title");
        assert_eq!(text_req.table_id(), "tbl_test");
    }

    #[test]
    fn test_job_status_predicates() {
        assert!(JobStatus::Queued.is_queued());
        assert!(!JobStatus::Queued.is_terminal());

        let processing = JobStatus::Processing { progress: None };
        assert!(!processing.is_queued());
        assert!(!processing.is_terminal());

        let failed = JobStatus::Failed {
            error: "err".to_string(),
        };
        assert!(!failed.is_queued());
        assert!(failed.is_terminal());
    }

    #[test]
    fn test_job_serialization_roundtrip() {
        let now = Utc::now();
        let job = Job {
            id: "job_test".to_string(),
            status: JobStatus::Queued,
            request: make_text_request("Roundtrip", "tbl_1"),
            created_at: now,
            updated_at: now,
        };

        let data = JobQueue::serialize_job(&job);
        let deserialized = JobQueue::deserialize_job(&data).unwrap();

        assert_eq!(deserialized.id, "job_test");
        assert!(matches!(deserialized.status, JobStatus::Queued));
        assert_eq!(deserialized.request.title(), "Roundtrip");
    }

    #[test]
    fn test_job_queue_durability() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("durable_queue.db");

        let job_id;
        {
            let store = Arc::new(NodeStore::open(&db_path).unwrap());
            let (queue, _rx) = JobQueue::new(store);
            job_id = queue
                .enqueue(make_text_request("Persistent", "tbl_1"))
                .unwrap();
        }

        // Reopen and verify
        let store = Arc::new(NodeStore::open(&db_path).unwrap());
        let (queue, _rx) = JobQueue::new(store);
        let status = queue.get_status(&job_id).unwrap();
        assert_eq!(status.job_id, job_id);
        assert!(matches!(status.status, JobStatus::Queued));
    }
}
