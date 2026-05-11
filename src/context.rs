use std::collections::HashMap;
use crate::Method;

/// 请求上下文
#[derive(Clone)]
pub struct Context {
    pub method: Method,
    pub path: String,
    pub params: HashMap<String, String>,
    pub body: Vec<u8>,
}

impl Context {
    pub fn new(method: Method, path: String, params: HashMap<String, String>, body: Vec<u8>) -> Self {
        Self {
            method,
            path,
            params,
            body,
        }
    }
    
    pub fn param(&self, key: &str) -> Option<&String> {
        self.params.get(key)
    }
    
    pub fn body_string(&self) -> String {
        String::from_utf8_lossy(&self.body).to_string()
    }
    
    pub fn body_json<T: serde::de::DeserializeOwned>(&self) -> Result<T, serde_json::Error> {
        serde_json::from_slice(&self.body)
    }
}