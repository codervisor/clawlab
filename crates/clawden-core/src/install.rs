use anyhow::{anyhow, bail, Context, Result};
use semver::{Version, VersionReq};
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
    pub start_args: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub enum InstallOutcome {
    Installed(InstalledRuntime),
    Uninstalled { runtime: String },
}

#[derive(Debug, Clone, Serialize)]
pub struct VersionCheck {
    pub runtime: String,
    pub installed: String,
    pub latest: String,
    pub update_available: bool,
}

type ProgressCallback = Box<dyn Fn(&str) + Send + Sync>;

pub struct RuntimeInstaller {
    root_dir: PathBuf,
    runtimes_dir: PathBuf,
    cache_dir: PathBuf,
    logs_dir: PathBuf,
    lock_path: PathBuf,
    progress: Option<ProgressCallback>,
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
            progress: None,
        })
    }

    pub fn set_progress_callback(&mut self, cb: impl Fn(&str) + Send + Sync + 'static) {
        self.progress = Some(Box::new(cb));
    }

    fn report_progress(&self, message: &str) {
        if let Some(cb) = &self.progress {
            cb(message);
        }
    }

    pub fn install_runtime(
        &self,
        runtime: &str,
        requested_version: Option<&str>,
    ) -> Result<InstalledRuntime> {
        ensure_runtime_supported(runtime)?;
        let _lock = InstallLock::acquire(&self.lock_path)?;

        self.report_progress(&format!("Resolving {runtime} version…"));
        let version = self.resolve_requested_version(runtime, requested_version)?;
        let runtime_dir = self.runtimes_dir.join(runtime);
        let tmp_dir = runtime_dir.join(format!(".{version}.tmp"));
        let final_dir = runtime_dir.join(&version);

        if tmp_dir.exists() {
            fs::remove_dir_all(&tmp_dir)?;
        }

        fs::create_dir_all(&tmp_dir)?;
        self.report_progress(&format!("Installing {runtime}@{version}…"));
        let executable = match runtime {
            "zeroclaw" => self.install_zeroclaw(&version, &tmp_dir)?,
            "picoclaw" => self.install_picoclaw(&version, &tmp_dir)?,
            "openclaw" => self.install_openclaw(&version, &tmp_dir)?,
            "nanoclaw" => self.install_nanoclaw(&version, &tmp_dir)?,
            "openfang" => self.install_openfang(&version, &tmp_dir)?,
            _ => unreachable!("validated by ensure_runtime_supported"),
        };
        validate_runtime_artifact(runtime, &executable)?;

        self.report_progress(&format!("Finalizing {runtime}@{version}…"));
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
        std::os::unix::fs::symlink(&version, &current_link)
            .with_context(|| format!("updating current symlink for {runtime}"))?;

        self.append_audit("runtime.install", runtime, "ok")?;

        Ok(InstalledRuntime {
            runtime: runtime.to_string(),
            version: version.clone(),
            executable: final_dir.join(runtime),
            start_args: runtime_start_args(runtime),
        })
    }

    fn resolve_requested_version(
        &self,
        runtime: &str,
        requested_version: Option<&str>,
    ) -> Result<String> {
        let Some(requested) = requested_version.map(str::trim).filter(|v| !v.is_empty()) else {
            return self.query_latest_version(runtime);
        };

        if requested.eq_ignore_ascii_case("latest") {
            return self.query_latest_version(runtime);
        }

        if is_version_constraint(requested) {
            let latest = self.query_latest_version(runtime)?;
            if version_satisfies(&latest, requested) {
                return Ok(latest);
            }
            if let Some(installed) = self.installed_version(runtime)? {
                if version_satisfies(&installed, requested) {
                    return Ok(installed);
                }
            }
            bail!(
                "unable to resolve a runtime version for '{}' that satisfies constraint '{}'",
                runtime,
                requested
            );
        }

        Ok(normalize_version(requested))
    }

    pub fn install_all(&self) -> Result<Vec<InstalledRuntime>> {
        let mut installed = Vec::new();
        for runtime in ["zeroclaw", "openclaw", "picoclaw", "nanoclaw", "openfang"] {
            installed.push(self.install_runtime(runtime, None)?);
        }
        Ok(installed)
    }

    pub fn installed_version(&self, runtime: &str) -> Result<Option<String>> {
        Ok(self
            .list_installed()?
            .into_iter()
            .find(|row| row.runtime == runtime)
            .map(|row| row.version))
    }

    pub fn query_latest_version(&self, runtime: &str) -> Result<String> {
        ensure_runtime_supported(runtime)?;
        match runtime {
            "zeroclaw" => Ok(normalize_version(
                &github_release_assets("zeroclaw-labs", "zeroclaw", "latest")?.tag,
            )),
            "picoclaw" => Ok(normalize_version(
                &github_release_assets("picoclaw-labs", "picoclaw", "latest")?.tag,
            )),
            "openclaw" => query_latest_openclaw_version(),
            "nanoclaw" => query_nanoclaw_head_branch(),
            "openfang" => Ok(normalize_version(
                &github_release_assets("RightNow-AI", "openfang", "latest")?.tag,
            )),
            _ => unreachable!("validated by ensure_runtime_supported"),
        }
    }

    pub fn check_for_updates(&self) -> Result<Vec<VersionCheck>> {
        let mut checks = Vec::new();
        for installed in self.list_installed()? {
            let latest = self.query_latest_version(&installed.runtime)?;
            checks.push(VersionCheck {
                runtime: installed.runtime,
                installed: installed.version.clone(),
                update_available: update_available(&installed.version, &latest),
                latest,
            });
        }
        checks.sort_by(|a, b| a.runtime.cmp(&b.runtime));
        Ok(checks)
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
                let start_args = runtime_start_args(&runtime);
                rows.push(InstalledRuntime {
                    runtime,
                    version,
                    executable,
                    start_args,
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
        self.report_progress("Extracting zeroclaw archive…");
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

        self.report_progress("Extracting picoclaw archive…");
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

        self.report_progress("Installing openclaw via npm…");
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

        self.report_progress("Cloning nanoclaw repository…");
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

        self.report_progress("Installing nanoclaw dependencies…");
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

    fn install_openfang(&self, version: &str, tmp_dir: &Path) -> Result<PathBuf> {
        let (os, arch) = host_os_arch()?;
        let release = github_release_assets("RightNow-AI", "openfang", version)?;

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
                "no openfang release asset matched platform {}-{} in {}",
                os,
                arch,
                release.tag
            )
        })?;

        let archive_path = self.download_to_cache(
            "openfang",
            release.tag.trim_start_matches('v'),
            &asset.name,
            &asset.url,
        )?;
        self.report_progress("Extracting openfang archive…");
        self.extract_tar_gz(&archive_path, tmp_dir)?;

        let candidate = find_executable_by_name(tmp_dir, "openfang")?.ok_or_else(|| {
            anyhow!(
                "Download validation failed for {}: archive is missing expected runtime binary",
                asset.name
            )
        })?;

        let target = tmp_dir.join("openfang");
        fs::rename(candidate, &target)?;
        make_executable(&target)?;
        Ok(target)
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
        self.report_progress(&format!("Downloading {runtime} {version}…"));
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

pub fn runtime_start_args(runtime: &str) -> Vec<String> {
    match runtime {
        "zeroclaw" => vec!["daemon".to_string()],
        "picoclaw" => vec!["gateway".to_string()],
        "openfang" => vec!["daemon".to_string()],
        "nullclaw" => vec!["daemon".to_string()],
        _ => Vec::new(),
    }
}

/// Common subcommands for known runtimes, used for hint messages only.
/// Never injected into the command line.
pub fn runtime_subcommand_hints(runtime: &str) -> &'static [(&'static str, &'static str)] {
    match runtime {
        "zeroclaw" => &[
            ("daemon", "run as background daemon"),
            ("repl", "interactive REPL"),
            ("chat", "single-turn chat"),
            ("serve", "HTTP API server"),
        ],
        "picoclaw" => &[
            ("gateway", "HTTP gateway mode"),
            ("proxy", "reverse proxy mode"),
        ],
        "openfang" => &[
            ("daemon", "run as background daemon"),
            ("serve", "HTTP API server"),
        ],
        "nullclaw" => &[("daemon", "run as background daemon")],
        _ => &[],
    }
}

