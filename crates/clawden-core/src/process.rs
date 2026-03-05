use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::runtime_descriptor;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionMode {
    Docker,
    Direct,
    Auto,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub runtime: String,
    pub pid: u32,
    pub started_at_unix_ms: u64,
    pub mode: ExecutionMode,
    pub log_path: PathBuf,
    pub restart_policy: Option<String>,
    pub health_url: Option<String>,
    #[serde(default)]
    pub project_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeProcessStatus {
    pub runtime: String,
    pub pid: Option<u32>,
    pub running: bool,
    pub mode: ExecutionMode,
    pub log_path: PathBuf,
    pub health: String,
}

#[derive(Debug, Clone)]
pub struct StopOutcome {
    pub forced: bool,
}

#[derive(Debug, Clone)]
pub struct LogLine {
    pub runtime: String,
    pub timestamp_ms: u64,
    pub text: String,
}

pub struct LogStream {
    inner: Arc<Mutex<LogStreamInner>>,
    running: Arc<AtomicBool>,
}

struct LogStreamInner {
    queue: VecDeque<LogLine>,
    dropped: HashMap<String, usize>,
}

const LOG_STREAM_CAPACITY: usize = 4096;

impl LogStream {
    pub fn drain(&self) -> Vec<LogLine> {
        let mut inner = match self.inner.lock() {
            Ok(guard) => guard,
            Err(_) => return Vec::new(),
        };

        let mut out = Vec::new();
        while let Some(line) = inner.queue.pop_front() {
            out.push(line);
        }

        if inner.queue.is_empty() && !inner.dropped.is_empty() {
            let mut dropped: Vec<_> = inner.dropped.drain().collect();
            dropped.sort_by(|a, b| a.0.cmp(&b.0));
            for (runtime, count) in dropped {
                out.push(LogLine {
                    runtime,
                    timestamp_ms: now_ms(),
                    text: format!("WARNING: {count} log lines dropped (slow consumer)"),
                });
            }
        }

        out
    }
}

impl Drop for LogStream {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
    }
}

pub struct ProcessManager {
    mode: ExecutionMode,
    state_dir: PathBuf,
    log_dir: PathBuf,
}

impl ProcessManager {
    pub fn new(mode: ExecutionMode) -> Result<Self> {
        let root = clawden_root_dir()?;
        let state_dir = root.join("run");
        let log_dir = root.join("logs");
        fs::create_dir_all(&state_dir)?;
        fs::create_dir_all(&log_dir)?;
        Ok(Self {
            mode,
            state_dir,
            log_dir,
        })
    }

    pub fn state_dir(&self) -> &Path {
        &self.state_dir
    }

    pub fn log_dir(&self) -> &Path {
        &self.log_dir
    }

    pub fn docker_available() -> bool {
        Command::new("docker")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

    pub fn resolve_mode(&self, force_no_docker: bool) -> ExecutionMode {
        if force_no_docker {
            return ExecutionMode::Direct;
        }

        match self.mode {
            ExecutionMode::Auto => {
                if Self::docker_available() {
                    ExecutionMode::Docker
                } else {
                    ExecutionMode::Direct
                }
            }
            explicit => explicit,
        }
    }

    pub fn start_direct(
        &self,
        runtime: &str,
        executable: &Path,
        args: &[String],
    ) -> Result<ProcessInfo> {
        self.start_direct_with_env(runtime, executable, args, &[])
    }

    pub fn start_direct_with_env(
        &self,
        runtime: &str,
        executable: &Path,
        args: &[String],
        env_vars: &[(String, String)],
    ) -> Result<ProcessInfo> {
        self.start_direct_with_env_and_project(runtime, executable, args, env_vars, None)
    }

    pub fn start_direct_with_env_and_project(
        &self,
        runtime: &str,
        executable: &Path,
        args: &[String],
        env_vars: &[(String, String)],
        project_hash: Option<String>,
    ) -> Result<ProcessInfo> {
        if !executable.exists() {
            return Err(anyhow!(
                "runtime executable not found: {}",
                executable.display()
            ));
        }

        let log_path = self.log_dir.join(format!("{runtime}.log"));
        // Start each launch session with a fresh log file to avoid replaying stale output.
        OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&log_path)
            .with_context(|| format!("preparing runtime log file {}", log_path.display()))?;

        let (runtime_args, restart_policy) = split_restart_policy(args);

        if restart_policy.as_deref() == Some("on-failure") {
            let script_path = self.state_dir.join(format!("{runtime}.supervisor.sh"));
            let audit_path = self.log_dir.join("audit.log");
            let script = build_restart_supervisor_script();
            fs::write(&script_path, script)
                .with_context(|| format!("writing supervisor script {}", script_path.display()))?;
            #[allow(clippy::permissions_set_readonly_false)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = fs::metadata(&script_path)?.permissions();
                perms.set_mode(0o755);
                fs::set_permissions(&script_path, perms)?;
            }

            let mut command = Command::new("sh");
            command
                .arg(script_path)
                .arg(executable)
                .arg(&log_path)
                .arg(audit_path)
                .arg(runtime);
            command.args(&runtime_args);
            command.envs(env_vars.iter().map(|(k, v)| (k.as_str(), v.as_str())));
            command.stdout(Stdio::null());
            command.stderr(Stdio::null());

            let child = command
                .spawn()
                .with_context(|| format!("failed to spawn {}", executable.display()))?;
            return self.finish_start(runtime, child.id(), log_path, restart_policy, project_hash);
        }

