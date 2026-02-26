use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use clawlab_core::ClawRuntime;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::audit::AuditLog;
use crate::lifecycle::AgentState;
use crate::manager::{append_audit, AgentRecord, LifecycleManager};

#[derive(Clone)]
pub struct AppState {
    pub manager: Arc<RwLock<LifecycleManager>>,
    pub audit: Arc<AuditLog>,
}

#[derive(Debug, Deserialize)]
pub struct RegisterAgentRequest {
    pub name: String,
    pub runtime: ClawRuntime,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct SendTaskRequest {
    pub message: String,
    #[serde(default)]
    pub required_capabilities: Vec<String>,
    pub agent_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct FleetStatusResponse {
    pub total_agents: usize,
    pub running_agents: usize,
    pub degraded_agents: usize,
}

#[derive(Debug, Serialize)]
pub struct TaskSendResponse {
    pub agent: AgentRecord,
    pub content: String,
}

pub async fn register_agent(
    State(state): State<AppState>,
    Json(request): Json<RegisterAgentRequest>,
) -> (StatusCode, Json<AgentRecord>) {
    let mut manager = state.manager.write().await;
    let record = manager.register_agent(request.name, request.runtime, request.capabilities);
    append_audit(&state.audit, "agent.register", &record.id);
    (StatusCode::CREATED, Json(record))
}

pub async fn list_agents(State(state): State<AppState>) -> Json<Vec<AgentRecord>> {
    let manager = state.manager.read().await;
    Json(manager.list_agents())
}

pub async fn start_agent(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
) -> Result<Json<AgentRecord>, (StatusCode, String)> {
    let mut manager = state.manager.write().await;
    let record = manager
        .start_agent(&agent_id)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    append_audit(&state.audit, "agent.start", &agent_id);
    Ok(Json(record))
}

pub async fn stop_agent(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
) -> Result<Json<AgentRecord>, (StatusCode, String)> {
    let mut manager = state.manager.write().await;
    let record = manager
        .stop_agent(&agent_id)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    append_audit(&state.audit, "agent.stop", &agent_id);
    Ok(Json(record))
}

pub async fn health_summary(State(state): State<AppState>) -> Json<Vec<AgentRecord>> {
    let mut manager = state.manager.write().await;
    Json(manager.refresh_health().await)
}

pub async fn fleet_status(State(state): State<AppState>) -> Json<FleetStatusResponse> {
    let manager = state.manager.read().await;
    let agents = manager.list_agents();

    Json(FleetStatusResponse {
        total_agents: agents.len(),
        running_agents: agents
            .iter()
            .filter(|agent| agent.state == AgentState::Running)
            .count(),
        degraded_agents: agents
            .iter()
            .filter(|agent| agent.state == AgentState::Degraded)
            .count(),
    })
}

pub async fn send_task(
    State(state): State<AppState>,
    Json(request): Json<SendTaskRequest>,
) -> Result<Json<TaskSendResponse>, (StatusCode, String)> {
    let mut manager = state.manager.write().await;
    let (agent, response) = manager
        .route_and_send(
            &request.required_capabilities,
            request.message,
            request.agent_id.clone(),
        )
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    append_audit(&state.audit, "task.send", &agent.id);

    Ok(Json(TaskSendResponse {
        agent,
        content: response.content,
    }))
}

pub async fn audit_log(State(state): State<AppState>) -> Json<Vec<crate::audit::AuditEvent>> {
    Json(state.audit.list())
}