/// Returns the set of extra CLI flags (long-form names) that a runtime's start
/// command is known to accept.  Any flag **not** in this list must be passed via
/// environment variables instead — blindly appending flags will break runtimes
/// whose CLIs don't recognise them.
pub fn runtime_supported_extra_args(runtime: &str) -> &'static [&'static str] {
    match runtime {
        "zeroclaw" => &["--config-dir", "--port", "--host"],
        "picoclaw" => &["--config-dir", "--port", "--host"],
        "openfang" => &["--config-dir", "--port", "--host"],
        "nullclaw" => &["--config-dir", "--port", "--host"],
        _ => &[],
    }
}

/// Validate that all `args` (beyond `start_args`) are accepted by the runtime.
/// Returns the names of any unsupported flags.
pub fn validate_runtime_args<'a>(runtime: &str, args: &'a [String]) -> Vec<&'a str> {
    let supported = runtime_supported_extra_args(runtime);
    args.iter()
        .filter_map(|arg| {
            let flag = arg.split('=').next().unwrap_or(arg);
            if flag.starts_with("--") && !supported.contains(&flag) {
                Some(flag)
            } else {
                None
            }
        })
        .collect()
}

pub fn version_satisfies(installed: &str, constraint: &str) -> bool {
    let normalized_constraint = constraint.trim();
    if normalized_constraint.is_empty() || normalized_constraint.eq_ignore_ascii_case("latest") {
        return true;
    }

    if let Some(prefix) = normalized_constraint
        .strip_suffix(".x")
        .or_else(|| normalized_constraint.strip_suffix(".*"))
    {
        let mut parts = prefix.split('.').filter(|part| !part.is_empty());
        let Some(major) = parts.next().and_then(|v| v.parse::<u64>().ok()) else {
            return false;
        };
        let Some(minor) = parts.next().and_then(|v| v.parse::<u64>().ok()) else {
            return false;
        };
        if parts.next().is_some() {
            return false;
        }

        let Some(installed_version) = parse_semver(installed) else {
            return false;
        };
        return installed_version.major == major && installed_version.minor == minor;
    }

    if normalized_constraint.starts_with('>')
        || normalized_constraint.starts_with('<')
        || normalized_constraint.starts_with('=')
    {
        let Ok(req) = VersionReq::parse(normalized_constraint) else {
            return false;
        };
        let Some(installed_version) = parse_semver(installed) else {
            return false;
        };
        return req.matches(&installed_version);
    }

    normalize_version(installed) == normalize_version(normalized_constraint)
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
    let allowed = ["zeroclaw", "openclaw", "picoclaw", "nanoclaw", "openfang"];
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

fn is_version_constraint(raw: &str) -> bool {
    let value = raw.trim();
    value.ends_with(".x")
        || value.ends_with(".*")
        || value.starts_with('>')
        || value.starts_with('<')
        || value.starts_with('=')
}

fn query_latest_openclaw_version() -> Result<String> {
    ensure_command_available("npm", "npm")?;
    let output = Command::new("npm")
        .args(["view", "openclaw", "version", "--json"])
        .output()
        .context("failed to query npm for openclaw latest version")?;
    if !output.status.success() {
        bail!(
            "npm view openclaw version failed with status {}",
            output.status
        );
    }

    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout)
        .context("npm returned invalid JSON for openclaw latest version")?;
    if let Some(version) = parsed.as_str() {
        return Ok(normalize_version(version));
    }
    bail!("npm returned unexpected latest version payload for openclaw")
}

