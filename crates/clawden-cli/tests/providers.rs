use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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

fn start_ok_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("server should bind");
    let addr = listener
        .local_addr()
        .expect("server addr should be available");
    thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buffer = [0u8; 1024];
            let _ = stream.read(&mut buffer);
            let response =
                b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\n\r\n{}";
            let _ = stream.write_all(response);
        }
    });
    format!("http://{addr}/v1")
}

#[test]
fn providers_test_reports_success_and_failure() {
    let dir = temp_dir("providers-test");
    let home = dir.join("home");
    fs::create_dir_all(&home).expect("home should be created");

    let base_url = start_ok_server();
    let yaml = format!(
        "runtime: zeroclaw\nproviders:\n  openai:\n    api_key: sk-test\n    base_url: \"{base_url}\"\n  anthropic: {{}}\n"
    );
    fs::write(dir.join("clawden.yaml"), yaml).expect("yaml should be written");

    let output = Command::new(binary_path())
        .current_dir(&dir)
        .env("HOME", &home)
        .args(["providers", "test"])
        .output()
        .expect("providers test should run");
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("provider=openai\ttest=ok"));
    assert!(stdout.contains("provider=anthropic\ttest=fail\terror=missing api_key"));
}

#[test]
fn providers_list_redacts_api_keys() {
    let dir = temp_dir("providers-list-redaction");
    let home = dir.join("home");
    fs::create_dir_all(&home).expect("home should be created");

    let yaml =
        "runtime: zeroclaw\nproviders:\n  openai:\n    api_key: sk-visible-should-not-print\n";
    fs::write(dir.join("clawden.yaml"), yaml).expect("yaml should be written");

    let output = Command::new(binary_path())
        .current_dir(&dir)
        .env("HOME", &home)
        .args(["providers"])
        .output()
        .expect("providers command should run");
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("provider=openai\tstatus=configured"));
    assert!(!stdout.contains("sk-visible-should-not-print"));
}

#[test]
fn up_passes_provider_env_to_runtime_process() {
    let dir = temp_dir("up-provider-env");
    let home = dir.join("home");
    let project = dir.join("project");
    let dump_path = dir.join("runtime.env");

    fs::create_dir_all(&home).expect("home should be created");
    fs::create_dir_all(&project).expect("project should be created");

    let runtime_dir = home
        .join(".clawden")
        .join("runtimes")
        .join("zeroclaw")
        .join("latest");
    fs::create_dir_all(&runtime_dir).expect("runtime directory should be created");

    let executable = runtime_dir.join("zeroclaw");
    fs::write(
        &executable,
        "#!/usr/bin/env sh\nprintenv > \"$CLAWDEN_ENV_DUMP_FILE\"\nexit 0\n",
    )
    .expect("runtime script should be written");

    let mut perms = fs::metadata(&executable)
        .expect("metadata should be available")
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&executable, perms).expect("runtime script should be executable");

    let current_link = home
        .join(".clawden")
        .join("runtimes")
        .join("zeroclaw")
        .join("current");
    std::os::unix::fs::symlink("latest", current_link).expect("current symlink should be created");

    let yaml = r#"
runtime: zeroclaw
provider: openai
model: gpt-4o-mini
"#;
    fs::write(project.join("clawden.yaml"), yaml).expect("yaml should be written");

    let status = Command::new(binary_path())
        .current_dir(&project)
        .env("HOME", &home)
        .env("OPENAI_API_KEY", "sk-launch-test")
        .env("CLAWDEN_ENV_DUMP_FILE", &dump_path)
        .args(["up", "--no-docker", "--detach"])
        .status()
        .expect("up should run");
    assert!(status.success());

    let mut env_dump = None;
    for _ in 0..50 {
        if dump_path.exists() {
            let content = fs::read_to_string(&dump_path).expect("env dump should be readable");
            if content.contains("CLAWDEN_LLM_PROVIDER=openai") {
                env_dump = Some(content);
                break;
            }
        }
        thread::sleep(Duration::from_millis(20));
    }

    let env_dump = env_dump.expect("runtime should write environment dump");
    assert!(env_dump.contains("CLAWDEN_LLM_API_KEY=sk-launch-test"));
    assert!(env_dump.contains("CLAWDEN_LLM_PROVIDER=openai"));
    assert!(env_dump.contains("CLAWDEN_LLM_MODEL=gpt-4o-mini"));
    assert!(env_dump.contains("ZEROCLAW_LLM_API_KEY=sk-launch-test"));
}
