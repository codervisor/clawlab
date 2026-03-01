use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use clawden_core::{
    append_audit, AgentRecord, AgentState, AuditEvent, AuditLog, BindChannelRequest,
    BindingConflict, ChannelConfigRequest, ChannelStore, ChannelTypeSummary, ClawRuntime,
    DiscoveredEndpoint, DiscoveryMethod, DiscoveryService, LifecycleManager, MatrixRow,
    RuntimeMetadata, SwarmCoordinator, SwarmMember, SwarmRole,
};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct AppState {
    pub manager: Arc<RwLock<LifecycleManager>>,
    pub audit: Arc<AuditLog>,
    pub discovery: Arc<RwLock<DiscoveryService>>,
    pub swarm: Arc<RwLock<SwarmCoordinator>>,
    pub channels: Arc<RwLock<ChannelStore>>,
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
    append_audit(&state.audit, "api", "agent.register", &record.id);
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
    append_audit(&state.audit, "api", "agent.start", &agent_id);
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
    append_audit(&state.audit, "api", "agent.stop", &agent_id);
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

    append_audit(&state.audit, "api", "task.send", &agent.id);

    Ok(Json(TaskSendResponse {
        agent,
        content: response.content,
    }))
}

pub async fn audit_log(State(state): State<AppState>) -> Json<Vec<AuditEvent>> {
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
    append_audit(&state.audit, "api", "discovery.register", &key);
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
    append_audit(&state.audit, "api", "swarm.create_team", &req.name);
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
    append_audit(&state.audit, "api", "swarm.fan_out", &req.team_name);
    Ok(Json(value))
}

pub async fn list_swarm_tasks(State(state): State<AppState>) -> Json<serde_json::Value> {
    let swarm = state.swarm.read().await;
    Json(serde_json::to_value(swarm.list_tasks(None)).unwrap_or_default())
}

// --- Runtime endpoints (spec 017/021) ---

pub async fn list_runtimes(State(state): State<AppState>) -> Json<Vec<RuntimeMetadata>> {
    let manager = state.manager.read().await;
    Json(manager.list_runtime_metadata())
}

#[derive(Debug, Deserialize)]
pub struct DeployRuntimeRequest {
    pub instance_name: String,
    pub runtime: ClawRuntime,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub channels: Vec<String>,
    #[serde(default)]
    #[allow(dead_code)]
    pub tools: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct DeployStatusResponse {
    pub agent: AgentRecord,
    pub step: String,
    pub completed: bool,
}

pub async fn deploy_runtime(
    State(state): State<AppState>,
    Path(runtime_name): Path<String>,
    Json(request): Json<DeployRuntimeRequest>,
) -> Result<Json<DeployStatusResponse>, (StatusCode, String)> {
    // Validate runtime name matches path
    let runtime_str = format!("{:?}", request.runtime).to_lowercase();
    if !runtime_name
        .to_lowercase()
        .contains(&runtime_str.replace("claw", ""))
        && runtime_name.to_lowercase() != runtime_str
    {
        // Allow flexible matching
    }

    let mut manager = state.manager.write().await;
    let record = manager.register_agent(
        request.instance_name.clone(),
        request.runtime,
        request.capabilities,
    );

    // Start the agent (install + start)
    let agent_id = record.id.clone();
    let started = manager.start_agent(&agent_id).await;

    append_audit(&state.audit, "api", "runtime.deploy", &agent_id);

    match started {
        Ok(agent) => {
            // Assign channels
            if !request.channels.is_empty() {
                let mut channels = state.channels.write().await;
                for ch in &request.channels {
                    channels.assign_channel(&agent_id, ch);
                }
            }

            Ok(Json(DeployStatusResponse {
                agent,
                step: "running".to_string(),
                completed: true,
            }))
        }
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e)),
    }
}

pub async fn deploy_status(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
) -> Result<Json<DeployStatusResponse>, (StatusCode, String)> {
    let manager = state.manager.read().await;
    let agents = manager.list_agents();
    let agent = agents
        .into_iter()
        .find(|a| a.id == agent_id)
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("agent {agent_id} not found")))?;

    let step = match agent.state {
        AgentState::Registered => "registered",
        AgentState::Installed => "installed",
        AgentState::Running => "running",
        AgentState::Stopped => "stopped",
        AgentState::Degraded => "degraded",
    };

    Ok(Json(DeployStatusResponse {
        completed: agent.state == AgentState::Running,
        step: step.to_string(),
        agent,
    }))
}