        let stdout_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .with_context(|| format!("opening runtime log file {}", log_path.display()))?;
        let stderr_file = stdout_file.try_clone()?;

        let mut command = Command::new(executable);
        command.args(&runtime_args);
        command.envs(env_vars.iter().map(|(k, v)| (k.as_str(), v.as_str())));
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());

        let mut child = command
            .spawn()
            .with_context(|| format!("failed to spawn {}", executable.display()))?;

        if let Some(out) = child.stdout.take() {
            tee_reader_to_log(out, Arc::new(Mutex::new(stdout_file)));
        }
        if let Some(err) = child.stderr.take() {
            tee_reader_to_log(err, Arc::new(Mutex::new(stderr_file)));
        }

        self.finish_start(runtime, child.id(), log_path, restart_policy, project_hash)
    }

    pub fn stop(&self, runtime: &str) -> Result<()> {
        let _ = self.stop_with_timeout(runtime, 2)?;
        Ok(())
    }

    pub fn stop_with_timeout(&self, runtime: &str, timeout_secs: u64) -> Result<StopOutcome> {
        let Some(info) = self.read_pid_file(runtime)? else {
            return Ok(StopOutcome { forced: false });
        };

        let pid = info.pid.to_string();
        let _ = Command::new("kill").args(["-TERM", &pid]).status();
        for _ in 0..(timeout_secs.saturating_mul(10).max(1)) {
            if !is_pid_running(info.pid) {
                self.remove_pid_file(runtime)?;
                return Ok(StopOutcome { forced: false });
            }
            thread::sleep(Duration::from_millis(100));
        }

        let _ = Command::new("kill").args(["-KILL", &pid]).status();
        self.remove_pid_file(runtime)?;
        Ok(StopOutcome { forced: true })
    }

    pub fn force_kill(&self, runtime: &str) -> Result<bool> {
        let Some(info) = self.read_pid_file(runtime)? else {
            return Ok(false);
        };
        let pid = info.pid.to_string();
        let _ = Command::new("kill").args(["-KILL", &pid]).status();
        self.remove_pid_file(runtime)?;
        Ok(true)
    }

    pub fn list_processes(&self) -> Result<Vec<ProcessInfo>> {
        let mut infos = Vec::new();
        if !self.state_dir.exists() {
            return Ok(infos);
        }

        for entry in fs::read_dir(&self.state_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("pid") {
                continue;
            }

            let body = fs::read_to_string(path)?;
            let info: ProcessInfo = serde_json::from_str(&body)?;
            infos.push(info);
        }

        infos.sort_by(|a, b| a.runtime.cmp(&b.runtime));
        Ok(infos)
    }

    pub fn list_statuses(&self) -> Result<Vec<RuntimeProcessStatus>> {
        let mut statuses = Vec::new();
        if !self.state_dir.exists() {
            return Ok(statuses);
        }

        for info in self.list_processes()? {
            let running = is_pid_running(info.pid);
            let health = if !running {
                "stopped".to_string()
            } else if let Some(url) = &info.health_url {
                if health_check_ok(url) {
                    "healthy".to_string()
                } else {
                    "unhealthy".to_string()
                }
            } else {
                "unknown".to_string()
            };

            statuses.push(RuntimeProcessStatus {
                runtime: info.runtime,
                pid: Some(info.pid),
                running,
                mode: info.mode,
                log_path: info.log_path,
                health,
            });
        }

        statuses.sort_by(|a, b| a.runtime.cmp(&b.runtime));
        Ok(statuses)
    }

    pub fn tail_logs(&self, runtime: &str, lines: usize) -> Result<String> {
        let log_path = self.log_dir.join(format!("{runtime}.log"));
        if !log_path.exists() {
            return Ok(String::new());
        }
        let content = fs::read_to_string(&log_path)?;
        let rows: Vec<&str> = content.lines().collect();
        let start = rows.len().saturating_sub(lines);
        Ok(rows[start..].join("\n"))
    }

    pub fn stream_logs(&self, runtimes: &[String]) -> Result<LogStream> {
        let selected = if runtimes.is_empty() {
            self.list_statuses()?
                .into_iter()
                .map(|s| s.runtime)
                .collect::<Vec<_>>()
        } else {
            runtimes.to_vec()
        };

        let mut watched = Vec::new();
        for runtime in &selected {
            watched.push((runtime.clone(), self.log_dir.join(format!("{runtime}.log"))));
        }

        let inner = Arc::new(Mutex::new(LogStreamInner {
            queue: VecDeque::with_capacity(LOG_STREAM_CAPACITY),
            dropped: HashMap::new(),
        }));
        let running = Arc::new(AtomicBool::new(true));

        let stream_inner = Arc::clone(&inner);
        let stream_running = Arc::clone(&running);
        thread::spawn(move || {
            let mut offsets: HashMap<String, usize> = watched
                .iter()
                .map(|(runtime, path)| {
                    let offset = fs::metadata(path)
                        .map(|meta| meta.len() as usize)
                        .unwrap_or(0);
                    (runtime.clone(), offset)
                })
                .collect();
            while stream_running.load(Ordering::Relaxed) {
                let mut any_sent = false;
                for (runtime, log_path) in &watched {
                    let Ok(content) = fs::read_to_string(log_path) else {
                        continue;
                    };

                    let offset = offsets.entry(runtime.clone()).or_insert(0usize);
                    if *offset > content.len() {
                        *offset = 0;
                    }
                    if *offset == content.len() {
                        continue;
                    }

                    let chunk = &content[*offset..];
                    for line in chunk.lines() {
                        let Ok(mut state) = stream_inner.lock() else {
                            return;
                        };

                        while state.queue.len() >= LOG_STREAM_CAPACITY {
                            if let Some(dropped_line) = state.queue.pop_front() {
                                *state.dropped.entry(dropped_line.runtime).or_insert(0) += 1;
                            } else {
                                break;
                            }
                        }

                        state.queue.push_back(LogLine {
                            runtime: runtime.clone(),
                            timestamp_ms: now_ms(),
                            text: line.to_string(),
                        });
                        any_sent = true;
                    }
                    *offset = content.len();
                }

                if !any_sent {
                    thread::sleep(Duration::from_millis(200));
                }
            }
        });

        Ok(LogStream { inner, running })
    }

    fn finish_start(
        &self,
        runtime: &str,
        pid: u32,
        log_path: PathBuf,
        restart_policy: Option<String>,
        project_hash: Option<String>,
    ) -> Result<ProcessInfo> {
        let info = ProcessInfo {
            runtime: runtime.to_string(),
            pid,
            started_at_unix_ms: now_ms(),
            mode: ExecutionMode::Direct,
            log_path,
            restart_policy,
            health_url: runtime_health_url(runtime),
            project_hash,
        };

        self.write_pid_file(runtime, &info)?;
        Ok(info)
    }

    fn write_pid_file(&self, runtime: &str, info: &ProcessInfo) -> Result<()> {
        let path = self.pid_file(runtime);
        let body = serde_json::to_string_pretty(info)?;
        let mut file = File::create(&path)?;
        file.write_all(body.as_bytes())?;
        Ok(())
    }

    fn read_pid_file(&self, runtime: &str) -> Result<Option<ProcessInfo>> {
        let path = self.pid_file(runtime);
        if !path.exists() {
            return Ok(None);
        }
        let body = fs::read_to_string(path)?;
        let info: ProcessInfo = serde_json::from_str(&body)?;
        Ok(Some(info))
    }

    fn remove_pid_file(&self, runtime: &str) -> Result<()> {
        let path = self.pid_file(runtime);
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }

    fn pid_file(&self, runtime: &str) -> PathBuf {
        self.state_dir.join(format!("{runtime}.pid"))
    }
}

