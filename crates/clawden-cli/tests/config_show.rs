use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_dir(name: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after unix epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("clawden-{name}-{stamp}"));
    fs::create_dir_all(&path).expect("temp dir should be created");
    path
}

fn binary_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_clawden"))
}

#[test]
fn config_show_env_redacts_and_reveal_unredacts() {
    let dir = temp_dir("config-show");
    let home = dir.join("home");
    fs::create_dir_all(&home).expect("home should be created");
    fs::write(
        dir.join("clawden.yaml"),
        "runtime: zeroclaw\nprovider: openai\nproviders:\n  openai:\n    api_key: sk-test\n",
    )
    .expect("yaml should be written");

    let output = Command::new(binary_path())
        .current_dir(&dir)
        .env("HOME", &home)
        .args(["config", "show", "--format", "env", "zeroclaw"])
        .output()
        .expect("config show should run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("CLAWDEN_LLM_API_KEY=<redacted>"));

    let reveal_output = Command::new(binary_path())
        .current_dir(&dir)
        .env("HOME", &home)
        .args(["config", "show", "--format", "env", "--reveal", "zeroclaw"])
        .output()
        .expect("config show reveal should run");
    assert!(reveal_output.status.success());
    let reveal_stdout = String::from_utf8_lossy(&reveal_output.stdout);
    assert!(reveal_stdout.contains("CLAWDEN_LLM_API_KEY=sk-test"));
}

#[test]
fn config_show_supports_json_format() {
    let dir = temp_dir("config-show-json");
    let home = dir.join("home");
    fs::create_dir_all(&home).expect("home should be created");
    fs::write(
        dir.join("clawden.yaml"),
        "runtime: zeroclaw\nprovider: openai\nproviders:\n  openai:\n    api_key: sk-test\n",
    )
    .expect("yaml should be written");

    let output = Command::new(binary_path())
        .current_dir(&dir)
        .env("HOME", &home)
        .args(["config", "show", "--format", "json", "zeroclaw"])
        .output()
        .expect("config show json should run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"runtime\": \"zeroclaw\""));
    assert!(stdout.contains("\"CLAWDEN_LLM_API_KEY\": \"<redacted>\""));
}

#[test]
fn config_show_uses_env_file_override() {
    let dir = temp_dir("config-show-env-file");
    let home = dir.join("home");
    fs::create_dir_all(&home).expect("home should be created");
    fs::write(
        dir.join("clawden.yaml"),
        "runtime: zeroclaw\nprovider: openai\nproviders:\n  openai:\n    api_key: $OPENAI_API_KEY\n",
    )
    .expect("yaml should be written");
    fs::write(dir.join("staging.env"), "OPENAI_API_KEY=sk-from-file\n").expect("env file write");

    let output = Command::new(binary_path())
        .current_dir(&dir)
        .env("HOME", &home)
        .args([
            "config",
            "show",
            "--format",
            "env",
            "--reveal",
            "--env-file",
            "staging.env",
            "zeroclaw",
        ])
        .output()
        .expect("config show env-file should run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("CLAWDEN_LLM_API_KEY=sk-from-file"));
}

#[test]
fn config_show_uses_env_file_override_without_clawden_yaml() {
    let dir = temp_dir("config-show-env-file-no-yaml");
    let home = dir.join("home");
    fs::create_dir_all(&home).expect("home should be created");
    fs::write(dir.join("staging.env"), "OPENAI_API_KEY=sk-from-file\n").expect("env file write");

    let output = Command::new(binary_path())
        .current_dir(&dir)
        .env("HOME", &home)
        .args([
            "config",
            "show",
            "--format",
            "env",
            "--reveal",
            "--env-file",
            "staging.env",
            "zeroclaw",
        ])
        .output()
        .expect("config show env-file should run without yaml");
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("OPENAI_API_KEY=sk-from-file"));
}
