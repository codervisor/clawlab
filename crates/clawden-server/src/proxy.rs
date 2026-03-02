use clawden_core::{AgentMessage, AgentResponse, ChannelSupport, ChannelType, RuntimeMetadata};
use serde::Serialize;

/// Channel proxy status for a proxied connection.
#[derive(Debug, Clone, Serialize)]
pub struct ProxyStatus {
    pub channel_type: String,
    pub runtime: String,
    pub is_proxied: bool,
    pub reason: Option<String>,
}

/// Determines whether a channel needs proxying for a given runtime by checking
/// the runtime's native channel support from adapter metadata.
pub fn needs_proxy(metadata: &RuntimeMetadata, channel: &ChannelType) -> bool {
    match metadata.channel_support.get(channel) {
        Some(ChannelSupport::Native) | Some(ChannelSupport::Via(_)) => false,
        Some(ChannelSupport::Unsupported) | None => true,
    }
}

/// Build a proxy status report for a runtime × channel combination.
pub fn proxy_status(metadata: &RuntimeMetadata, channel: &ChannelType) -> ProxyStatus {
    let proxied = needs_proxy(metadata, channel);
    ProxyStatus {
        channel_type: channel.to_string(),
        runtime: format!("{:?}", metadata.runtime),
        is_proxied: proxied,
        reason: if proxied {
            Some(format!(
                "{:?} does not natively support {}; ClawDen will proxy",
                metadata.runtime, channel
            ))
        } else {
            None
        },
    }
}

/// Proxy a message from an unsupported channel to a runtime via CRI send().
/// This is the core bridge function: receive message on channel X, relay to
/// runtime through its CRI adapter, and return the response.
///
/// In a real deployment, this would be called by the channel webhook handler.
/// The channel adapter (e.g., Telegram bot) receives a message, determines the
/// target runtime doesn't natively support this channel, and routes through
/// this proxy.
#[allow(dead_code)]
pub fn create_proxy_message(
    channel_type: &ChannelType,
    sender: &str,
    content: &str,
) -> AgentMessage {
    AgentMessage {
        role: format!("proxy:{}", channel_type),
        content: format!("[{sender}] {content}"),
    }
}

/// Format a proxied response for sending back to the channel.
#[allow(dead_code)]
pub fn format_proxy_response(response: &AgentResponse) -> String {
    response.content.clone()
}

#[cfg(test)]
mod tests {
    use super::needs_proxy;
    use clawden_adapters::{NanoClawAdapter, OpenClawAdapter, PicoClawAdapter, ZeroClawAdapter};
    use clawden_core::{ChannelType, ClawAdapter, ClawRuntime, RuntimeMetadata};
    use std::collections::HashMap;

    #[test]
    fn tier3_signal_proxy_matrix_matches_support_model() {
        let open = OpenClawAdapter.metadata();
        let zero = ZeroClawAdapter.metadata();
        let nano = NanoClawAdapter.metadata();
        let pico = PicoClawAdapter.metadata();

        assert!(!needs_proxy(&open, &ChannelType::Signal));
        assert!(!needs_proxy(&zero, &ChannelType::Signal));
        assert!(needs_proxy(&nano, &ChannelType::Signal));
        assert!(needs_proxy(&pico, &ChannelType::Signal));
    }

    #[test]
    fn tier3_dingtalk_proxy_matrix_matches_support_model() {
        let open = OpenClawAdapter.metadata();
        let zero = ZeroClawAdapter.metadata();
        let nano = NanoClawAdapter.metadata();
        let pico = PicoClawAdapter.metadata();

        assert!(needs_proxy(&open, &ChannelType::Dingtalk));
        assert!(needs_proxy(&zero, &ChannelType::Dingtalk));
        assert!(needs_proxy(&nano, &ChannelType::Dingtalk));
        assert!(!needs_proxy(&pico, &ChannelType::Dingtalk));
    }

    #[test]
    fn tier3_qq_proxy_matrix_matches_support_model() {
        let open = OpenClawAdapter.metadata();
        let zero = ZeroClawAdapter.metadata();
        let nano = NanoClawAdapter.metadata();
        let pico = PicoClawAdapter.metadata();

        assert!(needs_proxy(&open, &ChannelType::Qq));
        assert!(needs_proxy(&zero, &ChannelType::Qq));
        assert!(needs_proxy(&nano, &ChannelType::Qq));
        assert!(!needs_proxy(&pico, &ChannelType::Qq));
    }

    #[test]
    fn telegram_is_proxied_when_runtime_has_no_native_support() {
        let metadata = RuntimeMetadata {
            runtime: ClawRuntime::NullClaw,
            version: "test".to_string(),
            language: "zig".to_string(),
            capabilities: vec![],
            default_port: None,
            config_format: Some("json".to_string()),
            channel_support: HashMap::new(),
        };
        assert!(needs_proxy(&metadata, &ChannelType::Telegram));
    }
}
