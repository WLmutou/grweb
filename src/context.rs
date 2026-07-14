use crate::error::Error;
use crate::pool::{PoolStats, SharedPool};
use crate::session::{self, Session};
use crate::Method;
use grorm::ConnectionPool;
use std::any::Any;
use std::collections::HashMap;
use std::sync::Arc;

/// 请求上下文
#[derive(Clone)]
pub struct Context {
    pub method: Method,
    pub path: String,
    pub headers: HashMap<String, String>,
    store: HashMap<String, Arc<dyn Any + Send + Sync>>,
    pub body: Vec<u8>,
    query: HashMap<String, Vec<String>>,
    pool: Option<SharedPool>,
    /// 数据库连接池
    pub db_pool: Arc<ConnectionPool>,
}

impl Context {
    pub fn new(
        method: Method,
        path: String,
        headers: HashMap<String, String>,
        body: Vec<u8>,
        query: HashMap<String, Vec<String>>,
    ) -> Self {
        Self {
            method,
            path,
            headers,
            body,
            query,
            store: HashMap::new(),
            pool: None,
            db_pool: Arc::new(ConnectionPool::default()),
        }
    }

    pub fn with_pool(mut self, pool: SharedPool) -> Self {
        self.pool = Some(pool);
        self
    }

    pub fn with_db_pool(mut self, db_pool: Arc<ConnectionPool>) -> Self {
        self.db_pool = db_pool;
        self
    }

    pub fn get_db_pool(&self) -> Arc<ConnectionPool> {
        self.db_pool.clone()
    }

    pub fn get_db(&self) -> std::result::Result<grorm::pool::PoolConnection, Error> {
        self.db_pool.get().map_err(|e| {
            Error::database_with_source("Failed to get database connection".to_string(), e)
        })
    }

    pub fn pool_stats(&self) -> Option<PoolStats> {
        self.pool.as_ref().map(|p| p.stats())
    }

