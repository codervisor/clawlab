use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::{bail, Result};
use clawden_core::runtime_descriptor;
use std::os::unix::fs::symlink;
use time::OffsetDateTime;

use crate::cli::WorkspaceCommand;

pub fn exec_workspace(command: WorkspaceCommand) -> Result<()> {
    match command {
        WorkspaceCommand::Restore {
            repo,
            token,
            target,
            branch,
            agent,
        } => exec_restore(repo, token, target, branch, agent),
        WorkspaceCommand::Sync {
            target,
            message,
            token,
            agent,
        } => exec_sync(target, message, token, agent),
        WorkspaceCommand::Status { target, agent } => exec_status(target, agent),
    }
}

// ---------------------------------------------------------------------------
// Restore: clone or fast-forward pull agent workspace from a git repo
// ---------------------------------------------------------------------------

fn exec_restore(
    repo: Option<String>,
    token: Option<String>,
    target: Option<String>,
    branch: Option<String>,
    agent: Option<String>,
) -> Result<()> {
    // Resolve config: CLI flags override config values
    let ws_cfg = resolve_workspace_config(agent.as_deref())?;
    let workspace_runtimes = resolve_workspace_runtimes(agent.as_deref())?;

    let repo = repo
        .or_else(|| ws_cfg.as_ref().map(|w| w.repo.clone()))
        .or_else(|| {
            std::env::var("CLAWDEN_MEMORY_REPO")
                .ok()
                .filter(|v| !v.is_empty())
        });
    let Some(repo) = repo else {
        bail!(
            "No workspace repo specified. Use --repo, configure workspace.repo in clawden.yaml, \
             or set CLAWDEN_MEMORY_REPO."
        );
    };

    let token = token
        .or_else(|| ws_cfg.as_ref().and_then(|w| w.token.clone()))
        .or_else(|| {
            std::env::var("CLAWDEN_MEMORY_TOKEN")
                .ok()
                .filter(|v| !v.is_empty())
        });
    let branch = branch
        .or_else(|| ws_cfg.as_ref().and_then(|w| w.branch.clone()))
        .or_else(|| {
            std::env::var("CLAWDEN_MEMORY_BRANCH")
                .ok()
                .filter(|v| !v.is_empty())
        })
        .unwrap_or_else(|| "main".to_string());
    let target = resolve_workspace_target_override(target, ws_cfg.as_ref());

    do_restore(
        &repo,
        token.as_deref(),
        target.as_deref(),
        &branch,
        &workspace_runtimes,
    )
}

