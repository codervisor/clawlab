use std::collections::HashMap;

use clawden_core::{
    current_unix_ms, AgentState, ChannelBinding, ChannelBindingStatus, ChannelConnectionStatus,
    ChannelInstanceConfig, ChannelType,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// In-memory store for channel configurations and token bindings.
#[derive(Default)]
pub struct ChannelStore {
    /// Channel instance configs keyed by instance_name.
    configs: HashMap<String, ChannelInstanceConfig>,
    /// Bindings keyed by (channel_type display, bot_token_hash).
    bindings: HashMap<(String, String), ChannelBinding>,
    /// Next binding id.
    next_binding_id: u64,
    /// Instance → channel assignments: agent_id → list of channel instance names.
    assignments: HashMap<String, Vec<String>>,
    /// Live connection status: (agent_id, channel_instance_name) → status.
    connection_status: HashMap<(String, String), ChannelConnectionStatus>,
    /// Last time channel health was refreshed (unix ms).
    last_health_check_unix_ms: Option<u64>,
}

/// Health report entry for a single channel instance.
#[derive(Debug, Clone, Serialize)]
pub struct ChannelHealthEntry {
    pub instance_name: String,
    pub channel_type: String,
    pub agent_id: Option<String>,
    pub status: ChannelConnectionStatus,
    pub last_checked_unix_ms: Option<u64>,
}

/// A detected conflict: same token bound to multiple instances.
#[derive(Debug, Clone, Serialize)]
pub struct BindingConflict {
    pub channel_type: String,
    pub bot_token_hash: String,
    pub instance_ids: Vec<String>,
}

/// Summary of a channel type's configured state.
#[derive(Debug, Clone, Serialize)]
pub struct ChannelTypeSummary {
    pub channel_type: String,
    pub instance_count: usize,
    pub connected: usize,
    pub disconnected: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChannelConfigRequest {
    pub instance_name: String,
    pub channel_type: String,
    #[serde(default)]
    pub credentials: HashMap<String, String>,
    #[serde(default)]
    pub options: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BindChannelRequest {
    pub instance_id: String,
    pub channel_type: String,
    pub bot_token: String,
}

impl ChannelStore {
    pub fn new() -> Self {
        Self::default()
    }

    // --- Channel configs ---

    pub fn upsert_config(
        &mut self,
        req: ChannelConfigRequest,
    ) -> Result<ChannelInstanceConfig, String> {
        let channel_type = ChannelType::from_str_loose(&req.channel_type)
            .ok_or_else(|| format!("unknown channel type: {}", req.channel_type))?;

        let config = ChannelInstanceConfig {
            instance_name: req.instance_name.clone(),
            channel_type,
            credentials: req.credentials,
            options: req.options,
        };
        self.configs.insert(req.instance_name, config.clone());
        Ok(config)
    }

    #[allow(dead_code)]
    pub fn get_config(&self, instance_name: &str) -> Option<&ChannelInstanceConfig> {
        self.configs.get(instance_name)
    }

    pub fn delete_config(&mut self, instance_name: &str) -> bool {
        self.configs.remove(instance_name).is_some()
    }

    #[allow(dead_code)]
    pub fn list_configs(&self) -> Vec<&ChannelInstanceConfig> {
        self.configs.values().collect()
    }

    pub fn list_configs_by_type(&self, channel_type: &ChannelType) -> Vec<&ChannelInstanceConfig> {
        self.configs
            .values()
            .filter(|c| &c.channel_type == channel_type)
            .collect()
    }

    // --- Channel type summaries ---

    pub fn list_channel_summaries(&self) -> Vec<ChannelTypeSummary> {
        let mut type_map: HashMap<String, (usize, usize, usize)> = HashMap::new();
        for config in self.configs.values() {
            let key = config.channel_type.to_string();
            let entry = type_map.entry(key).or_insert((0, 0, 0));
            entry.0 += 1;
        }
        // Check connection statuses
        for ((_, channel_name), status) in &self.connection_status {
            if let Some(config) = self.configs.get(channel_name) {
                let key = config.channel_type.to_string();
                if let Some(entry) = type_map.get_mut(&key) {
                    match status {
                        ChannelConnectionStatus::Connected | ChannelConnectionStatus::Proxied => {
                            entry.1 += 1;
                        }
                        _ => {
                            entry.2 += 1;
                        }
                    }
                }
            }
        }
        type_map
            .into_iter()
            .map(
                |(channel_type, (instance_count, connected, disconnected))| ChannelTypeSummary {
                    channel_type,
                    instance_count,
                    connected,
                    disconnected,
                },
            )
            .collect()
    }

    // --- Bindings ---

    pub fn bind(
        &mut self,
        instance_id: String,
        channel_type: &str,
        bot_token: &str,
    ) -> Result<ChannelBinding, String> {
        let ct = ChannelType::from_str_loose(channel_type)
            .ok_or_else(|| format!("unknown channel type: {channel_type}"))?;
        let token_hash = hash_token(bot_token);
        let key = (ct.to_string(), token_hash.clone());

        // Check uniqueness: reject if already bound to a different instance
        if let Some(existing) = self.bindings.get(&key) {
            if existing.status == ChannelBindingStatus::Active
                && existing.instance_id != instance_id
            {
                return Err(format!(
                    "token already bound to instance {}",
                    existing.instance_id
                ));
            }
        }

        let now = current_unix_ms();
        let binding = ChannelBinding {
            instance_id,
            channel_type: ct,
            bot_token_hash: token_hash,
            status: ChannelBindingStatus::Active,
            bound_at_unix_ms: now,
        };
        self.bindings.insert(key, binding.clone());
        self.next_binding_id += 1;
        Ok(binding)
    }

    pub fn unbind(&mut self, binding_id: usize) -> Result<ChannelBinding, String> {
        // Find by index (simple approach for in-memory store)
        let keys: Vec<_> = self.bindings.keys().cloned().collect();
        let key = keys
            .get(binding_id)
            .ok_or_else(|| format!("binding {binding_id} not found"))?
            .clone();
        if let Some(binding) = self.bindings.get_mut(&key) {
            binding.status = ChannelBindingStatus::Released;
            Ok(binding.clone())
        } else {
            Err(format!("binding {binding_id} not found"))
        }
    }

    pub fn list_bindings(&self) -> Vec<ChannelBinding> {
        self.bindings.values().cloned().collect()
    }

    pub fn detect_conflicts(&self) -> Vec<BindingConflict> {
        // Group active bindings by (channel_type, token_hash)
        let mut groups: HashMap<(String, String), Vec<String>> = HashMap::new();
        for binding in self.bindings.values() {
            if binding.status == ChannelBindingStatus::Active {
                let key = (
                    binding.channel_type.to_string(),
                    binding.bot_token_hash.clone(),
                );
                groups
                    .entry(key)
                    .or_default()
                    .push(binding.instance_id.clone());
            }
        }
        groups
            .into_iter()
            .filter(|(_, ids)| ids.len() > 1)
            .map(
                |((channel_type, bot_token_hash), instance_ids)| BindingConflict {
                    channel_type,
                    bot_token_hash,
                    instance_ids,
                },
            )
            .collect()
    }

    // --- Assignments ---

    pub fn assign_channel(&mut self, agent_id: &str, channel_instance_name: &str) {
        let list = self.assignments.entry(agent_id.to_string()).or_default();
        if !list.contains(&channel_instance_name.to_string()) {
            list.push(channel_instance_name.to_string());
        }
    }

    #[allow(dead_code)]
    pub fn unassign_channel(&mut self, agent_id: &str, channel_instance_name: &str) {
        if let Some(list) = self.assignments.get_mut(agent_id) {
            list.retain(|n| n != channel_instance_name);
        }
    }

    pub fn get_agent_channels(&self, agent_id: &str) -> Vec<&ChannelInstanceConfig> {
        self.assignments
            .get(agent_id)
            .map(|names| names.iter().filter_map(|n| self.configs.get(n)).collect())
            .unwrap_or_default()
    }

    // --- Connection status ---

    pub fn set_connection_status(
        &mut self,
        agent_id: &str,
        channel_name: &str,
        status: ChannelConnectionStatus,
    ) {
        self.connection_status
            .insert((agent_id.to_string(), channel_name.to_string()), status);
    }

    pub fn get_connection_status(
        &self,
        agent_id: &str,
        channel_name: &str,
    ) -> ChannelConnectionStatus {
        self.connection_status
            .get(&(agent_id.to_string(), channel_name.to_string()))
            .cloned()
            .unwrap_or(ChannelConnectionStatus::Disconnected)
    }

    /// Refresh channel health based on current agent states.
    ///
    /// For each (agent_id, channel_instance) assignment:
    /// - If the agent is Running → status becomes Connected (or Proxied if flagged)
    /// - Otherwise → Disconnected
    ///
    /// Unassigned channel instances are left as Disconnected (no entry).
    ///
    /// `proxy_pairs` is a set of `(agent_id, channel_instance_name)` pairs that
    /// should be marked Proxied rather than Connected (populated by the caller
    /// using adapter metadata from the LifecycleManager).
    pub fn refresh_channel_health(
        &mut self,
        agent_states: &HashMap<String, AgentState>,
        proxy_pairs: &std::collections::HashSet<(String, String)>,
    ) {
        let now = current_unix_ms();
        self.last_health_check_unix_ms = Some(now);

        // For each assignment, derive the new connection status
        for (agent_id, instance_names) in &self.assignments {
            let agent_running = matches!(
                agent_states.get(agent_id),
                Some(AgentState::Running)
            );
            for instance_name in instance_names {
                let status = if agent_running {
                    if proxy_pairs.contains(&(agent_id.clone(), instance_name.clone())) {
                        ChannelConnectionStatus::Proxied
                    } else {
                        ChannelConnectionStatus::Connected
                    }
                } else {
                    ChannelConnectionStatus::Disconnected
                };
                self.connection_status
                    .insert((agent_id.clone(), instance_name.clone()), status);
            }
        }
    }

    /// Return a per-instance health report across all configured channels.
    pub fn channel_health_report(&self) -> Vec<ChannelHealthEntry> {
        let mut report: Vec<ChannelHealthEntry> = Vec::new();

        // Build a reverse map: instance_name → (agent_id, status)
        let mut instance_agent: HashMap<String, (String, ChannelConnectionStatus)> = HashMap::new();
        for ((agent_id, instance_name), status) in &self.connection_status {
            instance_agent.insert(instance_name.clone(), (agent_id.clone(), status.clone()));
        }

        for config in self.configs.values() {
            let (agent_id, status) = instance_agent
                .get(&config.instance_name)
                .map(|(aid, s)| (Some(aid.clone()), s.clone()))
                .unwrap_or((None, ChannelConnectionStatus::Disconnected));

            report.push(ChannelHealthEntry {
                instance_name: config.instance_name.clone(),
                channel_type: config.channel_type.to_string(),
                agent_id,
                status,
                last_checked_unix_ms: self.last_health_check_unix_ms,
            });
        }

        // Sort for deterministic output
        report.sort_by(|a, b| a.instance_name.cmp(&b.instance_name));
        report
    }

    /// Build the full channel × runtime matrix.
    pub fn build_matrix(
        &self,
        agents: &[(String, String)], // (agent_id, runtime_name)
    ) -> Vec<MatrixRow> {
        let mut rows = Vec::new();
        for config in self.configs.values() {
            let mut statuses = Vec::new();
            for (agent_id, runtime) in agents {
                let status = self.get_connection_status(agent_id, &config.instance_name);
                statuses.push(MatrixCell {
                    agent_id: agent_id.clone(),
                    runtime: runtime.clone(),
                    status,
                });
            }
            rows.push(MatrixRow {
                channel_instance: config.instance_name.clone(),
                channel_type: config.channel_type.to_string(),
                cells: statuses,
            });
        }
        rows
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct MatrixRow {
    pub channel_instance: String,
    pub channel_type: String,
    pub cells: Vec<MatrixCell>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MatrixCell {
    pub agent_id: String,
    pub runtime: String,
    pub status: ChannelConnectionStatus,
}

fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    format!("{:x}", hasher.finalize())
}