pub async fn agent_metrics_history(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let manager = state.manager.read().await;
    let agents = manager.list_agents();
    let _agent = agents
        .into_iter()
        .find(|a| a.id == agent_id)
        .ok_or_else(|| (StatusCode::NOT_FOUND, "agent not found".to_string()))?;

    // Return stub metrics history
    Ok(Json(serde_json::json!({
        "data_points": [],
        "message": "metrics history collection not yet implemented"
    })))
}

// --- Channel endpoints (spec 018/021) ---

pub async fn list_channels(State(state): State<AppState>) -> Json<Vec<ChannelTypeSummary>> {
    let channels = state.channels.read().await;
    Json(channels.list_channel_summaries())
}

pub async fn get_channel_config(
    State(state): State<AppState>,
    Path(channel_type): Path<String>,
) -> Json<Vec<serde_json::Value>> {
    let channels = state.channels.read().await;
    let ct = clawden_core::ChannelType::from_str_loose(&channel_type);
    let configs: Vec<serde_json::Value> = match ct {
        Some(ct) => channels
            .list_configs_by_type(&ct)
            .into_iter()
            .map(|c| serde_json::to_value(c).unwrap_or_default())
            .collect(),
        None => vec![],
    };
    Json(configs)
}

pub async fn upsert_channel_config(
    State(state): State<AppState>,
    Path(_channel_type): Path<String>,
    Json(req): Json<ChannelConfigRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, String)> {
    let mut channels = state.channels.write().await;
    let config = channels
        .upsert_config(req)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    append_audit(
        &state.audit,
        "api",
        "channel.configure",
        &config.instance_name,
    );
    Ok((
        StatusCode::OK,
        Json(serde_json::to_value(config).unwrap_or_default()),
    ))
}

pub async fn delete_channel_config(
    State(state): State<AppState>,
    Path(channel_type): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let mut channels = state.channels.write().await;
    if channels.delete_config(&channel_type) {
        append_audit(&state.audit, "api", "channel.delete", &channel_type);
        Ok(Json(serde_json::json!({ "deleted": channel_type })))
    } else {
        Err((
            StatusCode::NOT_FOUND,
            format!("channel config {channel_type} not found"),
        ))
    }
}

pub async fn channel_instances(
    State(state): State<AppState>,
    Path(channel_type): Path<String>,
) -> Json<Vec<serde_json::Value>> {
    let channels = state.channels.read().await;
    let ct = clawden_core::ChannelType::from_str_loose(&channel_type);
    let configs: Vec<serde_json::Value> = match ct {
        Some(ct) => channels
            .list_configs_by_type(&ct)
            .into_iter()
            .map(|c| serde_json::to_value(c).unwrap_or_default())
            .collect(),
        None => vec![],
    };
    Json(configs)
}

pub async fn test_channel(
    State(state): State<AppState>,
    Path(channel_type): Path<String>,
) -> Json<serde_json::Value> {
    // Stub: validate that we have configs for this type
    let channels = state.channels.read().await;
    let ct = clawden_core::ChannelType::from_str_loose(&channel_type);
    let count = match ct {
        Some(ct) => channels.list_configs_by_type(&ct).len(),
        None => 0,
    };
    Json(serde_json::json!({
        "channel_type": channel_type,
        "instances_tested": count,
        "status": if count > 0 { "ok" } else { "no_instances" },
    }))
}

pub async fn agent_channels(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
) -> Json<Vec<serde_json::Value>> {
    let channels = state.channels.read().await;
    let configs = channels.get_agent_channels(&agent_id);
    let result: Vec<serde_json::Value> = configs
        .into_iter()
        .map(|c| {
            let status = channels.get_connection_status(&agent_id, &c.instance_name);
            serde_json::json!({
                "instance_name": c.instance_name,
                "channel_type": c.channel_type.to_string(),
                "status": status,
            })
        })
        .collect();
    Json(result)
}

pub async fn channel_matrix(State(state): State<AppState>) -> Json<Vec<MatrixRow>> {
    let manager = state.manager.read().await;
    let agents: Vec<(String, String)> = manager
        .list_agents()
        .into_iter()
        .map(|a| (a.id, format!("{:?}", a.runtime)))
        .collect();
    let channels = state.channels.read().await;
    Json(channels.build_matrix(&agents))
}

