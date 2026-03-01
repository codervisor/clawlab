use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Serialize)]
pub struct InstalledRuntime {
    pub runtime: String,
    pub version: String,
    pub executable: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
pub enum InstallOutcome {
    Installed(InstalledRuntime),
    Uninstalled { runtime: String },
}

pub struct RuntimeInstaller {
    root_dir: PathBuf,
    runtimes_dir: PathBuf,
    cache_dir: PathBuf,
    logs_dir: PathBuf,
    lock_path: PathBuf,
}

impl RuntimeInstaller {
    pub fn new() -> Result<Self> {
        let root_dir = clawden_root_dir()?;
        let runtimes_dir = root_dir.join("runtimes");
        let cache_dir = root_dir.join("cache").join("downloads");
        let logs_dir = root_dir.join("logs");
        fs::create_dir_all(&runtimes_dir)?;
        fs::create_dir_all(&cache_dir)?;
        fs::create_dir_all(&logs_dir)?;

        Ok(Self {
            root_dir: root_dir.clone(),
            runtimes_dir,
            cache_dir,
            logs_dir,
            lock_path: root_dir.join(".install.lock"),
        })
    }

    pub fn install_runtime(
        &self,
        runtime: &str,
        requested_version: Option<&str>,
    ) -> Result<InstalledRuntime> {
        ensure_runtime_supported(runtime)?;
        let _lock = InstallLock::acquire(&self.lock_path)?;

        let version = requested_version.unwrap_or("latest");
        let runtime_dir = self.runtimes_dir.join(runtime);
        let tmp_dir = runtime_dir.join(format!(".{version}.tmp"));
        let final_dir = runtime_dir.join(version);

        if tmp_dir.exists() {
            fs::remove_dir_all(&tmp_dir)?;
        }

        fs::create_dir_all(&tmp_dir)?;
        let executable = match runtime {
            "zeroclaw" => self.install_zeroclaw(version, &tmp_dir)?,
            "picoclaw" => self.install_picoclaw(version, &tmp_dir)?,
            "openclaw" => self.install_openclaw(version, &tmp_dir)?,
            "nanoclaw" => self.install_nanoclaw(version, &tmp_dir)?,
            _ => unreachable!("validated by ensure_runtime_supported"),
        };
        validate_runtime_artifact(runtime, &executable)?;

        fs::create_dir_all(&runtime_dir)?;
        if final_dir.exists() {
            fs::remove_dir_all(&final_dir)?;
        }
        fs::rename(&tmp_dir, &final_dir)?;

        let current_link = runtime_dir.join("current");
        if current_link.exists() || current_link.is_symlink() {
            let _ = fs::remove_file(&current_link);
            let _ = fs::remove_dir_all(&current_link);
        }
        std::os::unix::fs::symlink(version, &current_link)
            .with_context(|| format!("updating current symlink for {runtime}"))?;

        self.append_audit("runtime.install", runtime, "ok")?;

        Ok(InstalledRuntime {
            runtime: runtime.to_string(),
            version: version.to_string(),
            executable: final_dir.join(runtime),
        })
    }

    pub fn install_all(&self) -> Result<Vec<InstalledRuntime>> {
        let mut installed = Vec::new();
        for runtime in ["zeroclaw", "openclaw", "picoclaw", "nanoclaw"] {
            installed.push(self.install_runtime(runtime, None)?);
        }
        Ok(installed)
    }

    pub fn uninstall_runtime(&self, runtime: &str) -> Result<()> {
        ensure_runtime_supported(runtime)?;
        let _lock = InstallLock::acquire(&self.lock_path)?;
        let runtime_dir = self.runtimes_dir.join(runtime);
        if runtime_dir.exists() {
            fs::remove_dir_all(&runtime_dir)?;
        }
        self.append_audit("runtime.uninstall", runtime, "ok")?;
        Ok(())
    }

    pub fn list_installed(&self) -> Result<Vec<InstalledRuntime>> {
        let mut rows = Vec::new();
        if !self.runtimes_dir.exists() {
            return Ok(rows);
        }

        for entry in fs::read_dir(&self.runtimes_dir)? {
            let entry = entry?;
            let runtime = entry.file_name().to_string_lossy().to_string();
            let current = entry.path().join("current");
            if !current.exists() {
                continue;
            }

            let version_path = fs::read_link(&current).unwrap_or_else(|_| PathBuf::from("latest"));
            let version = version_path.to_string_lossy().to_string();
            let executable = entry.path().join(&version).join(&runtime);
            if executable.exists() {
                rows.push(InstalledRuntime {
                    runtime,
                    version,
                    executable,
                });
            }
        }

        rows.sort_by(|a, b| a.runtime.cmp(&b.runtime));
        Ok(rows)
    }

