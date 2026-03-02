use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
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

fn write_executable(path: &Path, content: &str) {
    fs::write(path, content).expect("script should be written");
    let mut perms = fs::metadata(path)
        .expect("metadata should be available")
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).expect("script should be executable");
}

fn setup_fake_curl(bin_dir: &Path, latest_zeroclaw: &str) {
    let script = format!(
        r#"#!/usr/bin/env sh
set -eu
url=""
for arg in "$@"; do
  url="$arg"
done

case "$url" in
  *"/repos/zeroclaw-labs/zeroclaw/releases/latest")
    printf '%s' '{{"tag_name":"v{}","assets":[]}}'
    ;;
  *)
    echo "unexpected curl url: $url" >&2
    exit 1
    ;;
esac
"#,
        latest_zeroclaw
    );
    write_executable(&bin_dir.join("curl"), &script);
}

fn setup_installed_zeroclaw(home: &Path, version: &str) {
    let runtime_dir = home
        .join(".clawden")
        .join("runtimes")
        .join("zeroclaw")
        .join(version);
    fs::create_dir_all(&runtime_dir).expect("runtime dir should be created");

    let executable = runtime_dir.join("zeroclaw");
    fs::write(&executable, "#!/usr/bin/env sh\nexit 0\n").expect("runtime script should exist");
    let mut perms = fs::metadata(&executable)
        .expect("metadata should exist")
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&executable, perms).expect("runtime script should be executable");

    let current = home
        .join(".clawden")
        .join("runtimes")
        .join("zeroclaw")
        .join("current");
    std::os::unix::fs::symlink(version, current).expect("current symlink should be created");
}

#[test]
fn install_outdated_exits_zero_when_up_to_date() {
    let dir = temp_dir("install-outdated-ok");
    let home = dir.join("home");
    let bin_dir = dir.join("bin");

    fs::create_dir_all(&home).expect("home should be created");
    fs::create_dir_all(&bin_dir).expect("bin dir should be created");
    setup_fake_curl(&bin_dir, "0.2.1");
    setup_installed_zeroclaw(&home, "0.2.1");

    let base_path = std::env::var("PATH").unwrap_or_default();
    let path = format!("{}:{}", bin_dir.display(), base_path);

    let output = Command::new(binary_path())
        .env("HOME", &home)
        .env("PATH", path)
        .args(["install", "--outdated"])
        .output()
        .expect("install --outdated should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("RUNTIME"));
    assert!(stdout.contains("zeroclaw"));
    assert!(stdout.contains("Up to date"));
}

#[test]
fn install_outdated_exits_one_when_update_available() {
    let dir = temp_dir("install-outdated-update");
    let home = dir.join("home");
    let bin_dir = dir.join("bin");

    fs::create_dir_all(&home).expect("home should be created");
    fs::create_dir_all(&bin_dir).expect("bin dir should be created");
    setup_fake_curl(&bin_dir, "0.2.1");
    setup_installed_zeroclaw(&home, "0.1.0");

    let base_path = std::env::var("PATH").unwrap_or_default();
    let path = format!("{}:{}", bin_dir.display(), base_path);

    let output = Command::new(binary_path())
        .env("HOME", &home)
        .env("PATH", path)
        .args(["install", "--outdated"])
        .output()
        .expect("install --outdated should run");

    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("zeroclaw"));
    assert!(stdout.contains("Update available"));
}

#[test]
fn install_upgrade_without_installed_runtimes_prints_helpful_message() {
    let dir = temp_dir("install-upgrade-none");
    let home = dir.join("home");
    fs::create_dir_all(&home).expect("home should be created");

    let output = Command::new(binary_path())
        .env("HOME", &home)
        .args(["install", "--upgrade"])
        .output()
        .expect("install --upgrade should run");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("No installed runtimes to upgrade"));
}
