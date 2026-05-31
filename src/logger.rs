use crate::config;
use std::fs;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{fmt, EnvFilter};

pub fn init() -> Result<WorkerGuard, String> {
    let cfg = config::active_config();
    let log_cfg = &cfg.logging;
    let log_dir = config::resolve_path(&log_cfg.directory);

    fs::create_dir_all(&log_dir)
        .map_err(|err| format!("failed creating log dir {}: {err}", log_dir.display()))?;

    let file_appender = tracing_appender::rolling::never(&log_dir, &log_cfg.file_name);
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(log_cfg.level.clone()))
        .map_err(|err| format!("invalid log filter '{}': {err}", log_cfg.level))?;

    fmt()
        .with_env_filter(filter)
        .with_ansi(false)
        .with_writer(non_blocking)
        .with_target(true)
        .with_file(true)
        .with_line_number(true)
        .try_init()
        .map_err(|err| format!("logger init failed: {err}"))?;

    tracing::info!("logging initialized");
    Ok(guard)
}
