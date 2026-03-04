use std::path::PathBuf;
use std::process::Command;

fn binary_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_clawden-cli"))
}

/// Spawn `clawden config env` with a clean environment (only PATH preserved) plus any
/// extra vars supplied by the caller.  This prevents host credentials from leaking
/// into the test assertions.
fn run_config_env(extra_vars: &[(&str, &str)], reveal: bool) -> std::process::Output {
    let path = std::env::var("PATH").unwrap_or_default();
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let mut cmd = Command::new(binary_path());
    cmd.env_clear().env("PATH", path).env("HOME", home);
    for (k, v) in extra_vars {
        cmd.env(k, v);
    }
    cmd.arg("config").arg("env");
    if reveal {
        cmd.arg("--reveal");
    }
    cmd.output().expect("config env should run")
}

#[test]
fn config_env_shows_check_and_cross_markers() {
    // With no relevant env vars set, every entry should show the ✗ (not set) marker.
    let output = run_config_env(&[], false);
    assert!(output.status.success(), "config env should exit 0");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("Detected environment variables:"),
        "should print header"
    );
    // Spot-check each section header
    assert!(
        stdout.contains("LLM Providers:"),
        "should contain LLM Providers section"
    );
    assert!(
        stdout.contains("Channel Tokens:"),
        "should contain Channel Tokens section"
    );
    assert!(
        stdout.contains("ClawDen Config:"),
        "should contain ClawDen Config section"
    );

    // All vars should be listed as not-set when no env provided
    assert!(
        stdout.contains("✗ not set"),
        "should show ✗ not set when no vars are present"
    );
    assert!(
        !stdout.contains("✓ set"),
        "should not show ✓ set when no vars are present"
    );
}

#[test]
fn config_env_shows_set_var_with_redaction() {
    // OPENROUTER_API_KEY is set — it should appear as ✓ set with a redacted value.
    let output = run_config_env(&[("OPENROUTER_API_KEY", "sk-or-v1-supersecretkey")], false);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("✓ set"),
        "OPENROUTER_API_KEY should appear as set"
    );
    // The value should be redacted (first 8 chars + ***)
    assert!(
        stdout.contains("sk-or-v1***"),
        "value should be redacted to first 8 chars + ***"
    );
    // The full secret must not appear
    assert!(
        !stdout.contains("supersecretkey"),
        "full secret must not appear without --reveal"
    );
}

#[test]
fn config_env_reveal_shows_full_values() {
    // --reveal should show the complete value instead of a redacted form.
    let output = run_config_env(&[("OPENROUTER_API_KEY", "sk-or-v1-supersecretkey")], true);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("sk-or-v1-supersecretkey"),
        "--reveal should show the full value"
    );
    assert!(
        !stdout.contains("***"),
        "--reveal must not redact the value"
    );
}

#[test]
fn config_env_shows_provider_label_for_provider_vars() {
    // Provider vars should show their provider name in parentheses after the status.
    let output = run_config_env(&[("OPENAI_API_KEY", "sk-openai-test1234")], false);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("(openai)"),
        "set provider var should include provider label"
    );
}

#[test]
fn config_env_channel_token_appears_as_not_set_when_absent() {
    // With no channel tokens set, TELEGRAM_BOT_TOKEN should show ✗.
    let output = run_config_env(&[], false);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("TELEGRAM_BOT_TOKEN"),
        "TELEGRAM_BOT_TOKEN should be listed"
    );
    assert!(
        stdout.contains("DISCORD_BOT_TOKEN"),
        "DISCORD_BOT_TOKEN should be listed"
    );
}

#[test]
fn config_env_channel_token_appears_as_set_when_present() {
    let output = run_config_env(&[("TELEGRAM_BOT_TOKEN", "123456:ABCdef")], false);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("✓ set"),
        "TELEGRAM_BOT_TOKEN should appear as set"
    );
    // Value should be redacted
    assert!(
        !stdout.contains("123456:ABCdef"),
        "token value should be redacted"
    );
}
