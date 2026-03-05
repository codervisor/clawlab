use crate::ChannelType;

#[derive(Debug, Clone)]
pub struct ChannelDescriptor {
    pub channel_type: ChannelType,
    pub token_env_var: &'static str,
    pub required_credentials: &'static [&'static str],
    pub optional_credentials: &'static [&'static str],
    pub supports_allowed_users: bool,
    pub extra_env_vars: &'static [(&'static str, &'static str)],
}

pub static CHANNELS: &[ChannelDescriptor] = &[
    ChannelDescriptor {
        channel_type: ChannelType::Telegram,
        token_env_var: "TELEGRAM_BOT_TOKEN",
        required_credentials: &["bot_token"],
        optional_credentials: &[],
        supports_allowed_users: true,
        extra_env_vars: &[],
    },
    ChannelDescriptor {
        channel_type: ChannelType::Discord,
        token_env_var: "DISCORD_BOT_TOKEN",
        required_credentials: &["bot_token"],
        optional_credentials: &["guild_id"],
        supports_allowed_users: false,
        extra_env_vars: &[],
    },
    ChannelDescriptor {
        channel_type: ChannelType::Slack,
        token_env_var: "SLACK_BOT_TOKEN",
        required_credentials: &["bot_token", "app_token"],
        optional_credentials: &[],
        supports_allowed_users: false,
        extra_env_vars: &[("app_token", "SLACK_APP_TOKEN")],
    },
    ChannelDescriptor {
        channel_type: ChannelType::Whatsapp,
        token_env_var: "WHATSAPP_BOT_TOKEN",
        required_credentials: &["token"],
        optional_credentials: &["phone"],
        supports_allowed_users: false,
        extra_env_vars: &[("phone", "WHATSAPP_PHONE")],
    },
    ChannelDescriptor {
        channel_type: ChannelType::Signal,
        token_env_var: "SIGNAL_BOT_TOKEN",
        required_credentials: &["phone"],
        optional_credentials: &["token"],
        supports_allowed_users: false,
        extra_env_vars: &[("phone", "SIGNAL_PHONE")],
    },
    ChannelDescriptor {
        channel_type: ChannelType::Feishu,
        token_env_var: "FEISHU_APP_ID",
        required_credentials: &["app_id", "app_secret"],
        optional_credentials: &[],
        supports_allowed_users: false,
        extra_env_vars: &[("app_secret", "FEISHU_APP_SECRET")],
    },
];

const WELL_KNOWN_CHANNEL_ENV_VARS: &[&str] = &[
    "TELEGRAM_BOT_TOKEN",
    "DISCORD_BOT_TOKEN",
    "SLACK_BOT_TOKEN",
    "SLACK_APP_TOKEN",
    "FEISHU_APP_ID",
    "FEISHU_APP_SECRET",
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
