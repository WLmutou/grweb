use serde::Serialize;
use serde_json::json;

pub mod router;
pub mod middleware;
pub mod context;
pub mod server;
pub mod config;
pub mod static_files;
pub mod websocket;
pub mod pool;
pub mod error;

pub use router::Router;
pub use middleware::{Middleware, MiddlewareChain};
pub use context::Context;
pub use server::Server;
pub use config::{AppConfig, ServerConfig, LoggingConfig, CorsConfig};
pub use websocket::{WebSocket, Message};
pub use pool::{ConnectionPool, SharedPool, PoolStats};
pub use error::{Error, Result, ErrorResponse};

/// HTTP 方法枚举
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Method {
    GET,
    POST,
    PUT,
    DELETE,
    PATCH,
    HEAD,
    OPTIONS,
}

impl Method {
    pub fn as_str(&self) -> &'static str {
        match self {
            Method::GET => "GET",
            Method::POST => "POST",
            Method::PUT => "PUT",
            Method::DELETE => "DELETE",
            Method::PATCH => "PATCH",
            Method::HEAD => "HEAD",
            Method::OPTIONS => "OPTIONS",
        }
    }
}

/// HTTP 响应
#[derive(Debug)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}



impl Response {
    pub fn new(status: u16, body: impl Into<Vec<u8>>) -> Self {
        Self {
            status,
            headers: vec![
                ("Content-Type".to_string(), "text/plain;charset=utf-8".to_string()),
            ],
            body: body.into(),
        }
    }
    
     // 接受任何可序列化的类型
    pub fn json<T: Serialize>(data: T) -> Self {
        let body = serde_json::to_vec(&data).unwrap_or_else(|_| {
            json!({"error": "Failed to serialize JSON"}).to_string().into_bytes()
        });
        
        Self {
            status: 200,
            headers: vec![
                ("Content-Type".to_string(), "application/json;charset=utf-8".to_string()),
            ],
            body,
        }
    }
    // 带状态码的 JSON 响应
    pub fn json_with_status<T: Serialize>(status: u16, data: T) -> Self {
        let body = serde_json::to_vec(&data).unwrap_or_else(|_| {
            json!({"error": "Failed to serialize JSON"}).to_string().into_bytes()
        });
        
        Self {
            status,
            headers: vec![
                ("Content-Type".to_string(), "application/json;charset=utf-8".to_string()),
            ],
            body,
        }
    }
    
    pub fn json_str(body: impl Into<Vec<u8>>) -> Self {
        let mut resp = Self::new(200, body);
        resp.headers = vec![("Content-Type".to_string(), "application/json".to_string())];
        resp
    }
    
    pub fn html(body: impl Into<Vec<u8>>) -> Self {
        let mut resp = Self::new(200, body);
        resp.headers = vec![("Content-Type".to_string(), "text/html".to_string())];
        resp
    }
    
    pub fn not_found() -> Self {
        Self::new(404, "404 Not Found")
    }
    
    pub fn internal_error() -> Self {
        Self::new(500, "500 Internal Server Error")
    }
}

impl From<grorm::Error> for Response {
    fn from(err: grorm::Error) -> Self {
        let msg = format!(r#"{{"error":"{}"}}"#, err);
        let mut resp = Response::new(500, msg);
        resp.headers = vec![("Content-Type".to_string(), "application/json".to_string())];
        resp
    }
}

impl<T: Into<Response>, E: std::fmt::Display> From<std::result::Result<T, E>> for Response {
    fn from(result: std::result::Result<T, E>) -> Self {
        match result {
            Ok(t) => t.into(),
            Err(e) => {
                let msg = format!(r#"{{"error":"{}"}}"#, e);
                let mut resp = Response::new(500, msg);
                resp.headers = vec![("Content-Type".to_string(), "application/json".to_string())];
                resp
            }
        }
    }
}