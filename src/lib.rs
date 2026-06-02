use serde::Serialize;
use serde_json::json;

pub mod config;
pub mod context;
pub mod error;
pub mod middleware;
pub mod pool;
pub mod router;
pub mod server;
pub mod session;
pub mod static_files;
pub mod websocket;

pub use config::{AppConfig, CorsConfig, LoggingConfig, ServerConfig, DatabaseConfig, resolve_path, create_db_pool};
pub use context::Context;
pub use error::{Error, ErrorResponse, Result};
pub use middleware::{Middleware, MiddlewareChain};
pub use pool::{ConnectionPool, PoolStats, SharedPool};
pub use router::Router;
pub use server::Server;
pub use session::Session;
pub use websocket::{Message, WebSocket};


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
#[derive(Debug, Serialize)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    pub fn new(status: u16, body: impl Into<Vec<u8>>) -> Self {
        Self {
            status,
            headers: vec![(
                "Content-Type".to_string(),
                "text/plain;charset=utf-8".to_string(),
            )],
            body: body.into(),
        }
    }


    pub fn json<T: Serialize + std::fmt::Debug>(data: T) -> Self {
        let body = match serde_json::to_vec(&data) {
            Ok(bytes) => bytes,
            Err(e) => {
                grlog::error!("Failed to serialize JSON: {}", e);
                serde_json::to_vec(&json!({"error": "Failed to serialize JSON", "details": e.to_string()}))
                    .unwrap_or_else(|_| b"{}".to_vec())
            }
        };
        
        let resp = Self {
            status: 200,
            headers: vec![(
                "Content-Type".to_string(),
                "application/json;charset=utf-8".to_string(),
            )],
            body,
        };
        resp
    }

    pub fn json_with_status<T: Serialize + std::fmt::Debug>(status: u16, data: T) -> Self {
        grlog::debug!("Attempting to serialize data with status {}: {:?}", status, data);
        let body = match serde_json::to_vec(&data) {
            Ok(bytes) => {
                grlog::debug!("Successfully serialized data to {} bytes", bytes.len());
                bytes
            },
            Err(e) => {
                grlog::error!("Failed to serialize JSON with status {}: {}, data: {:?}", status, e, data);
                serde_json::to_vec(&json!({"error": "Failed to serialize JSON", "details": e.to_string()}))
                    .unwrap_or_else(|_| b"{}".to_vec())
            }
        };

        Self {
            status,
            headers: vec![(
                "Content-Type".to_string(),
                "application/json;charset=utf-8".to_string(),
            )],
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

    pub fn from_bytes(body: Vec<u8>) -> Self {
        Self {
            status: 200,
            headers: vec![],
            body,
        }
    }

    pub fn header(mut self, key: &str, value: &str) -> Self {
        self.headers.push((key.to_string(), value.to_string()));
        self
    }

    pub fn not_found() -> Self {
        Self::new(404, "404 Not Found")
    }

    pub fn internal_error() -> Self {
        Self::new(500, "500 Internal Server Error")
    }

    pub fn ok() -> Self {
        Self::new(200, "OK")
    }

    pub fn bad_request(message: &str) -> Self {
        Self::json_str(message)
    }

    pub fn unauthorized(message: &str) -> Self {
        Self::json_str(message)
    }

    pub fn forbidden(message: &str) -> Self {
        Self::json_str(message)
    }

    pub fn redirect(location: &str) -> Self {
        Self {
            status: 302,
            headers: vec![("Location".to_string(), location.to_string())],
            body: Vec::new(),
        }
    }

    pub fn with_status(mut self, status: u16) -> Self {
        self.status = status;
        self
    }

    pub fn content_type(mut self, content_type: &str) -> Self {
        self.headers.retain(|(k, _)| k != "Content-Type");
        self.headers
            .push(("Content-Type".to_string(), content_type.to_string()));
        self
    }

    pub fn body_len(&self) -> usize {
        self.body.len()
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
