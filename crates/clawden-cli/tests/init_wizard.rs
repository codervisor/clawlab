use clawden_config::ClawDenYaml;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
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
fn init_non_interactive_generates_valid_config_and_env() {
    let dir = temp_dir("init-non-interactive");

    let status = Command::new(binary_path())
        .current_dir(&dir)
        .args([
            "init",
            "--non-interactive",
            "--force",
            "--runtime",
            "zeroclaw",
        ])
        .status()
        .expect("init command should run");
    assert!(status.success());

    let yaml = fs::read_to_string(dir.join("clawden.yaml")).expect("yaml should exist");
    let parsed = ClawDenYaml::parse_yaml(&yaml).expect("yaml should parse");
    assert_eq!(parsed.runtime.as_deref(), Some("zeroclaw"));

    let env_file = fs::read_to_string(dir.join(".env")).expect("env should exist");
    assert!(env_file.contains("OPENAI_API_KEY="));

    let gitignore = fs::read_to_string(dir.join(".gitignore")).expect("gitignore should exist");
    assert!(gitignore.contains(".clawden/"));
}

#[test]
fn init_template_telegram_bot_writes_expected_fields() {
    let dir = temp_dir("init-template");

    let status = Command::new(binary_path())
        .current_dir(&dir)
        .args([
            "init",
            "--template",
            "telegram-bot",
            "--force",
            "--runtime",
            "zeroclaw",
            "--yes",
        ])
        .status()
        .expect("template init should run");
    assert!(status.success());

    let yaml = fs::read_to_string(dir.join("clawden.yaml")).expect("yaml should exist");
    assert!(yaml.contains("telegram:"));
    assert!(yaml.contains("token: $TELEGRAM_BOT_TOKEN"));
}

#[test]
fn init_interactive_accepts_stdin_flow() {
    let dir = temp_dir("init-interactive");

    let mut child = Command::new(binary_path())
        .current_dir(&dir)
        .args(["init", "--force", "--runtime", "zeroclaw"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("interactive init should spawn");

    let input = b"1\nzeroclaw\nn\n\n1\nn\n\n";
    child
        .stdin
        .take()
        .expect("stdin should be available")
        .write_all(input)
        .expect("stdin should accept input");

    let output = child.wait_with_output().expect("process should finish");
    assert!(output.status.success());

    let yaml = fs::read_to_string(dir.join("clawden.yaml")).expect("yaml should exist");
    let parsed = ClawDenYaml::parse_yaml(&yaml).expect("yaml should parse");
    assert_eq!(parsed.runtime.as_deref(), Some("zeroclaw"));
}

#[test]
fn init_force_overwrites_existing_config() {
    let dir = temp_dir("init-force-overwrite");

    let first = Command::new(binary_path())
        .current_dir(&dir)
        .args([
            "init",
            "--non-interactive",
            "--force",
            "--runtime",
            "zeroclaw",
        ])
        .status()
        .expect("initial init should run");
    assert!(first.success());

    let second = Command::new(binary_path())
        .current_dir(&dir)
        .args(["init", "--force", "--yes", "--runtime", "zeroclaw"])
        .status()
        .expect("force overwrite should run");
    assert!(second.success());

    let yaml = fs::read_to_string(dir.join("clawden.yaml")).expect("yaml should exist");
    let parsed = ClawDenYaml::parse_yaml(&yaml).expect("yaml should parse");
    assert_eq!(parsed.runtime.as_deref(), Some("zeroclaw"));
    assert_eq!(parsed.model.as_deref(), Some("gpt-4o-mini"));
}

#[test]
fn init_non_interactive_fails_when_yaml_exists_without_force() {
    let dir = temp_dir("init-no-force");

    let first = Command::new(binary_path())
        .current_dir(&dir)
        .args([
            "init",
            "--non-interactive",
            "--force",
            "--runtime",
            "zeroclaw",
        ])
        .status()
        .expect("initial init should run");
    assert!(first.success());

    let second = Command::new(binary_path())
        .current_dir(&dir)
        .args(["init", "--non-interactive", "--runtime", "zeroclaw"])
        .status()
        .expect("init should run");
    assert!(
        !second.success(),
        "should fail when yaml exists without --force"
    );
}
