mod api;
mod audit;
mod lifecycle;
mod manager;

use crate::api::{
    audit_log, fleet_status, health_summary, list_agents, register_agent, send_task, start_agent,
    stop_agent, AppState,
};
use crate::audit::{AuditEvent, AuditLog};
use crate::lifecycle::AgentState;
use crate::manager::{append_audit, LifecycleManager};
use axum::{routing::get, Json, Router};
use serde::Serialize;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tracing::info;

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    service: &'static str,
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: "clawlab-server",
    })
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_target(false)
        .compact()
        .init();

    let audit_store = Arc::new(AuditLog::default());
    let manager = LifecycleManager::new(clawlab_adapters::builtin_registry());
    let shared_state = AppState {
        manager: Arc::new(RwLock::new(manager)),
        audit: audit_store.clone(),
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/agents", get(list_agents))
        .route("/agents/register", axum::routing::post(register_agent))
        .route("/agents/{agent_id}/start", axum::routing::post(start_agent))
        .route("/agents/{agent_id}/stop", axum::routing::post(stop_agent))
        .route("/agents/health", get(health_summary))
        .route("/fleet/status", get(fleet_status))
        .route("/task/send", axum::routing::post(send_task))
        .route("/audit", get(audit_log))
        .with_state(shared_state);
    let addr = SocketAddr::from(([127, 0, 0, 1], 8080));

    let startup_event = AuditEvent {
        actor: "system".to_string(),
        action: "server.start".to_string(),
        target: "clawlab-server".to_string(),
        timestamp_unix_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before UNIX_EPOCH")
            .as_millis() as u64,
    };
    audit_store.append(startup_event);
    if let Some(last) = audit_store.list().last() {
        info!(
            actor = %last.actor,
            action = %last.action,
            target = %last.target,
            timestamp_unix_ms = last.timestamp_unix_ms,
            "audit event recorded"
        );
    }

    let lifecycle_path_valid = AgentState::Registered.can_transition_to(AgentState::Installed)
        && AgentState::Installed.can_transition_to(AgentState::Running);
    let known_states = [
        AgentState::Registered,
        AgentState::Installed,
        AgentState::Running,
        AgentState::Stopped,
        AgentState::Degraded,
    ];
    info!(
        lifecycle_path_valid,
        known_state_count = known_states.len(),
        "lifecycle transition check"
    );

    append_audit(&audit_store, "server.ready", "clawlab-server");

    info!(%addr, "starting clawlab server");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("failed to bind TCP listener");

    axum::serve(listener, app)
        .await
        .expect("server failed unexpectedly");
}