    pub fn runtime_executable(&self, runtime: &str) -> Option<PathBuf> {
        let current = self.runtimes_dir.join(runtime).join("current");
        if !current.exists() {
            return None;
        }
        let version = fs::read_link(&current).ok()?;
        let executable = self.runtimes_dir.join(runtime).join(version).join(runtime);
        executable.exists().then_some(executable)
    }

    fn install_zeroclaw(&self, version: &str, tmp_dir: &Path) -> Result<PathBuf> {
        let (os, arch) = host_os_arch()?;
        let release = github_release_assets("zeroclaw-labs", "zeroclaw", version)?;

        let mut patterns = Vec::new();
        match (os, arch) {
            ("linux", "x86_64") => {
                patterns.push("x86_64-unknown-linux-gnu");
                patterns.push("linux-x86_64");
            }
            ("linux", "aarch64") => {
                patterns.push("aarch64-unknown-linux-gnu");
                patterns.push("linux-aarch64");
                patterns.push("linux-arm64");
            }
            ("darwin", "x86_64") => {
                patterns.push("x86_64-apple-darwin");
                patterns.push("darwin-x86_64");
            }
            ("darwin", "aarch64") => {
                patterns.push("aarch64-apple-darwin");
                patterns.push("darwin-aarch64");
                patterns.push("darwin-arm64");
            }
            _ => {}
        }

        let asset = pick_asset(&release.assets, &patterns, ".tar.gz").ok_or_else(|| {
            anyhow!(
                "no zeroclaw release asset matched platform {}-{} in {}",
                os,
                arch,
                release.tag
            )
        })?;

        let archive_path = self.download_to_cache(
            "zeroclaw",
            release.tag.trim_start_matches('v'),
            &asset.name,
            &asset.url,
        )?;
        self.extract_tar_gz(&archive_path, tmp_dir)?;

        let candidate = find_executable_by_name(tmp_dir, "zeroclaw")?.ok_or_else(|| {
            anyhow!(
                "Download validation failed for {}: archive is missing expected runtime binary",
                asset.name
            )
        })?;

        let target = tmp_dir.join("zeroclaw");
        fs::rename(candidate, &target)?;
        make_executable(&target)?;
        Ok(target)
    }

    fn install_picoclaw(&self, _version: &str, tmp_dir: &Path) -> Result<PathBuf> {
        let archive_name = "picoclaw_x64.7z";
        let url =
            "https://github.com/picoclaw-labs/picoclaw/releases/download/picoclaw/picoclaw_x64.7z";
        let archive_path = self.download_to_cache("picoclaw", "latest", archive_name, url)?;

        ensure_command_available("7z", "p7zip")?;
        run_command(
            Command::new("7z")
                .arg("x")
                .arg(&archive_path)
                .arg(format!("-o{}", tmp_dir.display())),
            "extract picoclaw archive",
        )?;

        let candidate = find_executable_by_name(tmp_dir, "picoclaw")?.ok_or_else(|| {
            anyhow!(
                "Download validation failed for {archive_name}: archive is missing expected runtime binary"
            )
        })?;

        let target = tmp_dir.join("picoclaw");
        fs::rename(candidate, &target)?;
        make_executable(&target)?;
        Ok(target)
    }

    fn install_openclaw(&self, version: &str, tmp_dir: &Path) -> Result<PathBuf> {
        ensure_command_available("node", "node")?;
        ensure_command_available("npm", "npm")?;

        let install_prefix = tmp_dir.join("openclaw-prefix");
        fs::create_dir_all(&install_prefix)?;

        let package_spec = if version == "latest" {
            "openclaw@latest".to_string()
        } else {
            format!("openclaw@{}", normalize_version(version))
        };

        run_command(
            Command::new("npm")
                .arg("install")
                .arg("-g")
                .arg("--prefix")
                .arg(&install_prefix)
                .arg(&package_spec),
            "install openclaw with npm",
        )?;

        let runtime_root = tmp_dir.join("openclaw-runtime");
        fs::create_dir_all(&runtime_root)?;
        fs::rename(install_prefix, runtime_root.join("current"))?;

        let launcher = tmp_dir.join("openclaw");
        write_launcher(
            &launcher,
            "openclaw",
            "\"$SCRIPT_DIR/openclaw-runtime/current/bin/openclaw\" \"$@\"",
        )?;
        Ok(launcher)
    }

