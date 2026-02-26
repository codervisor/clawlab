mod openclaw;
mod registry;
mod zeroclaw;

use std::sync::Arc;

use clawlab_core::ClawRuntime;

pub use openclaw::OpenClawAdapter;
pub use registry::AdapterRegistry;
pub use zeroclaw::ZeroClawAdapter;

pub fn builtin_registry() -> AdapterRegistry {
	let mut registry = AdapterRegistry::new();
	registry.register(ClawRuntime::OpenClaw, Arc::new(OpenClawAdapter));
	registry.register(ClawRuntime::ZeroClaw, Arc::new(ZeroClawAdapter));
	registry
}
