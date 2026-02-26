mod api;
mod audit;
mod discovery;
mod lifecycle;
mod manager;
mod swarm;

use crate::api::{
    audit_log, create_team, fan_out_task, fleet_status, health_summary, list_agents,
    list_endpoints, list_swarm_tasks, list_teams, register_agent, register_endpoint,
    scan_endpoints, send_task, start_agent, stop_agent, AppState,
};
use crate::audit::{AuditEvent, AuditLog};
use crate::discovery::DiscoveryService;
use crate::lifecycle::AgentState;
use crate::manager::{append_audit, LifecycleManager};
use crate::swarm::SwarmCoordinator;
use axum::{routing::get, Json, Router};
use serde::Serialize;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
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
        service: "clawden-server",
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
    let manager = LifecycleManager::new(clawden_adapters::builtin_registry());
    let shared_state = AppState {
        manager: Arc::new(RwLock::new(manager)),
        audit: audit_store.clone(),
        discovery: Arc::new(RwLock::new(DiscoveryService::new())),
        swarm: Arc::new(RwLock::new(SwarmCoordinator::new())),
    };

    let health_interval_ms = std::env::var("CLAWDEN_HEALTH_INTERVAL_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(5_000);
    let recovery_base_backoff_ms = std::env::var("CLAWDEN_RECOVERY_BASE_BACKOFF_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(1_000);

    let monitor_manager = shared_state.manager.clone();
    let monitor_audit = shared_state.audit.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(health_interval_ms));
        loop {
            interval.tick().await;
            let mut manager = monitor_manager.write().await;
            manager
                .refresh_health_with_base_backoff_ms(recovery_base_backoff_ms)
                .await;
            let recovered = manager.recover_degraded().await;
            drop(manager);

            append_audit(&monitor_audit, "health.tick", "fleet");
            info!(
                checked_agents = recovered.len(),
                interval_ms = health_interval_ms,
                recovery_base_backoff_ms,
                "health monitor tick"
            );
        }
    });

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
        // Discovery endpoints
        .route("/discovery/endpoints", get(list_endpoints))
        .route(
            "/discovery/endpoints/register",
            axum::routing::post(register_endpoint),
        )
        .route("/discovery/scan", axum::routing::post(scan_endpoints))
        // Swarm endpoints
        .route("/swarm/teams", get(list_teams))
        .route("/swarm/teams/create", axum::routing::post(create_team))
        .route("/swarm/fan-out", axum::routing::post(fan_out_task))
        .route("/swarm/tasks", get(list_swarm_tasks))
        .with_state(shared_state);
    let addr = SocketAddr::from(([127, 0, 0, 1], 8080));

    let startup_event = AuditEvent {
        actor: "system".to_string(),
        action: "server.start".to_string(),
        target: "clawden-server".to_string(),
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

    append_audit(&audit_store, "server.ready", "clawden-server");

    info!(%addr, "starting clawden server");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("failed to bind TCP listener");

    axum::serve(listener, app)
        .await
        .expect("server failed unexpectedly");
}
