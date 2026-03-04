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
    PathBuf::from(env!("CARGO_BIN_EXE_clawden-cli"))
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
