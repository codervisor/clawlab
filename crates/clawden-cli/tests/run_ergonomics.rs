use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
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

fn setup_direct_runtime(home: &Path) {
    let runtime_dir = home
        .join(".clawden")
        .join("runtimes")
        .join("zeroclaw")
        .join("latest");
    fs::create_dir_all(&runtime_dir).expect("runtime directory should be created");

    let executable = runtime_dir.join("zeroclaw");
    fs::write(
        &executable,
        "#!/usr/bin/env sh\nprintenv > \"$CLAWDEN_ENV_DUMP_FILE\"\nsleep 3\nexit 0\n",
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
}

fn wait_for_dump(dump_path: &Path) -> String {
    for _ in 0..80 {
        if dump_path.exists() {
            let content = fs::read_to_string(dump_path).expect("env dump should be readable");
            if !content.trim().is_empty() {
                return content;
            }
        }
        thread::sleep(Duration::from_millis(25));
    }
    panic!("runtime did not write env dump in time");
}

#[test]
fn run_uses_env_file_override() {
    let dir = temp_dir("run-env-file");
    let home = dir.join("home");
    let project = dir.join("project");
    let dump_path = dir.join("runtime.env");

    fs::create_dir_all(&home).expect("home should be created");
    fs::create_dir_all(&project).expect("project should be created");
    setup_direct_runtime(&home);

    fs::write(
        project.join("clawden.yaml"),
        "runtime: zeroclaw\nprovider: openai\nproviders:\n  openai:\n    api_key: $OPENAI_API_KEY\n",
    )
    .expect("yaml should be written");
    fs::write(
        project.join("staging.env"),
        "OPENAI_API_KEY=sk-from-env-file\n",
    )
    .expect("env file should be written");

    let status = Command::new(binary_path())
        .current_dir(&project)
        .env("HOME", &home)
        .env("CLAWDEN_ENV_DUMP_FILE", &dump_path)
        .env_remove("OPENAI_API_KEY")
        .args([
            "--no-docker",
            "run",
            "--env-file",
            "staging.env",
            "zeroclaw",
        ])
        .status()
        .expect("run should execute");
    assert!(status.success());

    let env_dump = wait_for_dump(&dump_path);
    assert!(env_dump.contains("CLAWDEN_LLM_API_KEY=sk-from-env-file"));
}

#[test]
fn run_env_flag_overrides_api_key_shortcut_for_provider_key() {
    let dir = temp_dir("run-precedence");
    let home = dir.join("home");
    let project = dir.join("project");
    let dump_path = dir.join("runtime.env");

    fs::create_dir_all(&home).expect("home should be created");
    fs::create_dir_all(&project).expect("project should be created");
    setup_direct_runtime(&home);

    fs::write(project.join("clawden.yaml"), "runtime: zeroclaw\n").expect("yaml should be written");

    let status = Command::new(binary_path())
        .current_dir(&project)
        .env("HOME", &home)
        .env("CLAWDEN_ENV_DUMP_FILE", &dump_path)
        .args([
            "--no-docker",
            "run",
            "--provider",
            "openai",
            "--api-key",
            "sk-base",
            "-e",
            "OPENAI_API_KEY=sk-override",
            "zeroclaw",
        ])
        .status()
        .expect("run should execute");
    assert!(status.success());

    let env_dump = wait_for_dump(&dump_path);
    assert!(env_dump.contains("OPENAI_API_KEY=sk-override"));
    assert!(env_dump.contains("CLAWDEN_LLM_API_KEY=sk-base"));
}

#[test]
fn run_sets_port_map_env_in_direct_mode() {
    let dir = temp_dir("run-port-map");
    let home = dir.join("home");
    let project = dir.join("project");
    let dump_path = dir.join("runtime.env");

    fs::create_dir_all(&home).expect("home should be created");
    fs::create_dir_all(&project).expect("project should be created");
    setup_direct_runtime(&home);

    fs::write(project.join("clawden.yaml"), "runtime: zeroclaw\n").expect("yaml should be written");

    let status = Command::new(binary_path())
        .current_dir(&project)
        .env("HOME", &home)
        .env("CLAWDEN_ENV_DUMP_FILE", &dump_path)
        .args([
            "--no-docker",
            "run",
            "--allow-missing-credentials",
            "-p",
            "3000:42617",
            "zeroclaw",
        ])
        .status()
        .expect("run should execute");
    assert!(status.success());

    let env_dump = wait_for_dump(&dump_path);
    assert!(env_dump.contains("CLAWDEN_PORT_MAP=3000:42617"));
}
