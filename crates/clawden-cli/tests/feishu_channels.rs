use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
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

fn cli_command(dir: &PathBuf, home: &PathBuf) -> Command {
    let mut command = Command::new(binary_path());
    command
        .current_dir(dir)
        .env("HOME", home)
        .env_remove("FEISHU_APP_ID")
        .env_remove("FEISHU_APP_SECRET");
    command
}

fn read_http_request(stream: &mut std::net::TcpStream) -> String {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 1024];
    let mut header_end = None;
    let mut content_length = 0usize;

    loop {
        let read = stream.read(&mut chunk).expect("request should read");
        if read == 0 {
            break;
        }

        buffer.extend_from_slice(&chunk[..read]);

        if header_end.is_none() {
            header_end = buffer.windows(4).position(|window| window == b"\r\n\r\n");
            if let Some(index) = header_end {
                let header_bytes = &buffer[..index + 4];
                let headers = String::from_utf8_lossy(header_bytes);
                content_length = headers
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        if !name.eq_ignore_ascii_case("content-length") {
                            return None;
                        }

                        value.trim().parse::<usize>().ok()
                    })
                    .unwrap_or(0);
            }
        }

        if let Some(index) = header_end {
            let body_start = index + 4;
            if buffer.len() >= body_start + content_length {
                break;
            }
        }
    }

    String::from_utf8_lossy(&buffer).into_owned()
}

fn start_feishu_success_server(expected_app_id: Option<&str>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("server should bind");
    let addr = listener
        .local_addr()
        .expect("server addr should be available");
    let expected_app_id = expected_app_id.map(ToOwned::to_owned);

    thread::spawn(move || {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().expect("request should arrive");
            let request = read_http_request(&mut stream);

            let response_body = if request
                .contains("POST /open-apis/auth/v3/tenant_access_token/internal")
            {
                if let Some(app_id) = &expected_app_id {
                    assert!(
                        request.contains(&format!("\"app_id\":\"{app_id}\"")),
                        "expected app_id {app_id} in auth request: {request}"
                    );
                }
                r#"{"code":0,"tenant_access_token":"tenant-token"}"#.to_string()
            } else if request.contains("GET /open-apis/bot/v3/info") {
                r#"{"code":0,"data":{"bot_name":"ClawDen Helper","open_id":"ou_test"}}"#.to_string()
            } else {
                panic!("unexpected request: {request}");
            };

            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            stream
                .write_all(response.as_bytes())
                .expect("response should write");
        }
    });

    format!("http://{addr}")
}

fn start_feishu_invalid_credentials_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("server should bind");
    let addr = listener
        .local_addr()
        .expect("server addr should be available");

    thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("request should arrive");
        let _request = read_http_request(&mut stream);
        let body = r#"{"code":99991663,"msg":"invalid app secret"}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        stream
            .write_all(response.as_bytes())
            .expect("response should write");
    });

    format!("http://{addr}")
}

fn start_feishu_bot_disabled_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("server should bind");
    let addr = listener
        .local_addr()
        .expect("server addr should be available");

    thread::spawn(move || {
        for step in 0..2 {
            let (mut stream, _) = listener.accept().expect("request should arrive");
            let request = read_http_request(&mut stream);
            let (status_line, body) = if step == 0 {
                assert!(request.contains("POST /open-apis/auth/v3/tenant_access_token/internal"));
                (
                    "HTTP/1.1 200 OK",
                    r#"{"code":0,"tenant_access_token":"tenant-token"}"#,
                )
            } else {
                assert!(request.contains("GET /open-apis/bot/v3/info"));
                (
                    "HTTP/1.1 403 Forbidden",
                    r#"{"code":99991400,"msg":"bot capability not enabled"}"#,
                )
            };

            let response = format!(
                "{status_line}\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            stream
                .write_all(response.as_bytes())
                .expect("response should write");
        }
    });

    format!("http://{addr}")
}