fn tee_reader_to_log<R: std::io::Read + Send + 'static>(reader: R, file: Arc<Mutex<File>>) {
    thread::spawn(move || {
        let mut reader = BufReader::new(reader);
        let mut line = String::new();
        loop {
            line.clear();
            let Ok(read) = reader.read_line(&mut line) else {
                return;
            };
            if read == 0 {
                return;
            }

            if let Ok(mut log_file) = file.lock() {
                let _ = log_file.write_all(line.as_bytes());
                let _ = log_file.flush();
            }
        }
    });
}

fn is_pid_running(pid: u32) -> bool {
    // Check /proc/<pid>/stat first to detect zombie processes.
    // Zombies still respond to kill -0 but are no longer truly running.
    if let Ok(stat) = fs::read_to_string(format!("/proc/{pid}/stat")) {
        // The state field is the third field: "<pid> (<comm>) <state> ..."
        // A zombie has state 'Z'.
        if let Some(state_start) = stat.rfind(") ") {
            let after = &stat[state_start + 2..];
            if after.starts_with('Z') {
                return false;
            }
        }
    }
    if let Ok(output) = Command::new("ps")
        .args(["-o", "stat=", "-p", &pid.to_string()])
        .output()
    {
        if output.status.success() {
            let status = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if status.is_empty() || status.contains('Z') {
                return false;
            }
        }
    }

    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_millis() as u64
}

