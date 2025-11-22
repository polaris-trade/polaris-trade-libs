use crate::LoggingError;
use config_loader::{app_config::BaseAppConfig, logging::FileLoggerConfig};
use std::{
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

/// Custom rolling file writer that rotates based on size
/// Always writes to {prefix}.log
/// When size exceeds max_size, rotates to {prefix}-YYYYMMDD-{increment}.log
pub struct SizeBasedRollingWriter {
    path: std::path::PathBuf,
    prefix: String,
    max_size: u64,
    current_file: Arc<Mutex<std::fs::File>>,
    current_size: Arc<Mutex<u64>>,
}

impl SizeBasedRollingWriter {
    fn new(path: &Path, prefix: &str, max_size: u64) -> Result<Self, LoggingError> {
        let log_path = path.join(format!("{}.log", prefix));

        // Get current file size if it exists
        let current_size = if log_path.exists() {
            std::fs::metadata(&log_path).map(|m| m.len()).unwrap_or(0)
        } else {
            0
        };

        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .map_err(|e| LoggingError::BuildLayerError {
                message: anyhow::anyhow!("Failed to open log file {}: {}", log_path.display(), e)
                    .to_string(),
                context: "file_appender",
            })?;

        Ok(Self {
            path: path.to_path_buf(),
            prefix: prefix.to_string(),
            max_size,
            current_file: Arc::new(Mutex::new(file)),
            current_size: Arc::new(Mutex::new(current_size)),
        })
    }

    fn rotate(&self) -> Result<(), std::io::Error> {
        // First, flush and release the lock
        {
            let mut file = self.current_file.lock().unwrap();
            file.flush()?;
        }

        // Generate rotation filename: prefix-YYYYMMDD-increment.log
        let now = time::OffsetDateTime::now_utc();
        let date_str = format!("{:04}{:02}{:02}", now.year(), now.month() as u8, now.day());

        // Find next available increment
        let mut increment = 1;
        let rotated_path = loop {
            let rotated_name = format!("{}-{}-{}.log", self.prefix, date_str, increment);
            let path = self.path.join(&rotated_name);
            if !path.exists() {
                break path;
            }
            increment += 1;
        };

        // Rename current log to rotated name
        let current_log = self.path.join(format!("{}.log", self.prefix));
        std::fs::rename(&current_log, &rotated_path)?;

        // Open new file
        let log_path = self.path.join(format!("{}.log", self.prefix));
        let new_file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)?;

        // Update the file and size
        let mut file = self.current_file.lock().unwrap();
        let mut size = self.current_size.lock().unwrap();
        *file = new_file;
        *size = 0;

        Ok(())
    }
}

impl Write for SizeBasedRollingWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut file = self.current_file.lock().unwrap();
        let mut size = self.current_size.lock().unwrap();

        // Check if we need to rotate
        if *size >= self.max_size {
            drop(file);
            drop(size);
            self.rotate()?;
            file = self.current_file.lock().unwrap();
            size = self.current_size.lock().unwrap();
        }

        let written = file.write(buf)?;
        *size += written as u64;
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let mut file = self.current_file.lock().unwrap();
        file.flush()
    }
}

pub fn setup_file_appender(
    app_config: BaseAppConfig,
    file_logger_config: FileLoggerConfig,
) -> Result<
    (
        tracing_appender::non_blocking::NonBlocking,
        tracing_appender::non_blocking::WorkerGuard,
    ),
    LoggingError,
> {
    let path = PathBuf::from(&file_logger_config.path);
    let prefix = app_config.name;

    if !path.exists() {
        use std::fs;
        fs::create_dir_all(&path).map_err(|e| LoggingError::BuildLayerError {
            message: anyhow::anyhow!("Failed to create directory {}: {}", path.display(), e)
                .to_string(),
            context: "file_appender",
        })?;
    }

    if !path.is_dir() {
        return Err(LoggingError::BuildLayerError {
            message: anyhow::anyhow!("Path {} is not a directory", path.display()).to_string(),
            context: "file_appender",
        });
    }

    if path
        .metadata()
        .map(|m| m.permissions().readonly())
        .unwrap_or(true)
    {
        return Err(LoggingError::BuildLayerError {
            message: anyhow::anyhow!("No write permission for directory {}", path.display())
                .to_string(),
            context: "file_appender",
        });
    }

    // size-based rolling writer
    let writer = SizeBasedRollingWriter::new(&path, &prefix, file_logger_config.max_size)?;
    let (non_blocking_writer, guard) = tracing_appender::non_blocking(writer);
    Ok((non_blocking_writer, guard))
}
