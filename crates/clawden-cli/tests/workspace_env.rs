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
fn workspace_status_reads_dotenv_without_clawden_yaml() {
    let dir = temp_dir("workspace-status-dotenv");
    let home = dir.join("home");
    let workspace_path = dir.join("persisted-workspace");

    fs::create_dir_all(&home).expect("home should be created");
    fs::write(
        dir.join(".env"),
        format!("CLAWDEN_MEMORY_PATH={}\n", workspace_path.display()),
    )
    .expect("dotenv should be written");

    let output = Command::new(binary_path())
        .current_dir(&dir)
        .env("HOME", &home)
        .args(["workspace", "status"])
        .output()
        .expect("workspace status should run");
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(&format!(
        "No workspace repository at {}",
        workspace_path.display()
    )));
}
