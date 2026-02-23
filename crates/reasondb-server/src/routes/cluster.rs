//! Cluster management routes
//!
//! API endpoints for cluster status, management, and node operations.

use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use reasondb_core::cluster::NodeId;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;

use crate::{error::ApiError, state::AppState};

/// Cluster status response
#[derive(Debug, Serialize, ToSchema)]
pub struct ClusterStatusResponse {
    /// Whether clustering is enabled
    pub enabled: bool,
    /// This node's ID
    pub node_id: Option<String>,
    /// This node's role
    pub role: Option<String>,
    /// Current Raft term
    pub term: Option<u64>,
    /// Leader node ID
    pub leader_id: Option<String>,
    /// Number of nodes in cluster
    pub node_count: usize,
    /// Last applied log index
    pub last_applied: Option<u64>,
    /// Commit index
    pub commit_index: Option<u64>,
    /// Whether we have quorum
    pub has_quorum: bool,
    /// Cluster name
    pub cluster_name: String,
}

/// Individual node info
#[derive(Debug, Serialize, ToSchema)]
pub struct NodeInfo {
    /// Node ID
    pub id: String,
    /// Node name
    pub name: String,
    /// API address
    pub api_addr: String,
    /// Raft address
    pub raft_addr: String,
    /// Current role
    pub role: String,
    /// Current status
    pub status: String,
    /// Is leader
    pub is_leader: bool,
    /// Can serve reads
    pub can_serve_reads: bool,
    /// Last heartbeat timestamp
    pub last_heartbeat: i64,
}

/// List nodes response
#[derive(Debug, Serialize, ToSchema)]
pub struct ListNodesResponse {
    /// All nodes in the cluster
    pub nodes: Vec<NodeInfo>,
    /// Total count
    pub count: usize,
}

/// Add node request
#[derive(Debug, Deserialize, ToSchema)]
pub struct AddNodeRequest {
    /// Node ID
    pub node_id: String,
    /// Raft address
    pub raft_addr: String,
    /// Optional API address
    pub api_addr: Option<String>,
}

/// Remove node request
#[derive(Debug, Deserialize, ToSchema)]
pub struct RemoveNodeRequest {
    /// Node ID to remove
    pub node_id: String,
}

/// Operation response
#[derive(Debug, Serialize, ToSchema)]
pub struct OperationResponse {
    /// Success status
    pub success: bool,
    /// Message
    pub message: String,
}

/// Create cluster routes
pub fn cluster_routes<R>() -> Router<Arc<AppState<R>>>
where
    R: reasondb_core::llm::ReasoningEngine + Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/status", get(get_cluster_status::<R>))
        .route("/nodes", get(list_nodes::<R>))
        .route("/nodes/add", post(add_node::<R>))
        .route("/nodes/remove", post(remove_node::<R>))
        .route("/leader", get(get_leader::<R>))
        .route("/health", get(cluster_health::<R>))
}

/// Get cluster status
#[utoipa::path(
    get,
    path = "/api/v1/cluster/status",
    responses(
        (status = 200, description = "Cluster status", body = ClusterStatusResponse),
    ),
    tag = "cluster"
)]
pub async fn get_cluster_status<
    R: reasondb_core::llm::ReasoningEngine + Clone + Send + Sync + 'static,
>(
    State(state): State<Arc<AppState<R>>>,
) -> crate::error::ApiResult<Json<ClusterStatusResponse>> {
    let response = if let Some(node) = &state.cluster_node {
        let status = node.status().await;
        ClusterStatusResponse {
            enabled: true,
            node_id: Some(status.node_id.to_string()),
            role: Some(format!("{:?}", status.role)),
            term: Some(status.term),
            leader_id: status.leader_id.map(|id| id.to_string()),
            node_count: status.nodes,
            last_applied: Some(status.last_applied),
            commit_index: Some(status.commit_index),
            has_quorum: status.has_quorum,
            cluster_name: state.config.cluster.cluster_name.clone(),
        }
    } else {
        ClusterStatusResponse {
            enabled: false,
            node_id: None,
            role: Some("Leader".to_string()), // Single node is always leader
            term: None,
            leader_id: None,
            node_count: 1,
            last_applied: None,
            commit_index: None,
            has_quorum: true,
            cluster_name: "standalone".to_string(),
        }
    };

    Ok(Json(response))
}

