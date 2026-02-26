use serde::Serialize;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Serialize)]
pub struct AuditEvent {
    pub actor: String,
    pub action: String,
    pub target: String,
    pub timestamp_unix_ms: u64,
}

#[derive(Clone, Default)]
pub struct AuditLog {
    inner: Arc<Mutex<Vec<AuditEvent>>>,
}

impl AuditLog {
    pub fn append(&self, event: AuditEvent) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.push(event);
        }
    }

    pub fn list(&self) -> Vec<AuditEvent> {
        self.inner
            .lock()
            .map_or_else(|_| Vec::new(), |guard| guard.clone())
    }
}