#[test]
#[ignore]
fn feishu_verify_succeeds_with_flag_credentials() {
    let dir = temp_dir("feishu-verify-flags");
    let home = dir.join("home");
    fs::create_dir_all(&home).expect("home should exist");
    let base_url = start_feishu_success_server(Some("cli_test"));

    let output = cli_command(&dir, &home)
        .env("CLAWDEN_FEISHU_API_BASE_URL", &base_url)
        .args([
            "channels",
            "feishu",
            "verify",
            "--app-id",
            "cli_test",
            "--app-secret",
            "secret_test",
        ])
        .output()
        .expect("verify should run");
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Feishu app verification:"));
    assert!(stdout.contains("Credentials:      valid (tenant token obtained)"));
    assert!(stdout.contains("Bot capability:   enabled"));
    assert!(stdout.contains("Bot name:         ClawDen Helper"));
}

#[test]
#[ignore]
fn feishu_verify_reports_invalid_credentials() {
    let dir = temp_dir("feishu-verify-invalid");
    let home = dir.join("home");
    fs::create_dir_all(&home).expect("home should exist");
    let base_url = start_feishu_invalid_credentials_server();

    let output = cli_command(&dir, &home)
        .env("CLAWDEN_FEISHU_API_BASE_URL", &base_url)
        .args([
            "channels",
            "feishu",
            "verify",
            "--app-id",
            "cli_test",
            "--app-secret",
            "wrong_secret",
        ])
        .output()
        .expect("verify should run");
    assert!(!output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Invalid credentials. Check App ID and App Secret"));
}

#[test]
#[ignore]
fn feishu_verify_reports_missing_bot_capability() {
    let dir = temp_dir("feishu-verify-bot-disabled");
    let home = dir.join("home");
    fs::create_dir_all(&home).expect("home should exist");
    let base_url = start_feishu_bot_disabled_server();

    let output = cli_command(&dir, &home)
        .env("CLAWDEN_FEISHU_API_BASE_URL", &base_url)
        .args([
            "channels",
            "feishu",
            "verify",
            "--app-id",
            "cli_test",
            "--app-secret",
            "secret_test",
        ])
        .output()
        .expect("verify should run");
    assert!(!output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Bot capability is not enabled"));
}

#[test]
#[ignore]
fn feishu_verify_reports_missing_config_without_flags() {
    let dir = temp_dir("feishu-verify-missing-config");
    let home = dir.join("home");
    fs::create_dir_all(&home).expect("home should exist");

    let output = cli_command(&dir, &home)
        .args(["channels", "feishu", "verify"])
        .output()
        .expect("verify should run");
    assert!(!output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("missing Feishu app_id"));
}

#[test]
#[ignore]
fn feishu_verify_succeeds_with_env_credentials_without_yaml() {
    let dir = temp_dir("feishu-verify-env-only");
    let home = dir.join("home");
    fs::create_dir_all(&home).expect("home should exist");
    let base_url = start_feishu_success_server(Some("env_test"));

    let output = cli_command(&dir, &home)
        .env("CLAWDEN_FEISHU_API_BASE_URL", &base_url)
        .env("FEISHU_APP_ID", "env_test")
        .env("FEISHU_APP_SECRET", "env_secret")
        .args(["channels", "feishu", "verify"])
        .output()
        .expect("verify should run");
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("App ID:           env_test"));
}

#[test]
#[ignore]
fn feishu_verify_prefers_flags_over_env_credentials() {
    let dir = temp_dir("feishu-verify-flags-over-env");
    let home = dir.join("home");
    fs::create_dir_all(&home).expect("home should exist");
    let base_url = start_feishu_success_server(Some("cli_override"));

    let output = cli_command(&dir, &home)
        .env("CLAWDEN_FEISHU_API_BASE_URL", &base_url)
        .env("FEISHU_APP_ID", "env_test")
        .env("FEISHU_APP_SECRET", "env_secret")
        .args([
            "channels",
            "feishu",
            "verify",
            "--app-id",
            "cli_override",
            "--app-secret",
            "override_secret",
        ])
        .output()
        .expect("verify should run");
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("App ID:           cli_override"));
}

