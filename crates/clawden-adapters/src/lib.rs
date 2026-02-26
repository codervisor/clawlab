mod nanoclaw;
mod openclaw;
mod picoclaw;
mod registry;
mod zeroclaw;

use std::sync::Arc;

use clawden_core::ClawRuntime;

pub use nanoclaw::NanoClawAdapter;
pub use openclaw::OpenClawAdapter;
pub use picoclaw::PicoClawAdapter;
pub use registry::AdapterRegistry;
pub use zeroclaw::ZeroClawAdapter;

pub fn builtin_registry() -> AdapterRegistry {
	let mut registry = AdapterRegistry::new();
	registry.register(ClawRuntime::OpenClaw, Arc::new(OpenClawAdapter));
	registry.register(ClawRuntime::ZeroClaw, Arc::new(ZeroClawAdapter));
	registry.register(ClawRuntime::PicoClaw, Arc::new(PicoClawAdapter));
	registry.register(ClawRuntime::NanoClaw, Arc::new(NanoClawAdapter));
	registry
}