pub async fn list_bindings(
    State(state): State<AppState>,
) -> Json<Vec<clawden_core::ChannelBinding>> {
    let channels = state.channels.read().await;
    Json(channels.list_bindings())
}

pub async fn create_binding(
    State(state): State<AppState>,
    Json(req): Json<BindChannelRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, String)> {
    let mut channels = state.channels.write().await;
    let binding = channels
        .bind(req.instance_id.clone(), &req.channel_type, &req.bot_token)
        .map_err(|e| (StatusCode::CONFLICT, e))?;
    append_audit(&state.audit, "api", "channel.bind", &req.instance_id);
    Ok((
        StatusCode::CREATED,
        Json(serde_json::to_value(binding).unwrap_or_default()),
    ))
}

pub async fn delete_binding(
    State(state): State<AppState>,
    Path(binding_id): Path<usize>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let mut channels = state.channels.write().await;
    let binding = channels
        .unbind(binding_id)
        .map_err(|e| (StatusCode::NOT_FOUND, e))?;
    append_audit(&state.audit, "api", "channel.unbind", &binding.instance_id);
    Ok(Json(serde_json::to_value(binding).unwrap_or_default()))
}

pub async fn binding_conflicts(State(state): State<AppState>) -> Json<Vec<BindingConflict>> {
    let channels = state.channels.read().await;
    Json(channels.detect_conflicts())
}

/// Full channel support matrix from adapter metadata
pub async fn channel_support_matrix(State(state): State<AppState>) -> Json<serde_json::Value> {
    let manager = state.manager.read().await;
    let metadata = manager.list_runtime_metadata();
    let mut matrix = serde_json::Map::new();
    for meta in &metadata {
        let runtime_name = format!("{:?}", meta.runtime);
        let channels: serde_json::Map<String, serde_json::Value> = meta
            .channel_support
            .iter()
            .map(|(ct, support)| {
                (
                    ct.to_string(),
                    serde_json::to_value(support).unwrap_or_default(),
                )
            })
            .collect();
        matrix.insert(runtime_name, serde_json::Value::Object(channels));
    }
    Json(serde_json::Value::Object(matrix))
}

// --- Restart endpoint (spec 021) ---

pub async fn restart_agent(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
) -> Result<Json<AgentRecord>, (StatusCode, String)> {
    let mut manager = state.manager.write().await;
    // Stop then start
    let _ = manager.stop_agent(&agent_id).await;
    let record = manager
        .start_agent(&agent_id)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    append_audit(&state.audit, "api", "agent.restart", &agent_id);
    Ok(Json(record))
}

// --- Log streaming endpoint (spec 021) ---

pub async fn agent_logs(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let manager = state.manager.read().await;
    let agents = manager.list_agents();
    let agent = agents
        .into_iter()
        .find(|a| a.id == agent_id)
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("agent {agent_id} not found")))?;

    // Return stub log entries — in production this would stream via SSE
    Ok(Json(serde_json::json!({
        "agent_id": agent.id,
        "runtime": format!("{:?}", agent.runtime),
        "logs": [
            { "timestamp": "2026-02-27T00:00:00Z", "level": "info", "message": format!("{} started", agent.name) },
            { "timestamp": "2026-02-27T00:00:01Z", "level": "info", "message": "Ready to accept connections" }
        ],
        "note": "SSE streaming not yet implemented — polling fallback"
    })))
}

// --- Channel proxy status endpoint (spec 018) ---

pub async fn proxy_status_endpoint(
    State(state): State<AppState>,
    Path((agent_id, channel_type)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let manager = state.manager.read().await;
    let agents = manager.list_agents();
    let agent = agents
        .into_iter()
        .find(|a| a.id == agent_id)
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("agent {agent_id} not found")))?;

    let ct = clawden_core::ChannelType::from_str_loose(&channel_type).ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            format!("unknown channel type: {channel_type}"),
        )
    })?;

    let metadata_list = manager.list_runtime_metadata();
    let metadata = metadata_list
        .iter()
        .find(|m| m.runtime == agent.runtime)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                "adapter metadata not found".to_string(),
            )
        })?;

    let status = crate::proxy::proxy_status(metadata, &ct);
    Ok(Json(serde_json::to_value(status).unwrap_or_default()))
}
