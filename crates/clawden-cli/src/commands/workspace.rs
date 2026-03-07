use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Result};

use crate::cli::WorkspaceCommand;

pub fn exec_workspace(command: WorkspaceCommand) -> Result<()> {
    match command {
        WorkspaceCommand::Restore {
            repo,
            token,
            target,
            branch,
        } => exec_restore(repo, token, target, branch),
        WorkspaceCommand::Sync {
            target,
            message,
            token,
        } => exec_sync(target, message, token),
        WorkspaceCommand::Status { target } => exec_status(target),
    }
}

// ---------------------------------------------------------------------------
// Restore: clone or fast-forward pull agent workspace from a git repo
// ---------------------------------------------------------------------------

fn exec_restore(
    repo: String,
    token: Option<String>,
    target: Option<String>,
    branch: String,
) -> Result<()> {
    let target_dir = resolve_target(target.as_deref())?;
    let repo_url = build_repo_url(&repo, token.as_deref())?;

    if target_dir.join(".git").is_dir() {
        println!(
            "[clawden] Pulling latest agent memory into {}",
            target_dir.display()
        );
        let output = Command::new("git")
            .args(["pull", "--ff-only", "origin", &branch])
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

        let output = Command::new("git")
            .args([
                "clone",
                "--single-branch",
                "--branch",
                &branch,
                &repo_url,
                &target_dir.to_string_lossy(),
            ])
            .env("GIT_TERMINAL_PROMPT", "0")
            .output()?;

        print_scrubbed(&output.stdout);
        print_scrubbed(&output.stderr);

        if !output.status.success() {
            eprintln!("[clawden] Warning: git clone failed, continuing without agent memory");
            return Ok(());
        }
    }

    if target_dir.join(".git").is_dir() {
        println!("[clawden] Agent memory ready at {}", target_dir.display());
        crate::util::append_audit_file("workspace.restore", "memory", "ok")?;
    } else {
        eprintln!("[clawden] Warning: agent memory bootstrap failed (continuing without memory)");
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Sync: commit and push workspace changes back to the remote
// ---------------------------------------------------------------------------

fn exec_sync(target: Option<String>, message: Option<String>, token: Option<String>) -> Result<()> {
    let target_dir = resolve_target(target.as_deref())?;

    if !target_dir.join(".git").is_dir() {
        bail!(
            "No git repository found at {}. Run `clawden workspace restore` first.",
            target_dir.display()
        );
    }

    // Inject credentials if provided (for push auth)
    if let Some(tok) = &token {
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

fn exec_status(target: Option<String>) -> Result<()> {
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
        Ok(PathBuf::from(t))
    } else {
        // Docker default: /home/clawden/workspace
        // Local: current directory
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let docker_workspace = PathBuf::from(&home).join("workspace");
        if docker_workspace.exists() {
            Ok(docker_workspace)
        } else {
            Ok(std::env::current_dir()?)
        }
    }
}

/// Build a git URL, injecting a PAT for private repos. Supports:
/// - Full URL: `https://github.com/owner/repo.git`
/// - Shorthand: `owner/repo` → `https://github.com/owner/repo.git`
fn build_repo_url(repo: &str, token: Option<&str>) -> Result<String> {
    let base_url = if repo.starts_with("https://") || repo.starts_with("git@") {
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
}