#[test]
#[ignore]
fn feishu_verify_uses_selected_channel_credentials() {
    let dir = temp_dir("feishu-verify-channel");
    let home = dir.join("home");
    fs::create_dir_all(&home).expect("home should exist");
    let base_url = start_feishu_success_server(Some("cli_selected"));

    fs::write(
        dir.join("clawden.yaml"),
        r#"
runtime: zeroclaw
channels:
  support-feishu:
    type: feishu
    app_id: cli_selected
    app_secret: selected_secret
  backup-feishu:
    type: feishu
    app_id: cli_backup
    app_secret: backup_secret
"#,
    )
    .expect("yaml should be written");

    let output = cli_command(&dir, &home)
        .env("CLAWDEN_FEISHU_API_BASE_URL", &base_url)
        .args([
            "channels",
            "feishu",
            "verify",
            "--channel",
            "support-feishu",
        ])
        .output()
        .expect("verify should run");
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Channel:          support-feishu"));
    assert!(stdout.contains("App ID:           cli_selected"));
}

#[test]
#[ignore]
fn feishu_verify_prefers_flag_values_over_yaml() {
    let dir = temp_dir("feishu-verify-flag-override");
    let home = dir.join("home");
    fs::create_dir_all(&home).expect("home should exist");
    let base_url = start_feishu_success_server(Some("cli_override"));

    fs::write(
        dir.join("clawden.yaml"),
        r#"
runtime: zeroclaw
channels:
  feishu:
    type: feishu
    app_id: cli_yaml
    app_secret: yaml_secret
"#,
    )
    .expect("yaml should be written");

    let output = cli_command(&dir, &home)
        .env("CLAWDEN_FEISHU_API_BASE_URL", &base_url)
        .args([
            "channels",
            "feishu",
            "verify",
            "--app-id",
            "cli_override",
            "--app-secret",
            "override_secret",
        ])
        .output()
        .expect("verify should run");
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("App ID:           cli_override"));
}

#[test]
#[ignore]
fn feishu_verify_prompts_for_channel_selection_when_multiple_exist() {
    let dir = temp_dir("feishu-verify-prompt-select");
    let home = dir.join("home");
    fs::create_dir_all(&home).expect("home should exist");
    let base_url = start_feishu_success_server(Some("cli_backup"));

    fs::write(
        dir.join("clawden.yaml"),
        r#"
runtime: zeroclaw
channels:
  support-feishu:
    type: feishu
    app_id: cli_support
    app_secret: support_secret
  backup-feishu:
    type: feishu
    app_id: cli_backup
    app_secret: backup_secret
"#,
    )
    .expect("yaml should be written");

    let mut child = cli_command(&dir, &home)
        .env("CLAWDEN_FEISHU_API_BASE_URL", &base_url)
        .args(["channels", "feishu", "verify"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("verify should spawn");

    child
        .stdin
        .take()
        .expect("stdin should be available")
        .write_all(b"\n")
        .expect("stdin should accept input");

    let output = child.wait_with_output().expect("process should finish");
    assert!(
        output.status.success(),
        "verify exited with {}: stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Channel:          backup-feishu"));
    assert!(stdout.contains("App ID:           cli_backup"));
}

#[test]
#[ignore]
fn feishu_setup_prints_steps_and_verifies_credentials() {
    let dir = temp_dir("feishu-setup");
    let home = dir.join("home");
    fs::create_dir_all(&home).expect("home should exist");
    let base_url = start_feishu_success_server(Some("cli_setup"));

    let mut child = cli_command(&dir, &home)
        .env("CLAWDEN_FEISHU_API_BASE_URL", &base_url)
        .args(["channels", "feishu", "setup"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("setup should spawn");

    child
        .stdin
        .take()
        .expect("stdin should be available")
        .write_all(b"cli_setup\nsetup_secret\n")
        .expect("stdin should accept input");

    let output = child.wait_with_output().expect("process should finish");
    assert!(
        output.status.success(),
        "setup exited with {}: stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Step 1: Create a Feishu App"));
    assert!(stdout.contains("Step 6: Publish"));
    assert!(stdout.contains("Verifying credentials..."));
    assert!(stdout.contains("Your clawden.yaml channel config:"));
    assert!(stdout.contains("FEISHU_APP_ID=cli_setup"));
}
