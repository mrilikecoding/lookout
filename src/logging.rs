use crate::error::Result;
use directories::ProjectDirs;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// Initialize tracing-to-file. Returns a guard that flushes the appender on drop.
pub fn init(debug: bool) -> Result<WorkerGuard> {
    let dirs = ProjectDirs::from("", "", "lookout")
        .ok_or_else(|| crate::error::Error::Internal("no state dir".into()))?;
    let log_dir = dirs.state_dir().unwrap_or(dirs.data_local_dir());
    std::fs::create_dir_all(log_dir)?;
    let file = tracing_appender::rolling::daily(log_dir, "lookout.log");
    let (nb, guard) = tracing_appender::non_blocking(file);

    let filter = if debug {
        EnvFilter::new("lookout=debug,info")
    } else {
        EnvFilter::new("lookout=info,warn")
    };
    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_writer(nb).with_ansi(false))
        .init();
    Ok(guard)
}
