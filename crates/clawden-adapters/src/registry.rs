use std::collections::HashMap;
use std::sync::Arc;

use clawden_core::{ClawAdapter, ClawRuntime};

#[derive(Default)]
pub struct AdapterRegistry {
    adapters: HashMap<ClawRuntime, Arc<dyn ClawAdapter>>,
}

impl AdapterRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, runtime: ClawRuntime, adapter: Arc<dyn ClawAdapter>) {
        self.adapters.insert(runtime, adapter);
    }

    pub fn get(&self, runtime: &ClawRuntime) -> Option<Arc<dyn ClawAdapter>> {
        self.adapters.get(runtime).cloned()
    }

    pub fn list(&self) -> Vec<ClawRuntime> {
        let mut runtimes: Vec<_> = self.adapters.keys().cloned().collect();
        runtimes.sort_by_key(|runtime| format!("{runtime:?}"));
        runtimes
    }

    pub fn has(&self, runtime: &ClawRuntime) -> bool {
        self.adapters.contains_key(runtime)
    }

    pub fn detect_runtime_for_capability(&self, capability: &str) -> Option<ClawRuntime> {
        self.adapters.iter().find_map(|(runtime, adapter)| {
            let supports = adapter
                .metadata()
                .capabilities
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(capability));
            if supports {
                Some(runtime.clone())
            } else {
                None
            }
        })
    }
}
