use anyhow::{anyhow, bail, Context, Result};
use semver::{Version, VersionReq};
use serde::Serialize;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::{direct_install_descriptors, runtime_descriptor, InstallSource, VersionSource};

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
        let descriptor = ensure_runtime_supported(runtime)?;
        let runtime = descriptor.slug;
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
        let executable = match &descriptor.install_source {
            InstallSource::GithubRelease {
                owner,
                repo,
                archive_ext,
            } => {
                self.install_github_release(runtime, owner, repo, archive_ext, &version, &tmp_dir)?
            }
            InstallSource::Npm { package } => {
                self.install_npm_package(runtime, package, &version, &tmp_dir)?
            }
            InstallSource::GitClone { url } => {
                self.install_git_clone(runtime, url, &version, &tmp_dir)?
            }
            InstallSource::NotAvailable => {
                bail!("runtime '{runtime}' has no direct install implementation")
            }
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
        for descriptor in direct_install_descriptors() {
            installed.push(self.install_runtime(descriptor.slug, None)?);
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
        let descriptor = ensure_runtime_supported(runtime)?;
        match &descriptor.version_source {
            VersionSource::GithubLatest { owner, repo } => Ok(normalize_version(
                &github_release_assets(owner, repo, "latest")?.tag,
            )),
            VersionSource::Npm { package } => query_latest_npm_version(package),
            VersionSource::GitHead { url } => query_git_head_branch(url),
            VersionSource::NotAvailable => {
                bail!(
                    "runtime '{}' does not support version resolution",
                    descriptor.slug
                )
            }
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
        let descriptor = ensure_runtime_supported(runtime)?;
        let _lock = InstallLock::acquire(&self.lock_path)?;
        let runtime_dir = self.runtimes_dir.join(descriptor.slug);
        if runtime_dir.exists() {
            fs::remove_dir_all(&runtime_dir)?;
        }
        self.append_audit("runtime.uninstall", descriptor.slug, "ok")?;
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

    fn install_github_release(
        &self,
        slug: &str,
        owner: &str,
        repo: &str,
        archive_ext: &str,
        version: &str,
        tmp_dir: &Path,
    ) -> Result<PathBuf> {
        let (os, arch) = host_os_arch()?;
        // Some runtimes use a non-semver release tag; fall back to "latest"
        // to hit the /releases/latest endpoint.
        let query_version = if version.starts_with(|c: char| c.is_ascii_digit()) {
            version
        } else {
            "latest"
        };
        let release = github_release_assets(owner, repo, query_version)?;
        let patterns = platform_asset_patterns(os, arch);

        // Some runtimes publish a single platform-ambiguous archive (e.g. a
        // sole `.7z`).  When no OS-specific pattern matched, verify the
        // binary format before accepting it.
        let asset = if let Some(a) = pick_asset(&release.assets, &patterns, archive_ext) {
            a
        } else if archive_ext == ".7z" && release.assets.len() == 1 {
            let only = &release.assets[0];
            if only.name.ends_with(".7z") {
                let probe = self.download_to_cache(
                    slug,
                    release.tag.trim_start_matches('v'),
                    &only.name,
                    &only.url,
                )?;
                probe_runtime_archive(slug, &probe).with_context(|| {
                    format!(
                        "{slug} release asset '{}' is not runnable on {os}-{arch}",
                        only.name
                    )
                })?;
                only
            } else {
                bail!(
                    "no {slug} release asset matched platform {os}-{arch} in {}",
                    release.tag,
                )
            }
        } else {
            bail!(
                "no {slug} release asset matched platform {os}-{arch} in {} (available: {})",
                release.tag,
                release
                    .assets
                    .iter()
                    .map(|a| a.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
            )
        };

        let archive_path = self.download_to_cache(
            slug,
            release.tag.trim_start_matches('v'),
            &asset.name,
            &asset.url,
        )?;
        self.report_progress(&format!("Extracting {slug} archive…"));

        if archive_ext == ".7z" {
            sevenz_rust::decompress_file(&archive_path, tmp_dir).with_context(|| {
                format!(
                    "failed to extract {slug} 7z archive: {}",
                    archive_path.display()
                )
            })?;
        } else {
            self.extract_tar_gz(&archive_path, tmp_dir)?;
        }

        let candidate = find_executable_by_name(tmp_dir, slug)?.ok_or_else(|| {
            anyhow!(
                "Download validation failed for {}: archive is missing expected runtime binary",
                asset.name
            )
        })?;

        let target = tmp_dir.join(slug);
        fs::rename(candidate, &target)?;
        make_executable(&target)?;
        validate_runtime_binary_exec(slug, &target)?;
        Ok(target)
    }

    fn install_npm_package(
        &self,
        slug: &str,
        package: &str,
        version: &str,
        tmp_dir: &Path,
    ) -> Result<PathBuf> {
        ensure_command_available("node", "node")?;
        ensure_command_available("npm", "npm")?;

        let install_prefix = tmp_dir.join(format!("{slug}-prefix"));
        fs::create_dir_all(&install_prefix)?;

        let package_spec = if version == "latest" {
            format!("{package}@latest")
        } else {
            format!("{package}@{}", normalize_version(version))
        };

        self.report_progress(&format!("Installing {slug} via npm…"));
        run_command(
            Command::new("npm")
                .arg("install")
                .arg("-g")
                .arg("--prefix")
                .arg(&install_prefix)
                .arg(&package_spec),
            &format!("install {slug} with npm"),
        )?;

        let runtime_root = tmp_dir.join(format!("{slug}-runtime"));
        fs::create_dir_all(&runtime_root)?;
        fs::rename(install_prefix, runtime_root.join("current"))?;

        let launcher = tmp_dir.join(slug);
        write_launcher(
            &launcher,
            slug,
            &format!("\"$SCRIPT_DIR/{slug}-runtime/current/bin/{slug}\" \"$@\""),
        )?;
        Ok(launcher)
    }

    fn install_git_clone(
        &self,
        slug: &str,
        url: &str,
        version: &str,
        tmp_dir: &Path,
    ) -> Result<PathBuf> {
        ensure_command_available("git", "git")?;
        ensure_command_available("node", "node")?;
        ensure_command_available("pnpm", "pnpm")?;

        let ref_name = if version == "latest" {
            "main".to_string()
        } else {
            normalize_version(version)
        };

        self.report_progress(&format!("Cloning {slug} repository…"));
        let repo_dir = tmp_dir.join(format!("{slug}-src"));
        run_command(
            Command::new("git")
                .arg("clone")
                .arg("--depth")
                .arg("1")
                .arg("--branch")
                .arg(&ref_name)
                .arg(url)
                .arg(&repo_dir),
            &format!("clone {slug} repository"),
        )?;

        self.report_progress(&format!("Installing {slug} dependencies…"));
        run_command(
            command_in_dir("pnpm", &repo_dir).arg("install"),
            &format!("install {slug} dependencies"),
        )?;

        // pnpm's content-addressable store may skip install scripts for
        // native addons.  Walk node_modules looking for packages with a
        // binding.gyp and no compiled .node file, then build them.
        self.report_progress("Building native modules…");
        rebuild_native_modules(&repo_dir)?;

        if !repo_dir.join("package.json").exists() {
            bail!("{slug} validation failed: expected package.json missing");
        }

        self.report_progress(&format!("Building {slug}…"));
        run_command(
            command_in_dir("pnpm", &repo_dir).arg("run").arg("build"),
            &format!("build {slug}"),
        )?;

        if !repo_dir.join("dist").join("index.js").exists() {
            bail!("{slug} build failed: dist/index.js not produced");
        }

        let launcher = tmp_dir.join(slug);
        write_launcher(
            &launcher,
            slug,
            &format!("cd \"$SCRIPT_DIR/{slug}-src\" && pnpm start -- \"$@\""),
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

/// Whether this runtime supports `--config-dir` based config injection.
pub fn runtime_supports_config_dir(runtime: &str) -> bool {
    runtime_descriptor(runtime)
        .map(|descriptor| descriptor.supports_config_dir)
        .unwrap_or(false)
}

/// Default start args for a runtime when no subcommand is provided.
/// Used by both `clawden run` (smart default) and `clawden up` (managed start).
pub fn runtime_default_start_args(runtime: &str) -> &'static [&'static str] {
    runtime_descriptor(runtime)
        .map(|descriptor| descriptor.default_start_args)
        .unwrap_or(&[])
}

/// Common runtime subcommands for hint text only. Never inject these into argv.
pub fn runtime_subcommand_hints(runtime: &str) -> &'static [(&'static str, &'static str)] {
    runtime_descriptor(runtime)
        .map(|descriptor| descriptor.subcommand_hints)
        .unwrap_or(&[])
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
    for pattern in patterns {
        if let Some(asset) = assets.iter().find(|asset| {
            asset.name.ends_with(ext)
                && asset
                    .name
                    .to_ascii_lowercase()
                    .contains(&pattern.to_ascii_lowercase())
        }) {
            return Some(asset);
        }
    }
    None
}

fn validate_runtime_artifact(runtime: &str, executable: &Path) -> Result<()> {
    let metadata = fs::metadata(executable)
        .with_context(|| format!("runtime artifact missing for {runtime}"))?;
    if metadata.len() == 0 {
        return Err(anyhow!("runtime artifact is empty for {runtime}"));
    }
    Ok(())
}

fn validate_runtime_binary_exec(runtime: &str, executable: &Path) -> Result<()> {
    let mut failures = Vec::new();
    for args in runtime_probe_candidates(runtime) {
        let output = Command::new(executable)
            .args(*args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .with_context(|| {
                format!(
                    "failed to start {runtime} probe with args `{}`",
                    args.join(" ")
                )
            })?;

        if output.status.success() {
            return Ok(());
        }

        let mut detail = String::new();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if !stdout.is_empty() {
            detail.push_str("stdout: ");
            detail.push_str(&stdout);
        }
        if !stderr.is_empty() {
            if !detail.is_empty() {
                detail.push_str(" | ");
            }
            detail.push_str("stderr: ");
            detail.push_str(&stderr);
        }
        if detail.is_empty() {
            detail.push_str("no output captured");
        }

        failures.push(format!(
            "`{}` exited with {} ({detail})",
            args.join(" "),
            output
                .status
                .code()
                .map(|code| code.to_string())
                .unwrap_or_else(|| "signal".to_string())
        ));
    }

    Err(anyhow!(
        "runtime artifact probe failed for {runtime}: {}",
        failures.join("; ")
    ))
}

fn runtime_probe_candidates(runtime: &str) -> &'static [&'static [&'static str]] {
    match runtime {
        "zeroclaw" => &[&["--version"], &["version"], &["--help"]],
        "picoclaw" => &[&["--version"], &["version"], &["--help"], &["help"]],
        "openfang" => &[&["--version"], &["version"], &["--help"]],
        _ => &[&["--version"], &["--help"]],
    }
}

fn ensure_runtime_supported(runtime: &str) -> Result<&'static crate::RuntimeDescriptor> {
    let Some(descriptor) = runtime_descriptor(runtime) else {
        return Err(anyhow!("runtime '{}' not recognized", runtime));
    };
    if descriptor.direct_install_supported {
        Ok(descriptor)
    } else {
        Err(anyhow!(
            "runtime '{}' not supported by direct installer",
            descriptor.slug
        ))
    }
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

/// Comprehensive set of platform-specific substrings used to match release
/// assets across different naming conventions (Rust target triples, shorthand).
fn platform_asset_patterns(os: &str, arch: &str) -> Vec<&'static str> {
    match (os, arch) {
        ("linux", "x86_64") => vec![
            "x86_64-unknown-linux-musl",
            "linux-musl-x86_64",
            "linux_x86_64_musl",
            "linux-x64-musl",
            "linux_amd64_musl",
            "x86_64-unknown-linux-gnu",
            "linux-x86_64",
            "linux_x64",
            "linux_amd64",
            "x64",
        ],
        ("linux", "aarch64") => vec![
            "aarch64-unknown-linux-musl",
            "linux-musl-aarch64",
            "linux_arm64_musl",
            "linux-arm64-musl",
            "aarch64-unknown-linux-gnu",
            "linux-aarch64",
            "linux-arm64",
            "linux_arm64",
            "arm64",
        ],
        ("darwin", "x86_64") => vec![
            "x86_64-apple-darwin",
            "darwin-x86_64",
            "darwin_x64",
            "macos_x64",
        ],
        ("darwin", "aarch64") => vec![
            "aarch64-apple-darwin",
            "darwin-aarch64",
            "darwin-arm64",
            "darwin_arm64",
            "macos_arm64",
        ],
        _ => vec![],
    }
}

/// Extract an archive to a temporary directory and verify that it contains a
/// runnable runtime binary for the current host platform.
fn probe_runtime_archive(runtime: &str, archive: &Path) -> Result<()> {
    let probe_dir = archive.with_extension("probe");
    if probe_dir.exists() {
        fs::remove_dir_all(&probe_dir)?;
    }
    fs::create_dir_all(&probe_dir)?;
    let result = (|| -> Result<()> {
        sevenz_rust::decompress_file(archive, &probe_dir)
            .with_context(|| format!("failed to probe 7z archive: {}", archive.display()))?;

        let Some(candidate) = find_executable_by_name(&probe_dir, runtime)? else {
            bail!("archive is missing expected runtime binary");
        };
        make_executable(&candidate)?;
        validate_runtime_binary_exec(runtime, &candidate)
    })();
    let _ = fs::remove_dir_all(&probe_dir);
    result
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

fn query_latest_npm_version(package: &str) -> Result<String> {
    ensure_command_available("npm", "npm")?;
    let output = Command::new("npm")
        .args(["view", package, "version", "--json"])
        .output()
        .with_context(|| format!("failed to query npm for {package} latest version"))?;
    if !output.status.success() {
        bail!(
            "npm view {package} version failed with status {}",
            output.status
        );
    }

    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout)
        .with_context(|| format!("npm returned invalid JSON for {package} latest version"))?;
    if let Some(version) = parsed.as_str() {
        return Ok(normalize_version(version));
    }
    bail!("npm returned unexpected latest version payload for {package}")
}

fn query_git_head_branch(url: &str) -> Result<String> {
    ensure_command_available("git", "git")?;
    let output = Command::new("git")
        .args(["ls-remote", "--symref", url, "HEAD"])
        .output()
        .with_context(|| format!("failed to query remote HEAD for {url}"))?;
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

/// Walk `node_modules/.pnpm` for packages that have a `binding.gyp` but no
/// compiled `.node` file under `build/`.  For each such package, attempt
/// `prebuild-install` (downloads prebuilt binary) with fallback to
/// `node-gyp rebuild`.  Errors are logged but not fatal — the runtime may
/// still work if the missing addon is optional.
fn rebuild_native_modules(project_dir: &Path) -> Result<()> {
    let pnpm_dir = project_dir.join("node_modules").join(".pnpm");
    if !pnpm_dir.exists() {
        return Ok(());
    }

    // Collect package dirs that contain binding.gyp (native addon marker).
    let mut native_dirs = Vec::new();
    for entry in walkdir(&pnpm_dir) {
        let gyp = entry.join("binding.gyp");
        if gyp.exists() {
            // Already compiled?
            let has_node = entry
                .join("build")
                .join("Release")
                .read_dir()
                .ok()
                .and_then(|mut rd| {
                    rd.find(|e| {
                        e.as_ref()
                            .map(|e| e.path().extension().is_some_and(|ext| ext == "node"))
                            .unwrap_or(false)
                    })
                })
                .is_some();
            if !has_node {
                native_dirs.push(entry);
            }
        }
    }

    for dir in &native_dirs {
        // Try prebuild-install first, then fall back to node-gyp.
        let ok = Command::new("npx")
            .args(["--yes", "prebuild-install"])
            .current_dir(dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !ok {
            let _ = Command::new("npx")
                .args(["--yes", "node-gyp", "rebuild", "--release"])
                .current_dir(dir)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }
    Ok(())
}

/// Recursively find directories under `root` that directly contain a file
/// named `binding.gyp`.  Returns the directories, not the file paths.
fn walkdir(root: &Path) -> Vec<PathBuf> {
    let mut result = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let rd = match fs::read_dir(&dir) {
            Ok(rd) => rd,
            Err(_) => continue,
        };
        for entry in rd.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            }
        }
        if dir.join("binding.gyp").exists() {
            result.push(dir);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::{
        pick_asset, platform_asset_patterns, runtime_subcommand_hints, runtime_supports_config_dir,
        validate_runtime_binary_exec, version_satisfies, GithubAsset,
    };
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("clawden-install-{name}-{stamp}"));
        fs::create_dir_all(&path).expect("temp dir should exist");
        path
    }

    fn write_executable(path: &std::path::Path, content: &str) {
        fs::write(path, content).expect("script should be written");
        super::make_executable(path).expect("script should be executable");
    }

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
    fn runtime_supports_config_dir_for_known_runtimes() {
        assert!(runtime_supports_config_dir("zeroclaw"));
        assert!(runtime_supports_config_dir("picoclaw"));
        assert!(runtime_supports_config_dir("openfang"));
        assert!(!runtime_supports_config_dir("openclaw"));
    }

    #[test]
    fn runtime_subcommand_hints_include_zeroclaw_repl() {
        let hints = runtime_subcommand_hints("zeroclaw");
        assert!(hints.iter().any(|(name, _)| *name == "repl"));
    }

    #[test]
    fn linux_asset_patterns_prefer_musl_before_gnu() {
        let patterns = platform_asset_patterns("linux", "x86_64");
        let musl = patterns
            .iter()
            .position(|pattern| *pattern == "x86_64-unknown-linux-musl")
            .expect("musl pattern should exist");
        let gnu = patterns
            .iter()
            .position(|pattern| *pattern == "x86_64-unknown-linux-gnu")
            .expect("gnu pattern should exist");
        assert!(musl < gnu);
    }

    #[test]
    fn pick_asset_respects_pattern_priority() {
        let assets = vec![
            GithubAsset {
                name: "zeroclaw-x86_64-unknown-linux-gnu.tar.gz".to_string(),
                url: "https://example.invalid/gnu".to_string(),
            },
            GithubAsset {
                name: "zeroclaw-x86_64-unknown-linux-musl.tar.gz".to_string(),
                url: "https://example.invalid/musl".to_string(),
            },
        ];
        let patterns = platform_asset_patterns("linux", "x86_64");
        let picked = pick_asset(&assets, &patterns, ".tar.gz").expect("asset should match");
        assert_eq!(picked.name, "zeroclaw-x86_64-unknown-linux-musl.tar.gz");
    }

    #[test]
    fn runtime_binary_probe_accepts_help_fallback() {
        let dir = temp_dir("probe-help");
        let executable = dir.join("picoclaw");
        write_executable(
            &executable,
            r#"#!/usr/bin/env sh
if [ "$1" = "--version" ]; then
  echo "unknown flag" >&2
  exit 1
fi
if [ "$1" = "--help" ]; then
  echo "usage: picoclaw"
  exit 0
fi
exit 1
"#,
        );

        validate_runtime_binary_exec("picoclaw", &executable)
            .expect("help probe should validate runtime");
        let _ = fs::remove_dir_all(dir);
    }
}
