use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::env;
use std::sync::Arc;
use grlog::warn;
use grorm::{ConnectionConfig, ConnectionPool, SqliteDriverFactory, MysqlDriverFactory, PostgresDriverFactory};
use crate::server::init_logger;

#[derive(Debug, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
    #[serde(default)]
    pub cors: CorsConfig,
}

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_worker_pool_size")]
    pub worker_pool_size: usize,
    #[serde(default = "default_read_buffer_size")]
    pub read_buffer_size: usize,
    #[serde(default = "default_tcp_nodelay")]
    pub tcp_nodelay: bool,
    #[serde(default = "default_keep_alive_timeout")]
    pub keep_alive_timeout: u64,
    #[serde(default = "default_static_dir")]
    pub static_dir: String,
    #[serde(default = "default_max_connections")]
    pub max_connections: usize,
    #[serde(default = "default_connection_timeout")]
    pub connection_timeout: u64,
}

#[derive(Debug, Deserialize)]
pub struct DatabaseConfig {
    #[serde(default = "default_db_type")]
    pub db_type: String,
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_username")]
    pub username: String,
    #[serde(default = "default_password")]
    pub password: String,
    #[serde(default = "default_database")]
    pub database: String,
    #[serde(default = "default_max_size")]
    pub max_size: usize,
}

#[derive(Debug, Deserialize)]
pub struct LoggingConfig {
    #[serde(default = "default_log_level")]
    pub level: String,
    #[serde(default = "default_log_output")]
    pub output: String,
    #[serde(default)]
    pub file: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CorsConfig {
    #[serde(default = "default_allowed_origins")]
    pub allowed_origins: Vec<String>,
    #[serde(default = "default_allowed_methods")]
    pub allowed_methods: Vec<String>,
    #[serde(default = "default_allowed_headers")]
    pub allowed_headers: Vec<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            database: DatabaseConfig::default(),
            logging: LoggingConfig::default(),
            cors: CorsConfig::default(),
        }
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            worker_pool_size: default_worker_pool_size(),
            read_buffer_size: default_read_buffer_size(),
            tcp_nodelay: default_tcp_nodelay(),
            keep_alive_timeout: default_keep_alive_timeout(),
            static_dir: default_static_dir(),
            max_connections: default_max_connections(),
            connection_timeout: default_connection_timeout(),
        }
    }
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            db_type: default_db_type(),
            host: default_host(),
            port: default_port(),
            username: default_username(),
            password: default_password(),
            database: default_database(),
            max_size: default_max_size(),
        }
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            output: default_log_output(),
            file: None,
        }
    }
}

impl Default for CorsConfig {
    fn default() -> Self {
        Self {
            allowed_origins: default_allowed_origins(),
            allowed_methods: default_allowed_methods(),
            allowed_headers: default_allowed_headers(),
        }
    }
}

impl ServerConfig {
    pub fn addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

impl AppConfig {
    pub fn load(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let config_path = Path::new(path);
        let config = if config_path.exists() {
            let content = fs::read_to_string(config_path)?;
            toml::from_str(&content)?
        } else {
            warn!("Config file '{}' not found, using defaults", path);
            AppConfig::default()
        };
        
        // 自动初始化日志
        init_logger(&config.logging);
        
        Ok(config)
    }
}

fn default_db_type() -> String {
    "sqlite".to_string()
}
fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    9030
}

fn default_worker_pool_size() -> usize {
    num_cpus::get()
}

fn default_read_buffer_size() -> usize {
    8192
}

fn default_tcp_nodelay() -> bool {
    true
}

fn default_keep_alive_timeout() -> u64 {
    5
}

fn default_static_dir() -> String {
    "public".to_string()
}

fn default_max_connections() -> usize {
    0
}

fn default_connection_timeout() -> u64 {
    30
}

fn default_username() -> String {
    "root".to_string()
}
fn default_password() -> String {
    "".to_string()
}

fn default_database() -> String {
    "grweb_db".to_string()
}

fn default_max_size() -> usize {
    100
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_log_output() -> String {
    "console".to_string()
}

fn default_allowed_origins() -> Vec<String> {
    vec!["*".to_string()]
}

fn default_allowed_methods() -> Vec<String> {
    vec![
        "GET".to_string(),
        "POST".to_string(),
        "PUT".to_string(),
        "DELETE".to_string(),
        "OPTIONS".to_string(),
    ]
}

fn default_allowed_headers() -> Vec<String> {
    vec!["Content-Type".to_string()]
}

/// 解析路径：支持相对于可执行文件所在目录的路径
/// 
/// # 参数
/// - `path`: 要解析的路径（可以是绝对路径或相对路径）
/// 
/// # 返回
/// - 如果是绝对路径，直接返回
/// - 如果是相对路径，优先尝试相对于可执行文件所在目录解析
/// - 如果相对于可执行文件目录不存在，则返回原始相对路径
/// 
/// # 示例
/// ```rust
/// use grweb::resolve_path;
/// 
/// // 绝对路径直接返回
/// let resolved = resolve_path("/opt/app/static");
/// 
/// // 相对路径会尝试相对于可执行文件目录解析
/// let resolved = resolve_path("templates/dist/assets");
/// ```
pub fn resolve_path(path: &str) -> String {
    let p = PathBuf::from(path);
    if p.is_absolute() {
        return path.to_string();
    }
    
    // 尝试获取可执行文件所在目录
    if let Ok(exe_path) = env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let resolved = exe_dir.join(path);
            if resolved.exists() {
                return resolved.to_string_lossy().to_string();
            }
        }
    }
    
    // 回退到原始相对路径
    path.to_string()
}

/// 根据配置创建数据库连接池
/// 
/// # 参数
/// - `config`: 数据库配置
/// 
/// # 返回
/// - 数据库连接池的 Arc 包装
/// 
/// # 示例
/// ```rust
/// use grweb::{AppConfig, create_db_pool};
/// 
/// let config = AppConfig::load("config.toml").expect("Failed to load config");
/// let pool = create_db_pool(&config.database);
/// ```
pub fn create_db_pool(config: &DatabaseConfig) -> Arc<ConnectionPool> {
    if config.db_type == "sqlite" {
        let dbconfig = ConnectionConfig::sqlite(&config.database);
        return Arc::new(ConnectionPool::new(
            SqliteDriverFactory,
            dbconfig,
            config.max_size,
        ));
    } else if config.db_type == "mysql" {
        let dbconfig = ConnectionConfig::mysql(
            &config.host,
            config.port,
            &config.database,
            &config.username,
            &config.password,
        );
        return Arc::new(ConnectionPool::new(
            MysqlDriverFactory,
            dbconfig,
            config.max_size,
        ));
    } else {
        let dbconfig = ConnectionConfig::postgres(
            &config.host,
            config.port,
            &config.database,
            &config.username,
            &config.password,
        );
        return Arc::new(ConnectionPool::new(
            PostgresDriverFactory,
            dbconfig,
            config.max_size,
        ))
    }
}
