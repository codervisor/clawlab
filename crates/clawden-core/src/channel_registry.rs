use crate::ChannelType;

#[derive(Debug, Clone)]
pub struct ChannelDescriptor {
    pub channel_type: ChannelType,
    pub token_env_var: &'static str,
    pub required_credentials: &'static [&'static str],
    pub optional_credentials: &'static [&'static str],
}

pub static CHANNELS: &[ChannelDescriptor] = &[
    ChannelDescriptor {
        channel_type: ChannelType::Telegram,
        token_env_var: "TELEGRAM_BOT_TOKEN",
        required_credentials: &["token"],
        optional_credentials: &[],
    },
    ChannelDescriptor {
        channel_type: ChannelType::Discord,
        token_env_var: "DISCORD_BOT_TOKEN",
        required_credentials: &["token"],
        optional_credentials: &[],
    },
    ChannelDescriptor {
        channel_type: ChannelType::Slack,
        token_env_var: "SLACK_BOT_TOKEN",
        required_credentials: &["bot_token", "app_token"],
        optional_credentials: &[],
    },
    ChannelDescriptor {
        channel_type: ChannelType::Whatsapp,
        token_env_var: "WHATSAPP_BOT_TOKEN",
        required_credentials: &["token"],
        optional_credentials: &["phone"],
    },
    ChannelDescriptor {
        channel_type: ChannelType::Signal,
        token_env_var: "SIGNAL_BOT_TOKEN",
        required_credentials: &["phone"],
        optional_credentials: &["token"],
    },
    ChannelDescriptor {
        channel_type: ChannelType::Feishu,
        token_env_var: "FEISHU_BOT_TOKEN",
        required_credentials: &["token"],
        optional_credentials: &[],
    },
];

const WELL_KNOWN_CHANNEL_ENV_VARS: &[&str] = &[
    "TELEGRAM_BOT_TOKEN",
    "DISCORD_BOT_TOKEN",
    "SLACK_BOT_TOKEN",
    "SLACK_APP_TOKEN",
];

pub fn channel_descriptors() -> &'static [ChannelDescriptor] {
    CHANNELS
}

pub fn channel_descriptor(name: &str) -> Option<&'static ChannelDescriptor> {
    let channel_type = ChannelType::from_str_loose(name)?;
    CHANNELS
        .iter()
        .find(|descriptor| descriptor.channel_type == channel_type)
}

pub fn channel_token_env_name(channel: &str) -> String {
    channel_descriptor(channel)
        .map(|descriptor| descriptor.token_env_var.to_string())
        .unwrap_or_else(|| {
            format!(
                "{}_BOT_TOKEN",
                channel.to_ascii_uppercase().replace('-', "_")
            )
        })
}

pub fn known_channel_env_vars() -> &'static [&'static str] {
    WELL_KNOWN_CHANNEL_ENV_VARS
}
