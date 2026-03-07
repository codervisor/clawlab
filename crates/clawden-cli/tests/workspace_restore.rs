use std::fs;
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
    PathBuf::from(env!("CARGO_BIN_EXE_clawden"))
}

fn run_git(current_dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .current_dir(current_dir)
        .args(args)
        .output()
        .expect("git command should run");
    assert!(
        output.status.success(),
        "git {:?} failed: stdout={} stderr={}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn init_memory_repo(root: &Path) -> String {
    let remote = root.join("agent-memory.git");
    let seed = root.join("seed");

    run_git(
        root,
        &[
            "init",
            "--bare",
            "--initial-branch=main",
            remote.to_str().unwrap(),
        ],
    );
    run_git(
        root,
        &["clone", remote.to_str().unwrap(), seed.to_str().unwrap()],
    );
    run_git(&seed, &["config", "user.name", "ClawDen Test"]);
    run_git(&seed, &["config", "user.email", "clawden@example.com"]);

    fs::write(seed.join("MEMORY.md"), "memory\n").expect("memory file should be written");
    fs::create_dir_all(seed.join("memory")).expect("memory dir should be created");
    fs::write(seed.join("memory/2026-03-07.md"), "entry\n")
        .expect("dated memory file should be written");

    run_git(&seed, &["add", "."]);
    run_git(&seed, &["commit", "-m", "seed memory"]);
    run_git(&seed, &["push", "origin", "main"]);

    format!("file://{}", remote.display())
}

fn assert_symlink_points_to(path: &Path, expected_target: &Path) {
    let link_target = fs::read_link(path).expect("path should be a symlink");
    assert_eq!(link_target, expected_target);
}

#[test]
fn workspace_restore_defaults_to_clawden_workspace_and_links_openclaw() {
    let dir = temp_dir("workspace-restore-default");
    let home = dir.join("home");
    let repo_url = init_memory_repo(&dir);

    fs::create_dir_all(&home).expect("home should be created");
    fs::write(dir.join("existing.txt"), "keep current dir non-empty\n")
        .expect("marker file should be written");
    fs::write(
        dir.join("clawden.yaml"),
        format!("runtime: openclaw\nworkspace:\n  repo: \"{repo_url}\"\n"),
    )
    .expect("config should be written");

    let output = Command::new(binary_path())
        .current_dir(&dir)
        .env("HOME", &home)
        .args(["workspace", "restore"])
        .output()
        .expect("workspace restore should run");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let target = home.join(".clawden/workspace");
    assert!(target.join(".git").exists());
    assert!(target.join("MEMORY.md").exists());
    assert_symlink_points_to(&home.join(".openclaw/workspace"), &target);
}

#[test]
fn workspace_restore_prefers_config_path_over_env_and_links_zeroclaw() {
    let dir = temp_dir("workspace-restore-config-path");
    let home = dir.join("home");
    let configured_target = dir.join("configured-workspace");
    let env_target = dir.join("env-workspace");
    let repo_url = init_memory_repo(&dir);

    fs::create_dir_all(&home).expect("home should be created");
    fs::write(
        dir.join("clawden.yaml"),
        format!(
            "runtime: zeroclaw\nworkspace:\n  repo: \"{repo_url}\"\n  path: \"{}\"\n",
            configured_target.display()
        ),
    )
    .expect("config should be written");

    let output = Command::new(binary_path())
        .current_dir(&dir)
        .env("HOME", &home)
        .env("CLAWDEN_MEMORY_PATH", &env_target)
        .args(["workspace", "restore"])
        .output()
        .expect("workspace restore should run");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(configured_target.join(".git").exists());
    assert!(configured_target.join("MEMORY.md").exists());
    assert!(!env_target.join(".git").exists());
    assert_symlink_points_to(&home.join(".zeroclaw/workspace"), &configured_target);
}

#[test]
fn workspace_restore_bootstraps_nonempty_target_directory() {
    let dir = temp_dir("workspace-restore-nonempty-target");
    let home = dir.join("home");
    let target = dir.join("occupied-target");
    let repo_url = init_memory_repo(&dir);

    fs::create_dir_all(&home).expect("home should be created");
    fs::create_dir_all(&target).expect("target should be created");
    fs::write(target.join("notes.txt"), "preexisting\n").expect("marker should be written");

    let output = Command::new(binary_path())
        .current_dir(&dir)
        .env("HOME", &home)
        .args([
            "workspace",
            "restore",
            "--repo",
            &repo_url,
            "--target",
            target.to_str().unwrap(),
        ])
        .output()
        .expect("workspace restore should run");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(target.join(".git").exists());
    assert!(target.join("MEMORY.md").exists());
    assert!(target.join("notes.txt").exists());
}

#[test]
fn workspace_restore_detects_installed_runtime_without_config() {
    // When there is no clawden.yaml and no --agent flag, workspace restore
    // should detect installed runtimes by checking for their executable on
    // PATH and create symlinks for them.
    let dir = temp_dir("workspace-restore-detect-installed");
    let home = dir.join("home");
    let repo_url = init_memory_repo(&dir);

    fs::create_dir_all(&home).expect("home should be created");

    // Place a fake `openclaw` executable on PATH to simulate an installed runtime.
    let bin_dir = dir.join("bin");
    fs::create_dir_all(&bin_dir).expect("bin dir should be created");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let stub = bin_dir.join("openclaw");
        fs::write(&stub, "#!/bin/sh\n").expect("stub should be written");
        fs::set_permissions(&stub, fs::Permissions::from_mode(0o755))
            .expect("stub should be executable");
    }

    // No clawden.yaml — only use --repo flag.
    let output = Command::new(binary_path())
        .current_dir(&dir)
        .env("HOME", &home)
        .env("PATH", format!("{}:/usr/bin:/bin", bin_dir.display()))
        .args(["workspace", "restore", "--repo", &repo_url])
        .output()
        .expect("workspace restore should run");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let target = home.join(".clawden/workspace");
    assert!(target.join(".git").exists());
    assert!(target.join("MEMORY.md").exists());
    assert_symlink_points_to(&home.join(".openclaw/workspace"), &target);
}