/// Core restore implementation, reusable by auto-sync and entrypoint delegation.
pub(crate) fn do_restore(
    repo: &str,
    token: Option<&str>,
    target: Option<&str>,
    branch: &str,
    runtimes: &[String],
) -> Result<()> {
    let target_dir = resolve_target(target)?;
    let repo_url = build_repo_url(repo, token)?;

    if target_dir.join(".git").is_dir() {
        println!(
            "[clawden] Pulling latest agent memory into {}",
            target_dir.display()
        );
        let output = Command::new("git")
            .args(["pull", "--ff-only", "origin", branch])
            .current_dir(&target_dir)
            .env("GIT_TERMINAL_PROMPT", "0")
            .output()?;

        print_scrubbed(&output.stdout);
        print_scrubbed(&output.stderr);

        if !output.status.success() {
            eprintln!(
                "[clawden] Warning: git pull failed (exit {}), continuing with existing workspace",
                output.status.code().unwrap_or(-1)
            );
            return Ok(());
        }
    } else {
        println!(
            "[clawden] Cloning agent memory into {}",
            target_dir.display()
        );
        std::fs::create_dir_all(&target_dir)?;

        // If the directory is non-empty (e.g. user ran restore from an existing
        // workspace without --target), git clone will fail. Fall back to
        // init + fetch + checkout so the memory repo lands cleanly.
        let dir_non_empty = std::fs::read_dir(&target_dir)
            .map(|mut rd| rd.next().is_some())
            .unwrap_or(false);

        let clone_ok = if dir_non_empty {
            init_and_fetch(&target_dir, &repo_url, branch)?
        } else {
            let output = Command::new("git")
                .args([
                    "clone",
                    "--single-branch",
                    "--branch",
                    branch,
                    &repo_url,
                    &target_dir.to_string_lossy(),
                ])
                .env("GIT_TERMINAL_PROMPT", "0")
                .output()?;

            print_scrubbed(&output.stdout);
            print_scrubbed(&output.stderr);
            output.status.success()
        };

        if !clone_ok {
            eprintln!("[clawden] Warning: git clone failed, continuing without agent memory");
            return Ok(());
        }
    }

    if target_dir.join(".git").is_dir() {
        println!("[clawden] Agent memory ready at {}", target_dir.display());
        bridge_runtime_workspaces(&target_dir, runtimes)?;
        crate::util::append_audit_file("workspace.restore", "memory", "ok")?;
    } else {
        eprintln!("[clawden] Warning: agent memory bootstrap failed (continuing without memory)");
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Sync: commit and push workspace changes back to the remote
// ---------------------------------------------------------------------------

fn exec_sync(
    target: Option<String>,
    message: Option<String>,
    token: Option<String>,
    agent: Option<String>,
) -> Result<()> {
    let ws_cfg = resolve_workspace_config(agent.as_deref())?;
    let token = token.or_else(|| ws_cfg.as_ref().and_then(|w| w.token.clone()));
    let target = resolve_workspace_target_override(target, ws_cfg.as_ref());
    do_sync(target.as_deref(), message, token.as_deref())
}

/// Core sync implementation, reusable by auto-sync background task.
pub(crate) fn do_sync(
    target: Option<&str>,
    message: Option<String>,
    token: Option<&str>,
) -> Result<()> {
    let target_dir = resolve_target(target)?;

    if !target_dir.join(".git").is_dir() {
        bail!(
            "No git repository found at {}. Run `clawden workspace restore` first.",
            target_dir.display()
        );
    }

    // Inject credentials if provided (for push auth)
    if let Some(tok) = token {
        let remote_url = get_remote_url(&target_dir)?;
        let authed_url = inject_token_into_url(&remote_url, tok)?;
        run_git(&target_dir, &["remote", "set-url", "origin", &authed_url])?;
    }

    // Check for changes
    let status = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(&target_dir)
        .output()?;

    let status_text = String::from_utf8_lossy(&status.stdout);
    if status_text.trim().is_empty() {
        println!("[clawden] Workspace is clean, nothing to sync");
        return Ok(());
    }

    let commit_msg =
        message.unwrap_or_else(|| format!("clawden workspace sync {}", chrono_free_timestamp()));

    run_git(&target_dir, &["add", "-A"])?;
    run_git(&target_dir, &["commit", "-m", &commit_msg])?;
    run_git_scrubbed(&target_dir, &["push", "origin", "HEAD"])?;

    println!("[clawden] Workspace synced successfully");
    crate::util::append_audit_file("workspace.sync", "memory", "ok")?;

    // Restore original remote URL (strip token) if we injected one
    if token.is_some() {
        if let Ok(url) = get_remote_url(&target_dir) {
            let cleaned = strip_token_from_url(&url);
            let _ = run_git(&target_dir, &["remote", "set-url", "origin", &cleaned]);
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Status: show workspace git state
// ---------------------------------------------------------------------------

fn exec_status(target: Option<String>, agent: Option<String>) -> Result<()> {
    let ws_cfg = resolve_workspace_config(agent.as_deref())?;
    let target = resolve_workspace_target_override(target, ws_cfg.as_ref());

    // If agent specified, show config info
    if let Some(ws) = &ws_cfg {
        println!(
            "Config:    repo={} branch={} sync_interval={}s auto_restore={}",
            ws.repo,
            ws.branch_or_default(),
            ws.sync_interval_secs(),
            ws.auto_restore_enabled(),
        );
        if let Some(path) = &ws.path {
            println!("Path:      {}", path);
        }
    }

    let target_dir = resolve_target(target.as_deref())?;

    if !target_dir.join(".git").is_dir() {
        println!("No workspace repository at {}", target_dir.display());
        return Ok(());
    }

    // Remote URL (scrubbed)
    let remote = get_remote_url(&target_dir).unwrap_or_else(|_| "(none)".to_string());
    println!("Remote:    {}", strip_token_from_url(&remote));

    // Current branch
    let branch_out = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(&target_dir)
        .output()?;
    let branch = String::from_utf8_lossy(&branch_out.stdout)
        .trim()
        .to_string();
    println!("Branch:    {}", branch);

    // Last commit
    let log_out = Command::new("git")
        .args(["log", "-1", "--format=%h %s (%ar)"])
        .current_dir(&target_dir)
        .output()?;
    let last_commit = String::from_utf8_lossy(&log_out.stdout).trim().to_string();
    println!(
        "Last sync: {}",
        if last_commit.is_empty() {
            "(no commits)"
        } else {
            &last_commit
        }
    );

    // Working tree status
    let status = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(&target_dir)
        .output()?;
    let status_text = String::from_utf8_lossy(&status.stdout);
    let changed = status_text.lines().count();
    if changed == 0 {
        println!("Status:    clean");
    } else {
        println!("Status:    {} changed file(s)", changed);
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn resolve_target(target: Option<&str>) -> Result<PathBuf> {
    if let Some(t) = target {
        absolute_path(Path::new(t))
    } else {
        Ok(default_workspace_target(&home_dir()))
    }
}

fn resolve_workspace_target_override(
    cli_target: Option<String>,
    ws_cfg: Option<&clawden_config::WorkspaceYaml>,
) -> Option<String> {
    preferred_workspace_target(
        cli_target,
        ws_cfg.and_then(|workspace| workspace.path.clone()),
        std::env::var("CLAWDEN_MEMORY_PATH")
            .ok()
            .filter(|value| !value.is_empty()),
    )
}

fn preferred_workspace_target(
    cli_target: Option<String>,
    config_target: Option<String>,
    env_target: Option<String>,
) -> Option<String> {
    cli_target.or(config_target).or(env_target)
}

fn home_dir() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".to_string()))
}

fn default_workspace_target(home: &Path) -> PathBuf {
    let docker_workspace = docker_workspace_dir(home);
    if docker_workspace.exists() {
        docker_workspace
    } else {
        home.join(".clawden").join("workspace")
    }
}

fn docker_workspace_dir(home: &Path) -> PathBuf {
    home.join("workspace")
}

fn absolute_path(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

fn bridge_runtime_workspaces(target_dir: &Path, runtimes: &[String]) -> Result<()> {
    if runtimes.is_empty() {
        return Ok(());
    }

    if should_skip_runtime_workspace_bridging(&home_dir()) {
        println!("[clawden] Docker workspace detected; skipping runtime workspace symlinks");
        return Ok(());
    }

    let mut linked = std::collections::HashSet::new();

    for runtime in runtimes {
        let Some(descriptor) = runtime_descriptor(runtime) else {
            continue;
        };
        let Some(workspace_path) = descriptor.workspace_path else {
            continue;
        };
        if !linked.insert(descriptor.slug) {
            continue;
        }

        let runtime_workspace = expand_home_path(workspace_path)?;
        match ensure_workspace_symlink(&runtime_workspace, target_dir)? {
            WorkspaceSymlinkAction::Created => {
                println!(
                    "[clawden] Linked {} workspace {} -> {}",
                    descriptor.slug,
                    runtime_workspace.display(),
                    target_dir.display()
                );
            }
            WorkspaceSymlinkAction::Updated => {
                println!(
                    "[clawden] Updated {} workspace {} -> {}",
                    descriptor.slug,
                    runtime_workspace.display(),
                    target_dir.display()
                );
            }
            WorkspaceSymlinkAction::Unchanged => {}
            WorkspaceSymlinkAction::BackedUp(backup_path) => {
                println!(
                    "[clawden] Backed up existing {} workspace to {}",
                    descriptor.slug,
                    backup_path.display()
                );
                println!(
                    "[clawden] Linked {} workspace {} -> {}",
                    descriptor.slug,
                    runtime_workspace.display(),
                    target_dir.display()
                );
            }
        }
    }

    Ok(())
}

fn should_skip_runtime_workspace_bridging(home: &Path) -> bool {
    docker_workspace_dir(home).exists()
}

#[derive(Debug, PartialEq, Eq)]
enum WorkspaceSymlinkAction {
    Created,
    Updated,
    Unchanged,
    BackedUp(PathBuf),
}

fn ensure_workspace_symlink(link_path: &Path, target_dir: &Path) -> Result<WorkspaceSymlinkAction> {
    let target_dir = absolute_path(target_dir)?;

    if let Some(parent) = link_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    match std::fs::symlink_metadata(link_path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            let existing_target = symlink_target_path(link_path)?;
            if existing_target == target_dir {
                return Ok(WorkspaceSymlinkAction::Unchanged);
            }
            std::fs::remove_file(link_path)?;
            symlink(&target_dir, link_path)?;
            Ok(WorkspaceSymlinkAction::Updated)
        }
        Ok(_) => {
            let backup_path = next_backup_path(link_path)?;
            std::fs::rename(link_path, &backup_path)?;
            symlink(&target_dir, link_path)?;
            Ok(WorkspaceSymlinkAction::BackedUp(backup_path))
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            symlink(&target_dir, link_path)?;
            Ok(WorkspaceSymlinkAction::Created)
        }
        Err(err) => Err(err.into()),
    }
}

fn symlink_target_path(link_path: &Path) -> Result<PathBuf> {
    let target = std::fs::read_link(link_path)?;
    if target.is_absolute() {
        Ok(target)
    } else {
        absolute_path(
            &link_path
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join(target),
        )
    }
}

fn next_backup_path(path: &Path) -> Result<PathBuf> {
    let stamp = backup_date_stamp();
    let path_str = path.to_string_lossy();
    let mut candidate = PathBuf::from(format!("{path_str}.bak.{stamp}"));
    let mut suffix = 1;

    while candidate.exists() {
        candidate = PathBuf::from(format!("{path_str}.bak.{stamp}.{suffix}"));
        suffix += 1;
    }

    Ok(candidate)
}

fn backup_date_stamp() -> String {
    let now = OffsetDateTime::now_utc();
    format!("{:04}{:02}{:02}", now.year(), now.month() as u8, now.day())
}

fn expand_home_path(path: &str) -> Result<PathBuf> {
    if let Some(suffix) = path.strip_prefix("~/") {
        Ok(home_dir().join(suffix))
    } else {
        absolute_path(Path::new(path))
    }
}

/// Build a git URL, injecting a PAT for private repos. Supports:
/// - Full URL: `https://github.com/owner/repo.git`
/// - Full URL: `file:///tmp/repo.git`
/// - Shorthand: `owner/repo` → `https://github.com/owner/repo.git`
fn build_repo_url(repo: &str, token: Option<&str>) -> Result<String> {
    let base_url = if repo.starts_with("https://")
        || repo.starts_with("git@")
        || repo.starts_with("file://")
    {
        repo.to_string()
    } else {
        // Shorthand: owner/repo
        if !repo.contains('/') {
            bail!("Invalid repo format '{repo}'. Use 'owner/repo' or a full URL.");
        }
        format!("https://github.com/{}.git", repo.trim_end_matches(".git"))
    };

    match token {
        Some(tok) if base_url.starts_with("https://") => Ok(inject_token_into_url(&base_url, tok)?),
        _ => Ok(base_url),
    }
}

fn inject_token_into_url(url: &str, token: &str) -> Result<String> {
    if let Some(rest) = url.strip_prefix("https://") {
        // Strip existing credentials if present
        let host_and_path = if let Some((_creds, after)) = rest.split_once('@') {
            after
        } else {
            rest
        };
        Ok(format!(
            "https://x-access-token:{}@{}",
            token, host_and_path
        ))
    } else {
        bail!("Token injection only supported for HTTPS URLs, got: {url}");
    }
}

fn strip_token_from_url(url: &str) -> String {
    if let Some(rest) = url.strip_prefix("https://") {
        if let Some((_creds, after)) = rest.split_once('@') {
            return format!("https://{after}");
        }
    }
    url.to_string()
}

fn get_remote_url(dir: &Path) -> Result<String> {
    let output = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(dir)
        .output()?;
    if !output.status.success() {
        bail!("Failed to get git remote URL");
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn run_git(dir: &Path, args: &[&str]) -> Result<()> {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "git {} failed: {}",
            args.join(" "),
            scrub_credentials(&stderr)
        );
    }
    Ok(())
}

fn run_git_scrubbed(dir: &Path, args: &[&str]) -> Result<()> {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()?;

    print_scrubbed(&output.stdout);
    print_scrubbed(&output.stderr);

    if !output.status.success() {
        bail!("git {} failed", args.join(" "));
    }
    Ok(())
}

/// Bootstrap a git repo into a non-empty directory via init + fetch + checkout.
/// Returns `true` on success.
fn init_and_fetch(dir: &Path, repo_url: &str, branch: &str) -> Result<bool> {
    let run = |args: &[&str]| -> Result<bool> {
        let output = Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_TERMINAL_PROMPT", "0")
            .output()?;
        print_scrubbed(&output.stdout);
        print_scrubbed(&output.stderr);
        Ok(output.status.success())
    };

    if !run(&["init"])? {
        return Ok(false);
    }
    if !run(&["remote", "add", "origin", repo_url])? {
        // Remote may already exist from a previous partial attempt
        let _ = run(&["remote", "set-url", "origin", repo_url]);
    }
    if !run(&["fetch", "--depth=1", "origin", branch])? {
        return Ok(false);
    }
    if !run(&["checkout", &format!("origin/{branch}"), "-B", branch])? {
        return Ok(false);
    }
    Ok(true)
}

/// Print output with any credentials scrubbed
fn print_scrubbed(data: &[u8]) {
    let text = String::from_utf8_lossy(data);
    for line in text.lines() {
        if !line.is_empty() {
            println!("{}", scrub_credentials(line));
        }
    }
}

/// Remove tokens/credentials from git output
fn scrub_credentials(text: &str) -> String {
    // Scrub x-access-token:TOKEN@ patterns
    let re_token = regex_lite::Regex::new(r"x-access-token:[^@]+@").unwrap();
    let scrubbed = re_token.replace_all(text, "x-access-token:***@");
    // Also scrub any raw token-like strings after https://
    scrubbed.to_string()
}

/// Simple timestamp without pulling in chrono
fn chrono_free_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{secs}")
}

// ---------------------------------------------------------------------------
// Config-aware workspace resolution (Phase 6)
// ---------------------------------------------------------------------------

/// Load workspace config from clawden.yaml for a specific agent/runtime,
/// or the top-level workspace config if no agent is specified.
fn resolve_workspace_config(agent: Option<&str>) -> Result<Option<clawden_config::WorkspaceYaml>> {
    let Some(cfg) = super::up::load_config()? else {
        return Ok(None);
    };

    if let Some(agent_name) = agent {
        // Multi-runtime: look up workspace config for the specific runtime
        for rt in &cfg.runtimes {
            if rt.name == agent_name {
                return Ok(rt.workspace.clone());
            }
        }
        // Check if it matches the single-runtime shorthand
        if cfg.runtime.as_deref() == Some(agent_name) {
            return Ok(cfg.workspace.clone());
        }
        bail!(
            "No runtime '{}' found in clawden.yaml. Available: {}",
            agent_name,
            available_runtimes(&cfg)
        );
    }

    // No agent specified: use top-level workspace config, or first runtime's workspace
    if cfg.workspace.is_some() {
        return Ok(cfg.workspace.clone());
    }
    // Fall back to the first runtime with workspace config
    for rt in &cfg.runtimes {
        if rt.workspace.is_some() {
            return Ok(rt.workspace.clone());
        }
    }
    Ok(None)
}

fn resolve_workspace_runtimes(agent: Option<&str>) -> Result<Vec<String>> {
    let Some(cfg) = super::up::load_config()? else {
        return Ok(agent
            .and_then(|runtime| runtime_descriptor(runtime).map(|descriptor| descriptor.slug))
            .map(|runtime| vec![runtime.to_string()])
            .unwrap_or_default());
    };

    if let Some(agent_name) = agent {
        if cfg
            .runtimes
            .iter()
            .any(|runtime| runtime.name == agent_name)
            || cfg.runtime.as_deref() == Some(agent_name)
        {
            return Ok(vec![agent_name.to_string()]);
        }
        if let Some(descriptor) = runtime_descriptor(agent_name) {
            return Ok(vec![descriptor.slug.to_string()]);
        }
        bail!(
            "No runtime '{}' found in clawden.yaml. Available: {}",
            agent_name,
            available_runtimes(&cfg)
        );
    }

    if let Some(runtime) = &cfg.runtime {
        return Ok(vec![runtime.clone()]);
    }

    Ok(cfg
        .runtimes
        .iter()
        .map(|runtime| runtime.name.clone())
        .collect())
}

fn available_runtimes(cfg: &clawden_config::ClawDenYaml) -> String {
    if let Some(rt) = &cfg.runtime {
        return rt.clone();
    }
    cfg.runtimes
        .iter()
        .map(|r| r.name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

// ---------------------------------------------------------------------------
// Auto-sync background task (Phase 4)
// ---------------------------------------------------------------------------

/// Workspace sync task configuration, derived from clawden.yaml or env vars.
pub(crate) struct WorkspaceSyncTask {
    pub target: Option<String>,
    pub token: Option<String>,
    pub interval_secs: u64,
}

/// Collect workspace sync tasks from config for all runtimes being started.
/// Returns a list of sync tasks (one per unique workspace target).
pub(crate) fn collect_sync_tasks(
    config: Option<&clawden_config::ClawDenYaml>,
    target_runtimes: &[String],
) -> Vec<WorkspaceSyncTask> {
    let mut tasks = Vec::new();
    let mut seen_targets = std::collections::HashSet::new();

    if let Some(cfg) = config {
        // Single-runtime mode
        if let Some(ws) = &cfg.workspace {
            let target = ws.path.clone();
            let key = target.clone().unwrap_or_default();
            if seen_targets.insert(key) {
                tasks.push(WorkspaceSyncTask {
                    target,
                    token: ws.token.clone(),
                    interval_secs: ws.sync_interval_secs(),
                });
            }
        }

        // Multi-runtime mode: collect per-runtime workspace configs
        for rt in &cfg.runtimes {
            if !target_runtimes.contains(&rt.name) {
                continue;
            }
            if let Some(ws) = &rt.workspace {
                let target = ws.path.clone();
                let key = target.clone().unwrap_or_default();
                if seen_targets.insert(key) {
                    tasks.push(WorkspaceSyncTask {
                        target,
                        token: ws.token.clone(),
                        interval_secs: ws.sync_interval_secs(),
                    });
                }
            }
        }
    }

    // Also check env-based workspace (Docker entrypoint path)
    if tasks.is_empty() {
        if let Ok(repo) = std::env::var("CLAWDEN_MEMORY_REPO") {
            if !repo.is_empty() {
                let target = std::env::var("CLAWDEN_MEMORY_PATH")
                    .ok()
                    .filter(|v| !v.is_empty());
                let token = std::env::var("CLAWDEN_MEMORY_TOKEN")
                    .ok()
                    .filter(|v| !v.is_empty());
                let key = target.clone().unwrap_or_default();
                if seen_targets.insert(key) {
                    tasks.push(WorkspaceSyncTask {
                        target,
                        token,
                        interval_secs: 1800, // default 30m
                    });
                }
            }
        }
    }

    tasks
}

/// Spawn a background auto-sync loop. Returns a shutdown flag that can be set
/// to stop the loop. The task runs `do_sync` at each interval.
pub(crate) fn spawn_auto_sync(
    tasks: Vec<WorkspaceSyncTask>,
    shutdown: Arc<AtomicBool>,
) -> Vec<std::thread::JoinHandle<()>> {
    tasks
        .into_iter()
        .map(|task| {
            let shutdown = shutdown.clone();
            std::thread::spawn(move || {
                auto_sync_loop(&task, &shutdown);
            })
        })
        .collect()
}

fn auto_sync_loop(task: &WorkspaceSyncTask, shutdown: &AtomicBool) {
    let interval = std::time::Duration::from_secs(task.interval_secs);
    let mut elapsed = std::time::Duration::ZERO;
    let tick = std::time::Duration::from_secs(1);

    while !shutdown.load(Ordering::Relaxed) {
        std::thread::sleep(tick);
        elapsed += tick;

        if elapsed >= interval {
            elapsed = std::time::Duration::ZERO;
            match do_sync(task.target.as_deref(), None, task.token.as_deref()) {
                Ok(()) => {}
                Err(e) => {
                    eprintln!("[clawden] Auto-sync warning: {e}");
                }
            }
        }
    }

    // Final sync on shutdown
    if let Err(e) = do_sync(task.target.as_deref(), None, task.token.as_deref()) {
        eprintln!("[clawden] Final sync warning: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_repo_url_shorthand() {
        let url = build_repo_url("codervisor/agent-memory", None).unwrap();
        assert_eq!(url, "https://github.com/codervisor/agent-memory.git");
    }

    #[test]
    fn build_repo_url_shorthand_with_token() {
        let url = build_repo_url("codervisor/agent-memory", Some("ghp_test123")).unwrap();
        assert_eq!(
            url,
            "https://x-access-token:ghp_test123@github.com/codervisor/agent-memory.git"
        );
    }

    #[test]
    fn build_repo_url_full_https() {
        let url = build_repo_url("https://github.com/codervisor/agent-memory.git", None).unwrap();
        assert_eq!(url, "https://github.com/codervisor/agent-memory.git");
    }

    #[test]
    fn build_repo_url_file_scheme() {
        let url = build_repo_url("file:///tmp/agent-memory.git", None).unwrap();
        assert_eq!(url, "file:///tmp/agent-memory.git");
    }

    #[test]
    fn build_repo_url_full_https_with_token() {
        let url = build_repo_url(
            "https://github.com/codervisor/agent-memory.git",
            Some("ghp_abc"),
        )
        .unwrap();
        assert_eq!(
            url,
            "https://x-access-token:ghp_abc@github.com/codervisor/agent-memory.git"
        );
    }

    #[test]
    fn build_repo_url_rejects_invalid() {
        assert!(build_repo_url("just-a-name", None).is_err());
    }

    #[test]
    fn strip_token_round_trip() {
        let url = "https://x-access-token:ghp_secret@github.com/codervisor/agent-memory.git";
        assert_eq!(
            strip_token_from_url(url),
            "https://github.com/codervisor/agent-memory.git"
        );
    }

    #[test]
    fn strip_token_noop_for_clean_url() {
        let url = "https://github.com/codervisor/agent-memory.git";
        assert_eq!(strip_token_from_url(url), url);
    }

    #[test]
    fn scrub_credentials_removes_token() {
        let input = "fatal: could not read from remote 'https://x-access-token:ghp_secret123@github.com/owner/repo.git'";
        let scrubbed = scrub_credentials(input);
        assert!(!scrubbed.contains("ghp_secret123"));
        assert!(scrubbed.contains("x-access-token:***@"));
    }

    #[test]
    fn inject_token_replaces_existing_creds() {
        let url = "https://old-user:old-pass@github.com/owner/repo.git";
        let result = inject_token_into_url(url, "new_token").unwrap();
        assert_eq!(
            result,
            "https://x-access-token:new_token@github.com/owner/repo.git"
        );
        assert!(!result.contains("old-user"));
    }

    #[test]
    fn preferred_workspace_target_honors_precedence() {
        assert_eq!(
            preferred_workspace_target(
                Some("/cli".to_string()),
                Some("/config".to_string()),
                Some("/env".to_string())
            ),
            Some("/cli".to_string())
        );
        assert_eq!(
            preferred_workspace_target(None, Some("/config".to_string()), Some("/env".to_string())),
            Some("/config".to_string())
        );
        assert_eq!(
            preferred_workspace_target(None, None, Some("/env".to_string())),
            Some("/env".to_string())
        );
    }

    #[test]
    fn default_workspace_target_prefers_local_clawden_dir() {
        let root = std::env::temp_dir().join(format!(
            "clawden-workspace-default-{}",
            chrono_free_timestamp()
        ));
        std::fs::create_dir_all(&root).unwrap();

        let target = default_workspace_target(&root);
        assert_eq!(target, root.join(".clawden").join("workspace"));

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn ensure_workspace_symlink_creates_and_updates_links() {
        let root = std::env::temp_dir().join(format!(
            "clawden-workspace-link-{}",
            chrono_free_timestamp()
        ));
        let target_a = root.join("target-a");
        let target_b = root.join("target-b");
        let link = root.join("runtime").join("workspace");

        std::fs::create_dir_all(&target_a).unwrap();
        std::fs::create_dir_all(&target_b).unwrap();

        let created = ensure_workspace_symlink(&link, &target_a).unwrap();
        assert_eq!(created, WorkspaceSymlinkAction::Created);
        assert_eq!(symlink_target_path(&link).unwrap(), target_a);

        let unchanged = ensure_workspace_symlink(&link, &target_a).unwrap();
        assert_eq!(unchanged, WorkspaceSymlinkAction::Unchanged);

        let updated = ensure_workspace_symlink(&link, &target_b).unwrap();
        assert_eq!(updated, WorkspaceSymlinkAction::Updated);
        assert_eq!(symlink_target_path(&link).unwrap(), target_b);

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn ensure_workspace_symlink_backs_up_existing_directory() {
        let root = std::env::temp_dir().join(format!(
            "clawden-workspace-backup-{}",
            chrono_free_timestamp()
        ));
        let target = root.join("target");
        let link = root.join("runtime").join("workspace");

        std::fs::create_dir_all(&target).unwrap();
        std::fs::create_dir_all(&link).unwrap();
        std::fs::write(link.join("MEMORY.md"), "memory").unwrap();

        let action = ensure_workspace_symlink(&link, &target).unwrap();
        let WorkspaceSymlinkAction::BackedUp(backup_path) = action else {
            panic!("expected backup action");
        };

        assert!(backup_path.exists());
        assert!(backup_path.join("MEMORY.md").exists());
        assert_eq!(symlink_target_path(&link).unwrap(), target);

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn should_skip_runtime_workspace_bridging_when_docker_workspace_exists() {
        let root = std::env::temp_dir().join(format!(
            "clawden-workspace-docker-{}",
            chrono_free_timestamp()
        ));
        std::fs::create_dir_all(root.join("workspace")).unwrap();

        assert!(should_skip_runtime_workspace_bridging(&root));

        let _ = std::fs::remove_dir_all(root);
    }
}
