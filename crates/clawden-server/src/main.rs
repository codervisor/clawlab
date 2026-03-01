mod api;
mod proxy;

use crate::api::{
    agent_channels, agent_logs, agent_metrics_history, audit_log, binding_conflicts,
    channel_instances, channel_matrix, channel_support_matrix, create_binding, create_team,
    delete_binding, delete_channel_config, deploy_runtime, deploy_status, fan_out_task,
    fleet_status, get_channel_config, health_summary, list_agents, list_bindings, list_channels,
    list_endpoints, list_runtimes, list_swarm_tasks, list_teams, proxy_status_endpoint,
    register_agent, register_endpoint, restart_agent, scan_endpoints, send_task, start_agent,
    stop_agent, test_channel, upsert_channel_config, AppState,
};
use axum::{routing::get, Json, Router};
use clawden_core::{
    append_audit, AgentState, AuditEvent, AuditLog, ChannelStore, DiscoveryService,
    LifecycleManager, SwarmCoordinator,
};
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

fn build_app(shared_state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/agents", get(list_agents))
        .route("/agents/register", axum::routing::post(register_agent))
        .route("/agents/{agent_id}/start", axum::routing::post(start_agent))
        .route("/agents/{agent_id}/stop", axum::routing::post(stop_agent))
        .route("/agents/health", get(health_summary))
        .route("/fleet/status", get(fleet_status))
        .route("/task/send", axum::routing::post(send_task))
        .route("/audit", get(audit_log))
        .route("/discovery/endpoints", get(list_endpoints))
        .route(
            "/discovery/endpoints/register",
            axum::routing::post(register_endpoint),
        )
        .route("/discovery/scan", axum::routing::post(scan_endpoints))
        .route("/swarm/teams", get(list_teams))
        .route("/swarm/teams/create", axum::routing::post(create_team))
        .route("/swarm/fan-out", axum::routing::post(fan_out_task))
        .route("/swarm/tasks", get(list_swarm_tasks))
        .route("/runtimes", get(list_runtimes))
        .route(
            "/runtimes/{runtime}/deploy",
            axum::routing::post(deploy_runtime),
        )
        .route("/agents/{agent_id}/deploy-status", get(deploy_status))
        .route(
            "/agents/{agent_id}/restart",
            axum::routing::post(restart_agent),
        )
        .route("/agents/{agent_id}/logs", get(agent_logs))
        .route(
            "/agents/{agent_id}/metrics/history",
            get(agent_metrics_history),
        )
        .route(
            "/agents/{agent_id}/proxy-status/{channel_type}",
            get(proxy_status_endpoint),
        )
        .route("/channels", get(list_channels))
        .route(
            "/channels/{channel_type}",
            get(get_channel_config)
                .put(upsert_channel_config)
                .delete(delete_channel_config),
        )
        .route("/channels/{channel_type}/instances", get(channel_instances))
        .route(
            "/channels/{channel_type}/test",
            axum::routing::post(test_channel),
        )
        .route("/agents/{agent_id}/channels", get(agent_channels))
        .route("/channels/matrix", get(channel_matrix))
        .route("/channels/support-matrix", get(channel_support_matrix))
        .route(
            "/channels/bindings",
            get(list_bindings).post(create_binding),
        )
        .route(
            "/channels/bindings/{binding_id}",
            axum::routing::delete(delete_binding),
        )
        .route("/channels/bindings/conflicts", get(binding_conflicts))
        .with_state(shared_state)
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_target(false)
        .compact()
        .init();

    let audit_store = Arc::new(AuditLog::default());
    let registry = clawden_adapters::builtin_registry();
    let manager = LifecycleManager::new(registry.adapters_map());
    let shared_state = AppState {
        manager: Arc::new(RwLock::new(manager)),
        audit: audit_store.clone(),
        discovery: Arc::new(RwLock::new(DiscoveryService::new())),
        swarm: Arc::new(RwLock::new(SwarmCoordinator::new())),
        channels: Arc::new(RwLock::new(ChannelStore::new())),
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

            append_audit(&monitor_audit, "api", "health.tick", "fleet");
            info!(
                checked_agents = recovered.len(),
                interval_ms = health_interval_ms,
                recovery_base_backoff_ms,
                "health monitor tick"
            );
        }
    });

    let app = build_app(shared_state);
    let port = std::env::var("CLAWDEN_SERVER_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(8080);
    let addr = SocketAddr::from(([127, 0, 0, 1], port));

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

    append_audit(&audit_store, "system", "server.ready", "clawden-server");

    info!(%addr, "starting clawden server");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("failed to bind TCP listener");

    axum::serve(listener, app)
        .await
        .expect("server failed unexpectedly");
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::util::ServiceExt;

    fn test_state() -> AppState {
        let registry = clawden_adapters::builtin_registry();
        let manager = LifecycleManager::new(registry.adapters_map());
        AppState {
            manager: Arc::new(RwLock::new(manager)),
            audit: Arc::new(AuditLog::default()),
            discovery: Arc::new(RwLock::new(DiscoveryService::new())),
            swarm: Arc::new(RwLock::new(SwarmCoordinator::new())),
            channels: Arc::new(RwLock::new(ChannelStore::new())),
        }
    }

    #[tokio::test]
    async fn core_api_endpoints_are_reachable() {
        let app = build_app(test_state());
        let endpoints = [
            "/health",
            "/agents",
            "/agents/health",
            "/fleet/status",
            "/runtimes",
            "/channels",
            "/audit",
        ];

        for endpoint in endpoints {
            let request = Request::builder()
                .uri(endpoint)
                .body(Body::empty())
                .expect("request should build");
            let response = app
                .clone()
                .oneshot(request)
                .await
                .expect("request should succeed");
            assert_eq!(
                response.status(),
                StatusCode::OK,
                "endpoint {endpoint} returned unexpected status"
            );
        }
    }
}