    fn install_nanoclaw(&self, version: &str, tmp_dir: &Path) -> Result<PathBuf> {
        ensure_command_available("git", "git")?;
        ensure_command_available("node", "node")?;
        ensure_command_available("pnpm", "pnpm")?;

        let ref_name = if version == "latest" {
            "main".to_string()
        } else {
            normalize_version(version)
        };

        let repo_dir = tmp_dir.join("nanoclaw-src");
        run_command(
            Command::new("git")
                .arg("clone")
                .arg("--depth")
                .arg("1")
                .arg("--branch")
                .arg(&ref_name)
                .arg("https://github.com/qwibitai/nanoclaw.git")
                .arg(&repo_dir),
            "clone nanoclaw repository",
        )?;

        run_command(
            command_in_dir("pnpm", &repo_dir)
                .arg("install")
                .arg("--prod")
                .arg("--ignore-scripts"),
            "install nanoclaw dependencies",
        )?;

        if !repo_dir.join("package.json").exists() {
            bail!("nanoclaw validation failed: expected package.json missing");
        }

        let launcher = tmp_dir.join("nanoclaw");
        write_launcher(
            &launcher,
            "nanoclaw",
            "cd \"$SCRIPT_DIR/nanoclaw-src\" && pnpm start -- \"$@\"",
        )?;
        Ok(launcher)
    }

    fn download_to_cache(
        &self,
        runtime: &str,
        version: &str,
        artifact_name: &str,
        url: &str,
    ) -> Result<PathBuf> {
        if !url.starts_with("https://") {
            bail!("refusing non-https runtime download URL: {url}");
        }

        let runtime_cache = self.cache_dir.join(runtime).join(version);
        fs::create_dir_all(&runtime_cache)?;
        let final_path = runtime_cache.join(artifact_name);
        if final_path.exists() && fs::metadata(&final_path)?.len() > 0 {
            return Ok(final_path);
        }

        let tmp_path = runtime_cache.join(format!(".{artifact_name}.tmp"));
        if tmp_path.exists() {
            fs::remove_file(&tmp_path)?;
        }

        ensure_command_available("curl", "curl")?;
        run_command(
            Command::new("curl")
                .arg("-fsSL")
                .arg(url)
                .arg("-o")
                .arg(&tmp_path),
            &format!("download runtime artifact from {url}"),
        )?;

        if !tmp_path.exists() || fs::metadata(&tmp_path)?.len() == 0 {
            bail!("downloaded artifact is empty: {artifact_name}");
        }

        fs::rename(&tmp_path, &final_path)?;
        Ok(final_path)
    }

    fn extract_tar_gz(&self, archive: &Path, output_dir: &Path) -> Result<()> {
        ensure_command_available("tar", "tar")?;
        run_command(
            Command::new("tar")
                .arg("-xzf")
                .arg(archive)
                .arg("-C")
                .arg(output_dir),
            "extract tar.gz runtime artifact",
        )
    }

    fn append_audit(&self, action: &str, runtime: &str, outcome: &str) -> Result<()> {
        let audit_path = self.logs_dir.join("audit.log");
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock before UNIX_EPOCH")
            .as_millis();
        let line = format!("{now_ms}\t{action}\t{runtime}\t{outcome}\n");
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(audit_path)?;
        file.write_all(line.as_bytes())?;
        Ok(())
    }

    pub fn root_dir(&self) -> &Path {
        &self.root_dir
    }
}

struct GithubRelease {
    tag: String,
    assets: Vec<GithubAsset>,
}

struct GithubAsset {
    name: String,
    url: String,
}

fn github_release_assets(
    owner: &str,
    repo: &str,
    requested_version: &str,
) -> Result<GithubRelease> {
    ensure_command_available("curl", "curl")?;
    let url = if requested_version == "latest" {
        format!("https://api.github.com/repos/{owner}/{repo}/releases/latest")
    } else {
        let normalized = normalize_version(requested_version);
        format!("https://api.github.com/repos/{owner}/{repo}/releases/tags/v{normalized}")
    };

    let output = Command::new("curl")
        .arg("-fsSL")
        .arg("-H")
        .arg("Accept: application/vnd.github+json")
        .arg("-H")
        .arg("User-Agent: clawden")
        .arg(&url)
        .output()
        .with_context(|| format!("failed to query GitHub release API: {url}"))?;

    if !output.status.success() {
        bail!("failed to query GitHub release API: {url}");
    }

    let value: serde_json::Value = serde_json::from_slice(&output.stdout)
        .with_context(|| format!("invalid GitHub release API response: {url}"))?;

    let tag = value
        .get("tag_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("GitHub release response missing tag_name for {owner}/{repo}"))?
        .to_string();

    let mut assets = Vec::new();
    if let Some(entries) = value.get("assets").and_then(|v| v.as_array()) {
        for entry in entries {
            let Some(name) = entry.get("name").and_then(|v| v.as_str()) else {
                continue;
            };
            let Some(url) = entry.get("browser_download_url").and_then(|v| v.as_str()) else {
                continue;
            };
            assets.push(GithubAsset {
                name: name.to_string(),
                url: url.to_string(),
            });
        }
    }

    Ok(GithubRelease { tag, assets })
}

fn pick_asset<'a>(
    assets: &'a [GithubAsset],
    patterns: &[&str],
    ext: &str,
) -> Option<&'a GithubAsset> {
    assets.iter().find(|asset| {
        asset.name.ends_with(ext)
            && patterns.iter().any(|pattern| {
                asset
                    .name
                    .to_ascii_lowercase()
                    .contains(&pattern.to_ascii_lowercase())
            })
    })
}

