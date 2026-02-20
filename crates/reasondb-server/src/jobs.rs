//! Background ingestion job queue
//!
//! Provides async job processing so ingestion endpoints return immediately
//! with a job ID, and clients poll for status.

use crate::routes::ingest::{IngestResponse, IngestTextRequest, IngestUrlRequest};
use crate::state::AppState;
use chrono::{DateTime, Utc};
use reasondb_core::llm::ReasoningEngine;
use reasondb_ingest::{IngestPipeline, PipelineConfig};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{error, info, warn};
use utoipa::ToSchema;

const MAX_RETAINED_JOBS: usize = 200;
const JOB_EXPIRY_SECS: i64 = 600; // 10 minutes

#[derive(Debug, Clone, Serialize, ToSchema)]
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

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
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

pub struct JobQueue {
    jobs: RwLock<HashMap<String, Job>>,
    order: RwLock<Vec<String>>,
    notify_tx: mpsc::Sender<String>,
}

impl JobQueue {
    pub fn new() -> (Arc<Self>, mpsc::Receiver<String>) {
        let (tx, rx) = mpsc::channel(256);
        let queue = Arc::new(Self {
            jobs: RwLock::new(HashMap::new()),
            order: RwLock::new(Vec::new()),
            notify_tx: tx,
        });
        (queue, rx)
    }

    pub async fn enqueue(&self, request: JobRequest) -> String {
        let id = format!("job_{}", uuid::Uuid::new_v4().simple());
        let now = Utc::now();

        let job = Job {
            id: id.clone(),
            status: JobStatus::Queued,
            request,
            created_at: now,
            updated_at: now,
        };

        {
            let mut jobs = self.jobs.write().await;
            let mut order = self.order.write().await;
            jobs.insert(id.clone(), job);
            order.push(id.clone());
        }

        let _ = self.notify_tx.send(id.clone()).await;
        self.cleanup_old_jobs().await;
        id
    }

    pub async fn get_status(&self, id: &str) -> Option<JobStatusResponse> {
        let jobs = self.jobs.read().await;
        jobs.get(id).map(JobStatusResponse::from)
    }

    pub async fn list_jobs(&self, limit: usize) -> Vec<JobStatusResponse> {
        let jobs = self.jobs.read().await;
        let order = self.order.read().await;

        order
            .iter()
            .rev()
            .take(limit)
            .filter_map(|id| jobs.get(id).map(JobStatusResponse::from))
            .collect()
    }

    pub async fn update_status(&self, id: &str, status: JobStatus) {
        let mut jobs = self.jobs.write().await;
        if let Some(job) = jobs.get_mut(id) {
            job.status = status;
            job.updated_at = Utc::now();
        }
    }

    pub async fn next_queued(&self) -> Option<Job> {
        let jobs = self.jobs.read().await;
        let order = self.order.read().await;
        for id in order.iter() {
            if let Some(job) = jobs.get(id) {
                if matches!(job.status, JobStatus::Queued) {
                    return Some(job.clone());
                }
            }
        }
        None
    }

    async fn cleanup_old_jobs(&self) {
        let now = Utc::now();
        let mut jobs = self.jobs.write().await;
        let mut order = self.order.write().await;

        let expired: Vec<String> = jobs
            .iter()
            .filter(|(_, job)| {
                matches!(job.status, JobStatus::Completed { .. } | JobStatus::Failed { .. })
                    && (now - job.updated_at).num_seconds() > JOB_EXPIRY_SECS
            })
            .map(|(id, _)| id.clone())
            .collect();

        for id in &expired {
            jobs.remove(id);
        }
        order.retain(|id| jobs.contains_key(id));

        while jobs.len() > MAX_RETAINED_JOBS {
            if let Some(oldest_id) = order.first().cloned() {
                jobs.remove(&oldest_id);
                order.remove(0);
            } else {
                break;
            }
        }
    }
}

/// Background worker that processes queued ingestion jobs one at a time.
pub async fn run_worker<R: ReasoningEngine + Clone + Send + Sync + 'static>(
    state: Arc<AppState<R>>,
    mut rx: mpsc::Receiver<String>,
) {
    info!("Ingestion worker started");

    while let Some(_job_id) = rx.recv().await {
        loop {
            let job = match state.job_queue.next_queued().await {
                Some(j) => j,
                None => break,
            };

            info!("Processing job {}: {}", job.id, job.request.title());

            state
                .job_queue
                .update_status(&job.id, JobStatus::Processing { progress: None })
                .await;

            let result = process_job(&state, &job).await;

            match result {
                Ok(response) => {
                    info!("Job {} completed: {} nodes", job.id, response.total_nodes);
                    state
                        .job_queue
                        .update_status(&job.id, JobStatus::Completed { result: response })
                        .await;
                }
                Err(err) => {
                    error!("Job {} failed: {}", job.id, err);
                    state
                        .job_queue
                        .update_status(
                            &job.id,
                            JobStatus::Failed {
                                error: err.to_string(),
                            },
                        )
                        .await;
                }
            }
        }
    }

    warn!("Ingestion worker shutting down — channel closed");
}

async fn process_job<R: ReasoningEngine + Clone + Send + Sync + 'static>(
    state: &Arc<AppState<R>>,
    job: &Job,
) -> Result<IngestResponse, String> {
    let generate_summaries = match &job.request {
        JobRequest::Text(r) => r.generate_summaries.unwrap_or(state.config.generate_summaries),
        JobRequest::Url(r) => r.generate_summaries.unwrap_or(state.config.generate_summaries),
    };

    let config = PipelineConfig {
        generate_summaries,
        store_in_db: true,
        ..Default::default()
    };

    let pipeline = IngestPipeline::new((*state.reasoner).clone()).with_config(config);

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
