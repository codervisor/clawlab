#[cfg(feature = "nanoclaw")]
mod nanoclaw;
#[cfg(feature = "openclaw")]
mod openclaw;
#[cfg(feature = "picoclaw")]
mod picoclaw;
mod registry;
#[cfg(feature = "zeroclaw")]
mod zeroclaw;

use std::sync::Arc;

use clawden_core::ClawRuntime;

#[cfg(feature = "nanoclaw")]
pub use nanoclaw::NanoClawAdapter;
#[cfg(feature = "openclaw")]
pub use openclaw::OpenClawAdapter;
#[cfg(feature = "picoclaw")]
pub use picoclaw::PicoClawAdapter;
pub use registry::AdapterRegistry;
#[cfg(feature = "zeroclaw")]
pub use zeroclaw::ZeroClawAdapter;

/// Creates a registry pre-populated with all compile-time enabled adapters.
pub fn builtin_registry() -> AdapterRegistry {
    let mut registry = AdapterRegistry::new();

    #[cfg(feature = "openclaw")]
    registry.register(ClawRuntime::OpenClaw, Arc::new(OpenClawAdapter));

    #[cfg(feature = "zeroclaw")]
    registry.register(ClawRuntime::ZeroClaw, Arc::new(ZeroClawAdapter));

    #[cfg(feature = "picoclaw")]
    registry.register(ClawRuntime::PicoClaw, Arc::new(PicoClawAdapter));

    #[cfg(feature = "nanoclaw")]
    registry.register(ClawRuntime::NanoClaw, Arc::new(NanoClawAdapter));

    tracing::info!(
        adapter_count = registry.list().len(),
        "built-in adapter registry initialized"
    );
    registry
}