fn validate_runtime_artifact(runtime: &str, executable: &Path) -> Result<()> {
    let metadata = fs::metadata(executable)
        .with_context(|| format!("runtime artifact missing for {runtime}"))?;
    if metadata.len() == 0 {
        return Err(anyhow!("runtime artifact is empty for {runtime}"));
    }
    Ok(())
}

fn ensure_runtime_supported(runtime: &str) -> Result<()> {
    let allowed = ["zeroclaw", "openclaw", "picoclaw", "nanoclaw"];
    if allowed.contains(&runtime) {
        return Ok(());
    }
    Err(anyhow!(
        "runtime '{}' not supported by direct installer",
        runtime
    ))
}

fn clawden_root_dir() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME environment variable is not set")?;
    Ok(PathBuf::from(home).join(".clawden"))
}

struct InstallLock {
    path: PathBuf,
}

impl InstallLock {
    fn acquire(path: &Path) -> Result<Self> {
        if let Ok(mut file) = OpenOptions::new().create_new(true).write(true).open(path) {
            let _ = writeln!(file, "{}", std::process::id());
            return Ok(Self {
                path: path.to_path_buf(),
            });
        }

        if !is_lock_active(path) {
            let _ = fs::remove_file(path);
            if let Ok(mut file) = OpenOptions::new().create_new(true).write(true).open(path) {
                let _ = writeln!(file, "{}", std::process::id());
                return Ok(Self {
                    path: path.to_path_buf(),
                });
            }
        }

        anyhow::bail!("install already in progress (lock: {})", path.display());
    }
}

fn is_lock_active(path: &Path) -> bool {
    let Ok(body) = fs::read_to_string(path) else {
        return false;
    };

    let Ok(pid) = body.trim().parse::<u32>() else {
        return false;
    };

    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

impl Drop for InstallLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn host_os_arch() -> Result<(&'static str, &'static str)> {
    let os = match std::env::consts::OS {
        "linux" => "linux",
        "macos" => "darwin",
        other => bail!("unsupported host OS for direct install: {other}"),
    };

    let arch = match std::env::consts::ARCH {
        "x86_64" => "x86_64",
        "aarch64" => "aarch64",
        other => bail!("unsupported host architecture for direct install: {other}"),
    };

    Ok((os, arch))
}

fn normalize_version(version: &str) -> String {
    version.trim_start_matches('v').to_string()
}

fn ensure_command_available(command: &str, install_hint: &str) -> Result<()> {
    let status = Command::new("which")
        .arg(command)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    if matches!(status, Ok(code) if code.success()) {
        return Ok(());
    }

    bail!(
        "Tool '{command}' is required for direct install. Install it first (hint: {install_hint})."
    )
}

fn run_command(command: &mut Command, action: &str) -> Result<()> {
    let status = command
        .status()
        .with_context(|| format!("failed to {action}"))?;
    if !status.success() {
        bail!("command failed while trying to {action}: status {status}");
    }
    Ok(())
}

fn command_in_dir(program: &str, dir: &Path) -> Command {
    let mut command = Command::new(program);
    command.current_dir(dir);
    command
}

fn find_executable_by_name(dir: &Path, needle: &str) -> Result<Option<PathBuf>> {
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        for entry in fs::read_dir(&current)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if name.eq_ignore_ascii_case(needle) || name.starts_with(needle) {
                return Ok(Some(path));
            }
        }
    }
    Ok(None)
}

fn write_launcher(path: &Path, runtime: &str, body: &str) -> Result<()> {
    let script = format!(
        "#!/usr/bin/env sh\nSCRIPT_DIR=\"$(cd \"$(dirname \"$0\")\" && pwd)\"\n# Launcher for {runtime} installed by clawden\n{body}\n"
    );
    fs::write(path, script)?;
    make_executable(path)
}

fn make_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms)?;
    Ok(())
}
