use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use clawden_core::ClawRuntime;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::audit::AuditLog;
use crate::discovery::{DiscoveredEndpoint, DiscoveryMethod, DiscoveryService};
use crate::lifecycle::AgentState;
use crate::manager::{append_audit, AgentRecord, LifecycleManager};
use crate::swarm::{SwarmCoordinator, SwarmMember, SwarmRole};

#[derive(Clone)]
pub struct AppState {
    pub manager: Arc<RwLock<LifecycleManager>>,
    pub audit: Arc<AuditLog>,
    pub discovery: Arc<RwLock<DiscoveryService>>,
    pub swarm: Arc<RwLock<SwarmCoordinator>>,
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

// --- Discovery endpoints ---

#[derive(Debug, Deserialize)]
pub struct RegisterEndpointRequest {
    pub host: String,
    pub port: u16,
    pub method: Option<String>,
    pub runtime_hint: Option<String>,
}

pub async fn register_endpoint(
    State(state): State<AppState>,
    Json(req): Json<RegisterEndpointRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let method = match req.method.as_deref() {
        Some("network_scan") => DiscoveryMethod::NetworkScan,
        Some("dns_sd") => DiscoveryMethod::DnsSd,
        _ => DiscoveryMethod::Manual,
    };
    let mut discovery = state.discovery.write().await;
    let key = discovery.register_endpoint(DiscoveredEndpoint {
        host: req.host,
        port: req.port,
        method,
        runtime_hint: req.runtime_hint,
    });
    append_audit(&state.audit, "discovery.register", &key);
    (StatusCode::CREATED, Json(serde_json::json!({ "key": key })))
}

pub async fn list_endpoints(State(state): State<AppState>) -> Json<Vec<DiscoveredEndpoint>> {
    let discovery = state.discovery.read().await;
    Json(discovery.list_endpoints().into_iter().cloned().collect())
}

#[derive(Debug, Deserialize)]
pub struct ScanRequest {
    pub hosts: Vec<String>,
    pub ports: Vec<u16>,
}

pub async fn scan_endpoints(
    State(state): State<AppState>,
    Json(req): Json<ScanRequest>,
) -> Json<Vec<DiscoveredEndpoint>> {
    let discovery = state.discovery.read().await;
    Json(discovery.scan_ports(&req.hosts, &req.ports))
}

// --- Swarm endpoints ---

#[derive(Debug, Deserialize)]
pub struct CreateTeamRequest {
    pub name: String,
    pub members: Vec<SwarmMemberRequest>,
}

#[derive(Debug, Deserialize)]
pub struct SwarmMemberRequest {
    pub agent_id: String,
    pub role: String,
}

pub async fn create_team(
    State(state): State<AppState>,
    Json(req): Json<CreateTeamRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let members: Vec<SwarmMember> = req
        .members
        .into_iter()
        .map(|m| SwarmMember {
            agent_id: m.agent_id,
            role: match m.role.as_str() {
                "leader" => SwarmRole::Leader,
                "reviewer" => SwarmRole::Reviewer,
                _ => SwarmRole::Worker,
            },
        })
        .collect();

    let mut swarm = state.swarm.write().await;
    let team = swarm.create_team(req.name.clone(), members);
    let response = serde_json::to_value(team).unwrap_or_default();
    append_audit(&state.audit, "swarm.create_team", &req.name);
    (StatusCode::CREATED, Json(response))
}

pub async fn list_teams(State(state): State<AppState>) -> Json<serde_json::Value> {
    let swarm = state.swarm.read().await;
    Json(serde_json::to_value(swarm.list_teams()).unwrap_or_default())
}

#[derive(Debug, Deserialize)]
pub struct FanOutRequest {
    pub team_name: String,
    pub task_description: String,
    pub subtask_descriptions: Vec<String>,
}

pub async fn fan_out_task(
    State(state): State<AppState>,
    Json(req): Json<FanOutRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let mut swarm = state.swarm.write().await;
    let tasks = swarm
        .fan_out(
            &req.team_name,
            &req.task_description,
            req.subtask_descriptions,
        )
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    let value = serde_json::to_value(&tasks).unwrap_or_default();
    append_audit(&state.audit, "swarm.fan_out", &req.team_name);
    Ok(Json(value))
}

pub async fn list_swarm_tasks(State(state): State<AppState>) -> Json<serde_json::Value> {
    let swarm = state.swarm.read().await;
    Json(serde_json::to_value(swarm.list_tasks(None)).unwrap_or_default())
}
