pub fn current_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_millis() as u64
}

pub fn runtime_env_prefix(runtime: &str) -> String {
    runtime.to_ascii_uppercase().replace('-', "_")
}