/// List all nodes in the cluster
#[utoipa::path(
    get,
    path = "/api/v1/cluster/nodes",
    responses(
        (status = 200, description = "List of cluster nodes", body = ListNodesResponse),
    ),
    tag = "cluster"
)]
pub async fn list_nodes<R: reasondb_core::llm::ReasoningEngine + Clone + Send + Sync + 'static>(
    State(state): State<Arc<AppState<R>>>,
) -> crate::error::ApiResult<Json<ListNodesResponse>> {
    let nodes = if let Some(node) = &state.cluster_node {
        let cluster_state = node.state_machine().state();
        let state_guard = cluster_state
            .read()
            .map_err(|_| ApiError::Internal("Failed to read cluster state".to_string()))?;

        state_guard
            .nodes
            .values()
            .map(|n| NodeInfo {
                id: n.id.to_string(),
                name: n.name.clone(),
                api_addr: n.api_addr.to_string(),
                raft_addr: n.raft_addr.to_string(),
                role: format!("{:?}", n.role),
                status: format!("{:?}", n.status),
                is_leader: n.is_leader(),
                can_serve_reads: n.can_serve_reads(),
                last_heartbeat: n.last_heartbeat,
            })
            .collect()
    } else {
        // Single node mode - return self as the only node
        vec![NodeInfo {
            id: "standalone".to_string(),
            name: "ReasonDB".to_string(),
            api_addr: format!("{}:{}", state.config.host, state.config.port),
            raft_addr: "N/A".to_string(),
            role: "Leader".to_string(),
            status: "Healthy".to_string(),
            is_leader: true,
            can_serve_reads: true,
            last_heartbeat: chrono::Utc::now().timestamp(),
        }]
    };

    let count = nodes.len();
    Ok(Json(ListNodesResponse { nodes, count }))
}

/// Add a node to the cluster
#[utoipa::path(
    post,
    path = "/api/v1/cluster/nodes/add",
    request_body = AddNodeRequest,
    responses(
        (status = 200, description = "Node added successfully", body = OperationResponse),
        (status = 400, description = "Invalid request"),
    ),
    tag = "cluster"
)]
pub async fn add_node<R: reasondb_core::llm::ReasoningEngine + Clone + Send + Sync + 'static>(
    State(state): State<Arc<AppState<R>>>,
    Json(req): Json<AddNodeRequest>,
) -> crate::error::ApiResult<Json<OperationResponse>> {
    let Some(node) = &state.cluster_node else {
        return Err(ApiError::BadRequest(
            "Clustering is not enabled".to_string(),
        ));
    };

    // Only leader can add nodes
    if !node.is_leader().await {
        return Err(ApiError::BadRequest(
            "Only leader can add nodes".to_string(),
        ));
    }

    let node_id = NodeId::new(&req.node_id);
    let raft_addr: std::net::SocketAddr = req
        .raft_addr
        .parse()
        .map_err(|_| ApiError::BadRequest("Invalid raft_addr format".to_string()))?;

    // Add peer to network
    node.add_peer(&node_id, raft_addr).await;

    Ok(Json(OperationResponse {
        success: true,
        message: format!("Node {} added to cluster", req.node_id),
    }))
}

/// Remove a node from the cluster
#[utoipa::path(
    post,
    path = "/api/v1/cluster/nodes/remove",
    request_body = RemoveNodeRequest,
    responses(
        (status = 200, description = "Node removed successfully", body = OperationResponse),
        (status = 400, description = "Invalid request"),
    ),
    tag = "cluster"
)]
pub async fn remove_node<R: reasondb_core::llm::ReasoningEngine + Clone + Send + Sync + 'static>(
    State(state): State<Arc<AppState<R>>>,
    Json(req): Json<RemoveNodeRequest>,
) -> crate::error::ApiResult<Json<OperationResponse>> {
    let Some(node) = &state.cluster_node else {
        return Err(ApiError::BadRequest(
            "Clustering is not enabled".to_string(),
        ));
    };

    // Only leader can remove nodes
    if !node.is_leader().await {
        return Err(ApiError::BadRequest(
            "Only leader can remove nodes".to_string(),
        ));
    }

    let node_id = NodeId::new(&req.node_id);
    node.remove_peer(&node_id).await;

    Ok(Json(OperationResponse {
        success: true,
        message: format!("Node {} removed from cluster", req.node_id),
    }))
}

