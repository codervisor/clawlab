use anyhow::Result;

use crate::util::{append_audit_file, parse_runtime};

pub fn exec_init(runtime: String, multi: bool, force: bool) -> Result<()> {
    let yaml_path = std::env::current_dir()?.join("clawden.yaml");
    if yaml_path.exists() && !force {
        anyhow::bail!("clawden.yaml already exists. Use --force to overwrite.");
    }

    let _ = parse_runtime(&runtime)?;

    let yaml_content = if multi {
        format!(
            "# ClawDen multi-runtime config\n\
             # Docs: https://github.com/codervisor/clawden\n\n\
             channels:\n\
             #  my-telegram:\n\
             #    type: telegram\n\
             #    token: $TELEGRAM_BOT_TOKEN\n\n\
             providers:\n\
             #  main:\n\
             #    type: openai\n\
             #    api_key: $OPENAI_API_KEY\n\n\
             runtimes:\n\
             \x20 - name: {}\n\
             #    channels: [my-telegram]\n\
             \x20   tools: [git, http]\n\
             #    provider: main\n\
             #    model: gpt-4\n",
            runtime
        )
    } else {
        format!(
            "# ClawDen single-runtime config\n\
             # Docs: https://github.com/codervisor/clawden\n\n\
             runtime: {}\n\n\
             channels: {{}}\n\
             #  telegram:\n\
             #    token: $TELEGRAM_BOT_TOKEN\n\n\
             tools:\n\
             \x20 - git\n\
             \x20 - http\n\n\
             # provider: openai\n\
             # model: gpt-4\n",
            runtime
        )
    };

    std::fs::write(&yaml_path, &yaml_content)?;
    println!("Created {}", yaml_path.display());

    let env_path = yaml_path.parent().unwrap().join(".env");
    if !env_path.exists() {
        std::fs::write(
            &env_path,
            "# ClawDen environment variables\n\
             # TELEGRAM_BOT_TOKEN=\n\
             # OPENAI_API_KEY=\n\
             # ANTHROPIC_API_KEY=\n",
        )?;
        println!("Created {}", env_path.display());
    }

    append_audit_file("project.init", &runtime, "ok")?;
    Ok(())
}