    /// 存入任意类型的数据（T 必须是 Send + Sync + 'static）
    pub fn insert<T: Send + Sync + 'static>(&mut self, key: &str, value: T) {
        self.store.insert(key.to_string(), Arc::new(value));
    }

    /// 取出之前存入的任意类型，需要显式标注类型
    ///
    /// # 示例
    /// ```ignore
    /// ctx.insert("user_id".to_string(), 42i64);
    /// if let Some(id) = ctx.get::<i64>("user_id") {
    ///     println!("{}", id);
    /// }
    /// ```
    pub fn get<T: Send + Sync + 'static>(&self, key: &str) -> Option<&T> {
        self.store.get(key)?.downcast_ref::<T>()
    }
    

    pub fn header(&self, key: &str) -> Option<&String> {
        self.headers.get(key)
    }

    pub fn body_string(&self) -> String {
        String::from_utf8_lossy(&self.body).to_string()
    }

    /// 将 JSON Body 绑定到结构体（最推荐，结构明确）
    ///
    /// 对应 Gin: `c.ShouldBindJSON(&struct)`
    pub fn body_json<T: serde::de::DeserializeOwned>(
        &self,
    ) -> std::result::Result<T, serde_json::Error> {
        serde_json::from_slice(&self.body)
    }

    /// 将 JSON Body 绑定到结构体（`body_json` 的别名，与 Gin 命名对齐）
    ///
    /// 对应 Gin: `c.ShouldBindJSON(&struct)`
    pub fn should_bind_json<T: serde::de::DeserializeOwned>(
        &self,
    ) -> std::result::Result<T, serde_json::Error> {
        serde_json::from_slice(&self.body)
    }

    /// 将 JSON Body 绑定到动态 Map（适用于动态 JSON 结构）
    ///
    /// 对应 Gin: `c.ShouldBindJSON(&map)`
    pub fn should_bind_json_map(
        &self,
    ) -> std::result::Result<serde_json::Map<String, serde_json::Value>, serde_json::Error> {
        serde_json::from_slice(&self.body)
    }

    /// 将 JSON 数组 Body 绑定到结构体切片
    ///
    /// 对应 Gin: `c.ShouldBindJSON(&[]struct)`
    pub fn should_bind_json_array<T: serde::de::DeserializeOwned>(
        &self,
    ) -> std::result::Result<Vec<T>, serde_json::Error> {
        serde_json::from_slice(&self.body)
    }

    /// 获取原始 Body 数据（字节切片）
    ///
    /// 对应 Gin: `c.GetRawData()`
    pub fn get_raw_data(&self) -> &[u8] {
        &self.body
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

    pub fn session(&self) -> Session {
        session::get_session_from_headers(&self.headers)
    }

    // ============== Query 参数方法 ==============

    /// 获取单个查询参数值，不存在返回空字符串
    ///
    /// 对应 Gin: `c.Query(key)`
    pub fn query(&self, key: &str) -> &str {
        self.query
            .get(key)
            .and_then(|v| v.first())
            .map(|s| s.as_str())
            .unwrap_or("")
    }

    /// 获取单个查询参数值，不存在返回默认值
    ///
    /// 对应 Gin: `c.DefaultQuery(key, default)`
    pub fn default_query<'a>(&'a self, key: &str, default: &'a str) -> &'a str {
        self.query
            .get(key)
            .and_then(|v| v.first())
            .map(|s| s.as_str())
            .unwrap_or(default)
    }

    /// 获取单个查询参数值并判断是否存在
    ///
    /// 对应 Gin: `c.GetQuery(key)`
    pub fn get_query(&self, key: &str) -> (&str, bool) {
        match self.query.get(key).and_then(|v| v.first()) {
            Some(v) => (v.as_str(), true),
            None => ("", false),
        }
    }

    /// 获取同名参数的多个值
    ///
    /// 对应 Gin: `c.QueryArray(key)`
    pub fn query_array(&self, key: &str) -> &[String] {
        self.query.get(key).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// 获取所有查询参数（只取每个 key 的第一个值）
    ///
    /// 对应 Gin: `c.QueryMap(key)` — 简化版，返回所有参数映射
    pub fn query_map(&self) -> &HashMap<String, Vec<String>> {
        &self.query
    }

    /// 将查询参数绑定到结构体（需要 serde 反序列化支持）
    ///
    /// 对应 Gin: `c.ShouldBindQuery(&struct)`
    ///
    /// 将查询参数转换为 `HashMap<String, String>`（每个 key 取第一个值），
    /// 然后通过 serde 反序列化为目标结构体。
    pub fn should_bind_query<T: serde::de::DeserializeOwned>(
        &self,
    ) -> Result<T, serde_json::Error> {
        let map: HashMap<String, String> = self
            .query
            .iter()
            .map(|(k, v)| (k.clone(), v.first().cloned().unwrap_or_default()))
            .collect();
        // 使用 serde_json 将 HashMap 转为 JSON 再反序列化，实现类似 Gin 的绑定效果
        let json = serde_json::to_value(&map)?;
        serde_json::from_value(json)
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

/// 解析 URL 查询字符串为 `HashMap<String, Vec<String>>`
///
/// 支持重复 key（如 `?a=1&a=2` → `"a" => ["1", "2"]`）
pub fn parse_query_string(query_str: &str) -> HashMap<String, Vec<String>> {
    let mut result = HashMap::new();
    if query_str.is_empty() {
        return result;
    }
    for pair in query_str.split('&') {
        if pair.is_empty() {
            continue;
        }
        let mut parts = pair.splitn(2, '=');
        let key = parts.next().unwrap_or("");
        let value = parts.next().unwrap_or("");

        let decoded_key = String::from_utf8_lossy(&url_decode(key.as_bytes())).to_string();
        let decoded_value = String::from_utf8_lossy(&url_decode(value.as_bytes())).to_string();

        result
            .entry(decoded_key)
            .or_insert_with(Vec::new)
            .push(decoded_value);
    }
    result
}