fn split_restart_policy(args: &[String]) -> (Vec<String>, Option<String>) {
    let mut filtered = Vec::new();
    let mut restart_policy = None;

    for arg in args {
        if let Some(policy) = arg.strip_prefix("--restart=") {
            restart_policy = Some(policy.to_string());
            continue;
        }
        filtered.push(arg.clone());
    }

    (filtered, restart_policy)
}

fn runtime_health_url(runtime: &str) -> Option<String> {
    let runtime_key = runtime.to_ascii_uppercase().replace('-', "_");
    let url_key = format!("CLAWDEN_HEALTH_URL_{runtime_key}");
    if let Ok(url) = std::env::var(url_key) {
        if !url.trim().is_empty() {
            return Some(url);
        }
    }

    let port_key = format!("CLAWDEN_HEALTH_PORT_{runtime_key}");
    if let Ok(port) = std::env::var(port_key) {
        if !port.trim().is_empty() {
            return Some(format!("http://127.0.0.1:{port}/health"));
        }
    }

    runtime_descriptor(runtime).and_then(|descriptor| descriptor.health_url())
}

fn health_check_ok(url: &str) -> bool {
    Command::new("curl")
        .args(["-fsS", "--max-time", "2", url])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn build_restart_supervisor_script() -> &'static str {
    r#"#!/usr/bin/env sh
set -u

exec_path="$1"
log_path="$2"
audit_path="$3"
runtime_name="$4"
shift 4

backoff=1
child_pid=""

cleanup() {
    if [ -n "$child_pid" ]; then
        kill -TERM "$child_pid" 2>/dev/null || true
        sleep 2
        kill -KILL "$child_pid" 2>/dev/null || true
    fi
    exit 0
}

trap cleanup INT TERM

while true; do
    "$exec_path" "$@" >>"$log_path" 2>&1 &
    child_pid="$!"
    wait "$child_pid"
    exit_code="$?"
    child_pid=""

    if [ "$exit_code" -eq 0 ]; then
        exit 0
    fi

    ts="$(date +%s)000"
    printf "%s\truntime.crash\t%s\texit:%s\n" "$ts" "$runtime_name" "$exit_code" >>"$audit_path"
    printf "%s\truntime.restart\t%s\tbackoff:%s\n" "$ts" "$runtime_name" "$backoff" >>"$audit_path"

    sleep "$backoff"
    if [ "$backoff" -lt 30 ]; then
        backoff=$((backoff * 2))
        if [ "$backoff" -gt 30 ]; then
            backoff=30
        fi
    fi
done
"#
}

