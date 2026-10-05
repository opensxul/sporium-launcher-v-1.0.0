use std::path::Path;
use tracing_appender::{
    non_blocking::WorkerGuard,
    rolling::{Builder, Rotation},
};
use tracing_subscriber::prelude::*;

pub fn init(directory: &Path) -> Result<WorkerGuard, Box<dyn std::error::Error>> {
    let appender = Builder::new()
        .rotation(Rotation::DAILY)
        .filename_prefix("sporium")
        .filename_suffix("log")
        .max_log_files(7)
        .build(directory)?;
    let (writer, guard) = tracing_appender::non_blocking(appender);
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(writer)
                .with_ansi(false)
                .with_target(false)
                .with_filter(tracing_subscriber::filter::LevelFilter::INFO),
        )
        .try_init()?;
    Ok(guard)
}
