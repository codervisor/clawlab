use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use clawlab_adapters::AdapterRegistry;
use clawlab_core::{
    AgentConfig, AgentHandle, AgentMessage, AgentResponse, ClawRuntime, HealthStatus,
};
use serde::Serialize;

use crate::audit::{AuditEvent, AuditLog};
use crate::lifecycle::AgentState;

#[derive(Debug, Clone, Serialize)]
pub struct AgentRecord {
    pub id: String,
    pub name: String,
    pub runtime: ClawRuntime,
    pub capabilities: Vec<String>,
    pub state: AgentState,
    pub task_count: u64,
    pub health: HealthStatus,
}

pub struct LifecycleManager {
    adapters: AdapterRegistry,
    agents: HashMap<String, AgentRecord>,
    handles: HashMap<String, AgentHandle>,
    next_id: AtomicU64,
    round_robin_index: usize,
}

impl LifecycleManager {
    pub fn new(adapters: AdapterRegistry) -> Self {
        Self {
            adapters,
            agents: HashMap::new(),
            handles: HashMap::new(),
            next_id: AtomicU64::new(1),
            round_robin_index: 0,
        }
    }

    pub fn register_agent(&mut self, name: String, runtime: ClawRuntime, capabilities: Vec<String>) -> AgentRecord {
        let id = format!("agent-{}", self.next_id.fetch_add(1, Ordering::Relaxed));
        let record = AgentRecord {
            id: id.clone(),
            name,
            runtime,
            capabilities,
            state: AgentState::Registered,
            task_count: 0,
            health: HealthStatus::Unknown,
        };
        self.agents.insert(id, record.clone());
        record
    }

    pub fn list_agents(&self) -> Vec<AgentRecord> {
        let mut agents: Vec<_> = self.agents.values().cloned().collect();
        agents.sort_by(|a, b| a.id.cmp(&b.id));
        agents
    }

    pub async fn start_agent(&mut self, agent_id: &str) -> Result<AgentRecord, String> {
        let Some(record) = self.agents.get_mut(agent_id) else {
            return Err(format!("agent {agent_id} not found"));
        };

        let Some(adapter) = self.adapters.get(&record.runtime) else {
            return Err(format!("no adapter registered for runtime {:?}", record.runtime));
        };

        if !record.state.can_transition_to(AgentState::Running)
            && record.state != AgentState::Registered
        {
            return Err(format!(
                "invalid state transition from {:?} to running",
                record.state
            ));
        }

        let config = AgentConfig {
            name: record.name.clone(),
            runtime: record.runtime.clone(),
            model: None,
        };

        let handle = adapter
            .start(&config)
            .await
            .map_err(|e| format!("failed to start agent: {e}"))?;

        record.state = AgentState::Running;
        record.health = HealthStatus::Unknown;
        self.handles.insert(agent_id.to_string(), handle);
        Ok(record.clone())
    }

    pub async fn stop_agent(&mut self, agent_id: &str) -> Result<AgentRecord, String> {
        let Some(record) = self.agents.get_mut(agent_id) else {
            return Err(format!("agent {agent_id} not found"));
        };

        let Some(handle) = self.handles.get(agent_id) else {
            if record.state.can_transition_to(AgentState::Stopped) {
                record.state = AgentState::Stopped;
            }
            return Ok(record.clone());
        };

        let Some(adapter) = self.adapters.get(&record.runtime) else {
            return Err(format!("no adapter registered for runtime {:?}", record.runtime));
        };

        adapter
            .stop(handle)
            .await
            .map_err(|e| format!("failed to stop agent: {e}"))?;

        self.handles.remove(agent_id);
        if record.state.can_transition_to(AgentState::Stopped) {
            record.state = AgentState::Stopped;
        }
        Ok(record.clone())
    }

    pub async fn refresh_health(&mut self) -> Vec<AgentRecord> {
        let ids: Vec<String> = self.agents.keys().cloned().collect();
        for id in ids {
            let Some(record) = self.agents.get_mut(&id) else {
                continue;
            };
            let Some(handle) = self.handles.get(&id) else {
                record.health = HealthStatus::Unknown;
                continue;
            };
            let Some(adapter) = self.adapters.get(&record.runtime) else {
                record.health = HealthStatus::Unknown;
                continue;
            };
            match adapter.health(handle).await {
                Ok(health) => record.health = health,
                Err(_) => {
                    record.health = HealthStatus::Degraded;
                    if record.state.can_transition_to(AgentState::Degraded) {
                        record.state = AgentState::Degraded;
                    }
                }
            }
        }

        self.list_agents()
    }

    pub async fn route_and_send(
        &mut self,
        required_capabilities: &[String],
        message: String,
        target_agent_id: Option<String>,
    ) -> Result<(AgentRecord, AgentResponse), String> {
        let selected_id = if let Some(id) = target_agent_id {
            id
        } else {
            self.select_agent(required_capabilities)?
        };

        let Some(record) = self.agents.get_mut(&selected_id) else {
            return Err(format!("agent {selected_id} not found"));
        };

        if record.state != AgentState::Running {
            return Err(format!("agent {} is not running", record.id));
        }

        let Some(handle) = self.handles.get(&selected_id) else {
            return Err(format!("agent {} has no active handle", record.id));
        };

        let Some(adapter) = self.adapters.get(&record.runtime) else {
            return Err(format!("no adapter registered for runtime {:?}", record.runtime));
        };

        let response = adapter
            .send(
                handle,
                &AgentMessage {
                    role: "user".to_string(),
                    content: message,
                },
            )
            .await
            .map_err(|e| format!("send failed: {e}"))?;

        record.task_count += 1;
        Ok((record.clone(), response))
    }

    fn select_agent(&mut self, required_capabilities: &[String]) -> Result<String, String> {
        let eligible: Vec<&AgentRecord> = self
            .agents
            .values()
            .filter(|agent| {
                agent.state == AgentState::Running
                    && required_capabilities
                        .iter()
                        .all(|cap| agent.capabilities.iter().any(|agent_cap| agent_cap == cap))
            })
            .collect();

        if eligible.is_empty() {
            return Err("no running agent matches required capabilities".to_string());
        }

        let idx = self.round_robin_index % eligible.len();
        self.round_robin_index = self.round_robin_index.wrapping_add(1);
        Ok(eligible[idx].id.clone())
    }
}

pub fn append_audit(audit: &Arc<AuditLog>, action: &str, target: &str) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_millis() as u64;

    audit.append(AuditEvent {
        actor: "api".to_string(),
        action: action.to_string(),
        target: target.to_string(),
        timestamp_unix_ms: now,
    });
}

#[cfg(test)]
mod tests {
    use clawlab_adapters::builtin_registry;

    use super::LifecycleManager;
    use clawlab_core::ClawRuntime;

    #[test]
    fn registers_and_lists_agents() {
        let mut manager = LifecycleManager::new(builtin_registry());
        manager.register_agent("alpha".to_string(), ClawRuntime::ZeroClaw, vec!["chat".to_string()]);

        let listed = manager.list_agents();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "alpha");
    }
}