fn clawden_root_dir() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME environment variable is not set")?;
    Ok(PathBuf::from(home).join(".clawden"))
}

#[cfg(test)]
mod tests {
    use super::{ExecutionMode, ProcessManager};
    use std::fs;
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    use std::sync::{Mutex, OnceLock};
    use std::thread;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn stream_logs_skips_preexisting_content() {
        let _guard = env_lock().lock().expect("env lock poisoned");
        let original_home = std::env::var("HOME").ok();

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after UNIX_EPOCH")
            .as_nanos();
        let tmp_home = std::env::temp_dir().join(format!("clawden-process-test-{unique}"));
        fs::create_dir_all(&tmp_home).expect("failed to create temporary HOME dir");
        std::env::set_var("HOME", &tmp_home);

        let manager = ProcessManager::new(ExecutionMode::Direct).expect("process manager init");
        let runtime = "zeroclaw".to_string();
        let log_path = manager.log_dir().join("zeroclaw.log");
        fs::write(&log_path, "stale line\n").expect("seed stale log line");

        let stream = manager
            .stream_logs(std::slice::from_ref(&runtime))
            .expect("create log stream");

        thread::sleep(Duration::from_millis(250));
        assert!(
            stream.drain().is_empty(),
            "stale line should not be replayed"
        );

        fs::OpenOptions::new()
            .append(true)
            .open(&log_path)
            .expect("open log for append")
            .write_all(b"fresh line\n")
            .expect("append fresh line");

        thread::sleep(Duration::from_millis(300));
        let drained = stream.drain();
        assert!(
            drained.iter().any(|line| line.text == "fresh line"),
            "expected freshly appended line in stream"
        );
        assert!(
            drained.iter().all(|line| line.text != "stale line"),
            "stale line must never appear"
        );

        if let Some(home) = original_home {
            std::env::set_var("HOME", home);
        } else {
            std::env::remove_var("HOME");
        }
        let _ = fs::remove_dir_all(tmp_home);
    }

    #[test]
    fn start_direct_truncates_previous_log_content() {
        let _guard = env_lock().lock().expect("env lock poisoned");
        let original_home = std::env::var("HOME").ok();

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after UNIX_EPOCH")
            .as_nanos();
        let tmp_home = std::env::temp_dir().join(format!("clawden-process-test-{unique}"));
        fs::create_dir_all(&tmp_home).expect("failed to create temporary HOME dir");
        std::env::set_var("HOME", &tmp_home);

        let manager = ProcessManager::new(ExecutionMode::Direct).expect("process manager init");
        let runtime = "zeroclaw";
        let log_path = manager.log_dir().join("zeroclaw.log");
        fs::write(&log_path, "stale line\n").expect("seed stale content");

        let script = tmp_home.join("echo-runtime.sh");
        write_executable(&script, "#!/usr/bin/env sh\necho fresh line\nexit 0\n");

        let _info = manager
            .start_direct_with_env(runtime, &script, &[], &[])
            .expect("runtime should start");

        let deadline = Instant::now() + Duration::from_secs(2);
        let content = loop {
            let current = fs::read_to_string(&log_path).expect("log file should be readable");
            if current.contains("fresh line") {
                break current;
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for fresh line; current log content: {current:?}"
            );
            thread::sleep(Duration::from_millis(25));
        };
        assert!(!content.contains("stale line"));

        let _ = manager.stop_with_timeout(runtime, 1);

        if let Some(home) = original_home {
            std::env::set_var("HOME", home);
        } else {
            std::env::remove_var("HOME");
        }
        let _ = fs::remove_dir_all(tmp_home);
    }

    fn write_executable(path: &Path, body: &str) {
        fs::write(path, body).expect("script should be written");
        let mut perms = fs::metadata(path)
            .expect("metadata should be available")
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).expect("script should be executable");
    }
}