/// Get current leader information
#[utoipa::path(
    get,
    path = "/api/v1/cluster/leader",
    responses(
        (status = 200, description = "Leader information", body = NodeInfo),
        (status = 503, description = "No leader elected"),
    ),
    tag = "cluster"
)]
pub async fn get_leader<R: reasondb_core::llm::ReasoningEngine + Clone + Send + Sync + 'static>(
    State(state): State<Arc<AppState<R>>>,
) -> crate::error::ApiResult<Json<NodeInfo>> {
    if let Some(node) = &state.cluster_node {
        let cluster_state = node.state_machine().state();
        let state_guard = cluster_state
            .read()
            .map_err(|_| ApiError::Internal("Failed to read cluster state".to_string()))?;

        if let Some(leader) = state_guard.get_leader() {
            return Ok(Json(NodeInfo {
                id: leader.id.to_string(),
                name: leader.name.clone(),
                api_addr: leader.api_addr.to_string(),
                raft_addr: leader.raft_addr.to_string(),
                role: "Leader".to_string(),
                status: format!("{:?}", leader.status),
                is_leader: true,
                can_serve_reads: leader.can_serve_reads(),
                last_heartbeat: leader.last_heartbeat,
            }));
        }

        // No leader elected
        return Err(ApiError::ServiceUnavailable(
            "No leader elected".to_string(),
        ));
    }

    // Single node mode - this node is leader
    Ok(Json(NodeInfo {
        id: "standalone".to_string(),
        name: "ReasonDB".to_string(),
        api_addr: format!("{}:{}", state.config.host, state.config.port),
        raft_addr: "N/A".to_string(),
        role: "Leader".to_string(),
        status: "Healthy".to_string(),
        is_leader: true,
        can_serve_reads: true,
        last_heartbeat: chrono::Utc::now().timestamp(),
    }))
}

/// Cluster health check
#[utoipa::path(
    get,
    path = "/api/v1/cluster/health",
    responses(
        (status = 200, description = "Cluster is healthy", body = ClusterHealthResponse),
        (status = 503, description = "Cluster is unhealthy"),
    ),
    tag = "cluster"
)]
pub async fn cluster_health<
    R: reasondb_core::llm::ReasoningEngine + Clone + Send + Sync + 'static,
>(
    State(state): State<Arc<AppState<R>>>,
) -> crate::error::ApiResult<Json<ClusterHealthResponse>> {
    if let Some(node) = &state.cluster_node {
        let status = node.status().await;

        let healthy = status.has_quorum && status.leader_id.is_some();

        if !healthy {
            return Err(ApiError::ServiceUnavailable(
                "Cluster does not have quorum".to_string(),
            ));
        }

        return Ok(Json(ClusterHealthResponse {
            healthy: true,
            has_quorum: status.has_quorum,
            has_leader: status.leader_id.is_some(),
            node_count: status.nodes,
            role: format!("{:?}", status.role),
        }));
    }

    // Single node is always healthy
    Ok(Json(ClusterHealthResponse {
        healthy: true,
        has_quorum: true,
        has_leader: true,
        node_count: 1,
        role: "Leader".to_string(),
    }))
}

/// Cluster health response
#[derive(Debug, Serialize, ToSchema)]
pub struct ClusterHealthResponse {
    /// Overall health status
    pub healthy: bool,
    /// Whether we have quorum
    pub has_quorum: bool,
    /// Whether a leader is elected
    pub has_leader: bool,
    /// Number of nodes
    pub node_count: usize,
    /// This node's role
    pub role: String,
}
