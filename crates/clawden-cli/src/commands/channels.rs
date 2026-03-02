use anyhow::Result;
use clawden_core::LifecycleManager;

use crate::cli::ChannelCommand;

pub fn exec_channels(
    command: Option<ChannelCommand>,
    manager: &mut LifecycleManager,
) -> Result<()> {
    match command {
        None => {
            let metadata = manager.list_runtime_metadata();
            for runtime in metadata {
                println!("{}", runtime.runtime.as_slug());
                for (channel, support) in runtime.channel_support {
                    println!("  {}: {:?}", channel, support);
                }
            }
        }
        Some(ChannelCommand::Test { channel_type }) => {
            if let Some(ct) = channel_type {
                println!(
                    "Channel config test for '{ct}' is available in dashboard server mode"
                );
            } else {
                println!("Channel config test requires a channel type");
            }
        }
    }
    Ok(())
}