fn query_nanoclaw_head_branch() -> Result<String> {
    ensure_command_available("git", "git")?;
    let output = Command::new("git")
        .args([
            "ls-remote",
            "--symref",
            "https://github.com/qwibitai/nanoclaw.git",
            "HEAD",
        ])
        .output()
        .context("failed to query nanoclaw remote HEAD")?;
    if !output.status.success() {
        bail!(
            "git ls-remote for nanoclaw failed with status {}",
            output.status
        );
    }

    let body = String::from_utf8_lossy(&output.stdout);
    for line in body.lines() {
        if let Some(ref_name) = line.strip_prefix("ref: refs/heads/") {
            let branch = ref_name.split_whitespace().next().unwrap_or("main");
            return Ok(branch.to_string());
        }
    }
    Ok("main".to_string())
}

fn parse_semver(raw: &str) -> Option<Version> {
    Version::parse(raw.trim().trim_start_matches('v')).ok()
}

fn update_available(installed: &str, latest: &str) -> bool {
    match (parse_semver(installed), parse_semver(latest)) {
        (Some(installed_ver), Some(latest_ver)) => latest_ver > installed_ver,
        _ => normalize_version(installed) != normalize_version(latest),
    }
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
    let output = command
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("failed to {action}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = if stderr.trim().is_empty() {
            format!("exit status {}", output.status)
        } else {
            stderr.trim().to_string()
        };
        bail!("command failed while trying to {action}: {detail}");
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

#[cfg(test)]
mod tests {
    use super::{validate_runtime_args, version_satisfies};

    #[test]
    fn version_constraints_support_exact_wildcard_range_and_latest() {
        assert!(version_satisfies("0.2.1", "0.2.1"));
        assert!(version_satisfies("v0.2.5", "0.2.x"));
        assert!(version_satisfies("0.3.0", ">=0.2.1"));
        assert!(version_satisfies("main", "latest"));
        assert!(!version_satisfies("0.3.0", "0.2.x"));
        assert!(!version_satisfies("main", ">=0.2.1"));
    }

    #[test]
    fn validate_runtime_args_rejects_unknown_flags() {
        let args = vec![
            "daemon".to_string(),
            "--channels=telegram,discord".to_string(),
            "--tools=git,http".to_string(),
        ];
        let bad = validate_runtime_args("zeroclaw", &args);
        assert_eq!(bad, vec!["--channels", "--tools"]);
    }

    #[test]
    fn validate_runtime_args_allows_known_flags() {
        let args = vec![
            "daemon".to_string(),
            "--port=9090".to_string(),
            "--host=127.0.0.1".to_string(),
        ];
        let bad = validate_runtime_args("zeroclaw", &args);
        assert!(bad.is_empty(), "expected no bad flags, got: {:?}", bad);
    }
}
