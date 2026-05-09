pub mod router;
pub mod middleware;
pub mod context;
pub mod server;

// 重新导出常用类型
pub use router::Router;
pub use middleware::{Middleware, MiddlewareChain};
pub use context::Context;
pub use server::Server;

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
                ("Content-Type".to_string(), "text/plain".to_string()),
            ],
            body: body.into(),
        }
    }
    
    pub fn json(body: impl Into<Vec<u8>>) -> Self {
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

impl From<&str> for Response {
    fn from(s: &str) -> Self {
        Response::html(s.to_string())
    }
}

impl From<String> for Response {
    fn from(s: String) -> Self {
        Response::html(s)
    }
}

impl From<Vec<u8>> for Response {
    fn from(data: Vec<u8>) -> Self {
        Response::new(200, data)
    }
}