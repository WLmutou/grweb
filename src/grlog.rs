use log::{LevelFilter, Metadata, Record, Log};
use std::fs::OpenOptions;
use std::io::Write;

use crate::config::LoggingConfig;



/// 初始化日志配置
pub fn init_logging(log_config: &LoggingConfig) {
    let global_level = parse_level(&log_config.level);

    let logger: Box<dyn Log + Send + Sync> = match log_config.output.as_str() {
        "file" => {
            if let Some(ref log_file) = log_config.file {
                if let Some(parent) = std::path::Path::new(log_file).parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                
                let file = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(log_file)
                    .expect("Failed to open log file");
                
                Box::new(FileLogger::new(file, global_level))
            } else {
                eprintln!("Warning: log output is set to 'file' but no file path specified, falling back to console");
                Box::new(ConsoleLogger::new(global_level))
            }
        }
        _ => {
            Box::new(ConsoleLogger::new(global_level))
        }
    };

    let _ = log::set_boxed_logger(logger);
    log::set_max_level(global_level);
}

fn parse_level(level: &str) -> LevelFilter {
    match level.to_lowercase().as_str() {
        "trace" => LevelFilter::Trace,
        "debug" => LevelFilter::Debug,
        "info" => LevelFilter::Info,
        "warn" => LevelFilter::Warn,
        "error" => LevelFilter::Error,
        "off" => LevelFilter::Off,
        _ => LevelFilter::Info,
    }
}

struct ConsoleLogger {
    level: LevelFilter,
}

impl ConsoleLogger {
    fn new(level: LevelFilter) -> Self {
        Self { level }
    }
}

impl Log for ConsoleLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= self.level
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            eprintln!(
                "{} [{}] {}",
                record.level(),
                record.target(),
                record.args()
            );
        }
    }

    fn flush(&self) {}
}

struct FileLogger {
    level: LevelFilter,
    file: std::sync::Mutex<std::io::BufWriter<std::fs::File>>,
}

impl FileLogger {
    fn new(file: std::fs::File, level: LevelFilter) -> Self {
        Self {
            level,
            file: std::sync::Mutex::new(std::io::BufWriter::new(file)),
        }
    }
}

impl Log for FileLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= self.level
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let mut file = self.file.lock().unwrap();
            let _ = writeln!(
                file,
                "{} [{}] {}",
                record.level(),
                record.target(),
                record.args()
            );
            let _ = file.flush();
        }
    }

    fn flush(&self) {}
}
