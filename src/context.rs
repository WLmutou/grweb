use std::collections::HashMap;
use crate::Method;
use crate::pool::{SharedPool, PoolStats};

/// 请求上下文
#[derive(Clone)]
pub struct Context {
    pub method: Method,
    pub path: String,
    pub params: HashMap<String, String>,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
    pool: Option<SharedPool>,
}

impl Context {
    pub fn new(
        method: Method,
        path: String,
        params: HashMap<String, String>,
        headers: HashMap<String, String>,
        body: Vec<u8>,
    ) -> Self {
        Self {
            method,
            path,
            params,
            headers,
            body,
            pool: None,
        }
    }

    pub fn with_pool(mut self, pool: SharedPool) -> Self {
        self.pool = Some(pool);
        self
    }

    pub fn pool_stats(&self) -> Option<PoolStats> {
        self.pool.as_ref().map(|p| p.stats())
    }

    pub fn param(&self, key: &str) -> Option<&String> {
        self.params.get(key)
    }

    pub fn header(&self, key: &str) -> Option<&String> {
        self.headers.get(key)
    }

    pub fn body_string(&self) -> String {
        String::from_utf8_lossy(&self.body).to_string()
    }

    pub fn body_json<T: serde::de::DeserializeOwned>(&self) -> Result<T, serde_json::Error> {
        serde_json::from_slice(&self.body)
    }

    pub fn form_value(&self, key: &str) -> Option<String> {
        self.form_values().remove(key)
    }

    pub fn form_values(&self) -> HashMap<String, String> {
        let content_type = self
            .headers
            .get("Content-Type")
            .map(|v| v.as_str())
            .unwrap_or("");

        if content_type.starts_with("application/x-www-form-urlencoded") {
            parse_urlencoded(&self.body)
        } else if content_type.starts_with("multipart/form-data") {
            let boundary = extract_boundary(content_type);
            parse_multipart(&self.body, &boundary)
        } else {
            HashMap::new()
        }
    }
}

fn url_decode(input: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(input.len());
    let mut i = 0;
    while i < input.len() {
        match input[i] {
            b'+' => {
                result.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < input.len() => {
                if let (Some(h1), Some(h2)) = (hex_val(input[i + 1]), hex_val(input[i + 2])) {
                    result.push(h1 * 16 + h2);
                    i += 3;
                } else {
                    result.push(b'%');
                    i += 1;
                }
            }
            b => {
                result.push(b);
                i += 1;
            }
        }
    }
    result
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn parse_urlencoded(body: &[u8]) -> HashMap<String, String> {
    let mut result = HashMap::new();
    let body_str = String::from_utf8_lossy(body);

    for pair in body_str.split('&') {
        if pair.is_empty() {
            continue;
        }
        let mut parts = pair.splitn(2, '=');
        let key = parts.next().unwrap_or("");
        let value = parts.next().unwrap_or("");

        let decoded_key = String::from_utf8_lossy(&url_decode(key.as_bytes())).to_string();
        let decoded_value = String::from_utf8_lossy(&url_decode(value.as_bytes())).to_string();

        result.insert(decoded_key, decoded_value);
    }

    result
}

fn extract_boundary(content_type: &str) -> String {
    for part in content_type.split(';') {
        let trimmed = part.trim();
        if trimmed.starts_with("boundary=") {
            return format!("--{}", &trimmed[9..].trim_matches('"'));
        }
    }
    String::new()
}

fn parse_multipart(body: &[u8], boundary: &str) -> HashMap<String, String> {
    let mut result = HashMap::new();
    if boundary.is_empty() || body.is_empty() {
        return result;
    }

    let boundary_bytes = boundary.as_bytes();
    let mut pos = 0;

    while pos < body.len() {
        let part_start = match find_bytes(&body[pos..], boundary_bytes) {
            Some(p) => pos + p + boundary_bytes.len(),
            None => break,
        };

        if part_start >= body.len() {
            break;
        }

        let after_boundary = &body[part_start..];
        let part_start = if after_boundary.starts_with(b"\r\n") {
            part_start + 2
        } else if after_boundary.starts_with(b"\n") {
            part_start + 1
        } else if after_boundary.starts_with(b"--") {
            break;
        } else {
            part_start
        };

        pos = part_start;

        let header_end = match find_bytes(&body[pos..], b"\r\n\r\n") {
            Some(p) => pos + p + 4,
            None => match find_bytes(&body[pos..], b"\n\n") {
                Some(p) => pos + p + 2,
                None => break,
            },
        };

        let headers_slice = &body[pos..header_end];
        let mut field_name = String::new();

        let headers_str = String::from_utf8_lossy(headers_slice);
        for line in headers_str.lines() {
            let lower = line.to_lowercase();
            if let Some(idx) = lower.find("content-disposition") {
                let disp = &line[idx + 19..];
                for param in disp.split(';') {
                    let trimmed = param.trim();
                    if let Some(eq) = trimmed.find('=') {
                        let k = trimmed[..eq].trim();
                        let v = trimmed[eq + 1..].trim_matches('"');
                        if k == "name" {
                            field_name = v.to_string();
                        }
                    }
                }
            }
        }

        let body_start = header_end;
        let body_end = match find_bytes(&body[body_start..], boundary_bytes) {
            Some(p) => body_start + p,
            None => body.len(),
        };

        let part_body = &body[body_start..body_end];
        let part_body = if part_body.ends_with(b"\r\n") {
            &part_body[..part_body.len() - 2]
        } else if part_body.ends_with(b"\n") {
            &part_body[..part_body.len() - 1]
        } else {
            part_body
        };

        if !field_name.is_empty() {
            let value = String::from_utf8_lossy(part_body).to_string();
            result.insert(field_name, value);
        }

        pos = body_end;
    }

    result
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}