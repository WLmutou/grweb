use crate::{
    AppConfig, ConnectionPool, Method, Response, Router, ServerConfig, SharedPool,
    WebSocket,
};
use gorust::{go, runtime};
use grorm::ConnectionPool as dbConnectionPool;
use log::{error, info};
use std::collections::HashMap;
use crate::grlog;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

pub struct Server {
    config: AppConfig,
    router: Arc<Router>,
    pool: SharedPool,
}

impl Server {
    pub fn new(app_config: AppConfig, mut router: Router) -> Self {
        // 初始化日志（必须在运行时启动之前）
        grlog::init_logging(&app_config.logging);

        let config = app_config;
        let config_server = &config.server;
        let pool = Arc::new(ConnectionPool::new(config_server.max_connections));
        router.set_pool(pool.clone());
        Self {
            config,
            router: Arc::new(router),
            pool,
        }
    }

    pub fn with_db_pool(mut self, db_pool: Arc<dbConnectionPool>) -> Self {
        Arc::get_mut(&mut self.router).unwrap().set_db_pool(db_pool);
        self
    }

    pub fn pool(&self) -> &SharedPool {
        &self.pool
    }

    #[runtime]
    pub fn run(self) -> std::io::Result<()> {
        let addr = self.config.server.addr();
        let listener = TcpListener::bind(&addr)?;

        let addr_for_log = addr.clone();
        go(move || {println!("Server listening on {}", addr_for_log)});

        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_flag = shutdown.clone();
        let shutdown_addr = self.config.server.addr();

        std::thread::spawn(move || {
            while gorust::scheduler::Scheduler::is_running() {
                std::thread::sleep(Duration::from_millis(100));
            }
            shutdown_flag.store(true, Ordering::SeqCst);
            let addr: std::net::SocketAddr = shutdown_addr.parse().unwrap();
            for _ in 0..10 {
                if TcpStream::connect_timeout(&addr, Duration::from_millis(100)).is_ok() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        });

        let router = self.router.clone();
        let config = Arc::new(self.config.server);
        let pool = self.pool.clone();

        for stream in listener.incoming() {
            if shutdown.load(Ordering::SeqCst) {
                info!("Server stopped");
                break;
            }
            match stream {
                Ok(stream) => {
                    if shutdown.load(Ordering::SeqCst) {
                        break;
                    }
                    if !pool.try_acquire() {
                        let _ = send_503_and_close(stream);
                        continue;
                    }
                    let router = router.clone();
                    let config = config.clone();
                    let pool = pool.clone();
                    go(move || {
                        handle_connection(stream, &router, &config);
                        pool.release();
                    });
                }
                Err(e) => {
                    if shutdown.load(Ordering::SeqCst) {
                        break;
                    }
                    error!("Connection failed: {}", e);
                }
            }
        }

        Ok(())
    }
}



fn send_503_and_close(mut stream: TcpStream) -> std::io::Result<()> {
    let response =
        b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
    stream.write_all(response)?;
    stream.flush()
}

fn handle_connection(mut stream: TcpStream, router: &Router, config: &ServerConfig) {
    if config.tcp_nodelay {
        let _ = stream.set_nodelay(true);
    }

    let keep_alive_timeout = Duration::from_secs(config.keep_alive_timeout);
    let mut buffer = vec![0u8; config.read_buffer_size];
    let mut keep_alive = true;

    while keep_alive {
        let _ = stream.set_read_timeout(Some(keep_alive_timeout));

        match stream.read(&mut buffer) {
            Ok(0) => return,
            Ok(n) => {
                if let Some((method, path, body, _header_end, req_keep_alive, headers)) =
                    parse_http_request(&buffer[..n])
                {
                    if is_websocket_upgrade(&method, &headers) {
                        if let Some(ws_handler) = router.find_ws(&path) {
                            if let Some(key) = headers.get("Sec-WebSocket-Key") {
                                if let Some(ws) = WebSocket::accept(stream, key) {
                                    ws_handler(ws);
                                }
                                return;
                            }
                        }
                    }

                    keep_alive = req_keep_alive;

                    let req_data = if body.is_empty() {
                        Vec::new()
                    } else {
                        body.to_vec()
                    };
                    let response = router.handle_request(method, path, req_data, headers);
                    let response_bytes = format_response_fast(&response, keep_alive);
                    if stream.write_all(&response_bytes).is_err() {
                        return;
                    }
                    let _ = stream.flush();
                } else {
                    return;
                }
            }
            Err(_) => return,
        }
    }
}

fn is_websocket_upgrade(method: &Method, headers: &HashMap<String, String>) -> bool {
    if *method != Method::GET {
        return false;
    }
    let upgrade = headers
        .get("Upgrade")
        .map(|v| v.to_lowercase() == "websocket")
        .unwrap_or(false);
    let connection = headers
        .get("Connection")
        .map(|v| v.to_lowercase().contains("upgrade"))
        .unwrap_or(false);
    let has_key = headers.contains_key("Sec-WebSocket-Key");
    let version = headers
        .get("Sec-WebSocket-Version")
        .map(|v| v == "13")
        .unwrap_or(false);

    upgrade && connection && has_key && version
}

fn parse_http_request(
    buffer: &[u8],
) -> Option<(Method, String, &[u8], usize, bool, HashMap<String, String>)> {
    let header_end = find_headers_end(buffer)?;

    let request_line_end = memchr::memchr(b'\n', buffer)?;
    let request_line = &buffer[..request_line_end];

    let first_space = memchr::memchr(b' ', request_line)?;
    let second_space =
        memchr::memchr(b' ', &request_line[first_space + 1..]).map(|p| first_space + 1 + p)?;

    let method_bytes = &request_line[..first_space];
    let path_bytes = &request_line[first_space + 1..second_space];
    let version_bytes = &request_line[second_space + 1..];

    let method = match method_bytes {
        b"GET" => Method::GET,
        b"POST" => Method::POST,
        b"PUT" => Method::PUT,
        b"DELETE" => Method::DELETE,
        b"PATCH" => Method::PATCH,
        b"HEAD" => Method::HEAD,
        b"OPTIONS" => Method::OPTIONS,
        _ => Method::GET,
    };

    let path = String::from_utf8_lossy(path_bytes).to_string();
    let body = if header_end < buffer.len() {
        &buffer[header_end..]
    } else {
        &[]
    };

    let is_http11 = version_bytes == b"HTTP/1.1";
    let headers_slice = &buffer[request_line_end + 1..header_end];
    let has_connection_close = has_header_value(headers_slice, b"Connection", b"close");
    let has_keep_alive = has_header_value(headers_slice, b"Connection", b"keep-alive");

    let keep_alive = if is_http11 {
        !has_connection_close
    } else {
        has_keep_alive
    };

    let headers = parse_headers(headers_slice);

    Some((method, path, body, header_end, keep_alive, headers))
}

fn find_headers_end(buffer: &[u8]) -> Option<usize> {
    buffer
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|p| p + 4)
}

fn parse_headers(headers_slice: &[u8]) -> HashMap<String, String> {
    let mut headers = HashMap::new();
    let mut pos = 0;
    while pos < headers_slice.len() {
        let line_end = match memchr::memchr(b'\n', &headers_slice[pos..]) {
            Some(p) => pos + p,
            None => headers_slice.len(),
        };
        let line = &headers_slice[pos..line_end];
        let line = if line.ends_with(b"\r") {
            &line[..line.len() - 1]
        } else {
            line
        };

        if let Some(colon) = memchr::memchr(b':', line) {
            let name = String::from_utf8_lossy(&line[..colon]).to_string();
            let value = String::from_utf8_lossy(line[colon + 1..].trim_ascii()).to_string();
            headers.insert(name, value);
        }

        pos = line_end + 1;
    }
    headers
}

fn has_header_value(headers: &[u8], name: &[u8], value: &[u8]) -> bool {
    let mut pos = 0;
    let name_lower = name.to_ascii_lowercase();
    while pos < headers.len() {
        let line_end = match memchr::memchr(b'\n', &headers[pos..]) {
            Some(p) => pos + p,
            None => headers.len(),
        };
        let line = &headers[pos..line_end];
        let line = if line.ends_with(b"\r") {
            &line[..line.len() - 1]
        } else {
            line
        };

        if let Some(colon) = memchr::memchr(b':', line) {
            let header_name = &line[..colon];
            if header_name.len() == name.len() && header_name.to_ascii_lowercase() == name_lower {
                let header_value = &line[colon + 1..];
                let header_value = header_value.trim_ascii();
                if header_value.to_ascii_lowercase() == value.to_ascii_lowercase() {
                    return true;
                }
            }
        }

        pos = line_end + 1;
    }
    false
}

fn format_response_fast(response: &Response, keep_alive: bool) -> Vec<u8> {
    let status_line: &[u8] = match response.status {
        200 => b"HTTP/1.1 200 OK\r\n",
        201 => b"HTTP/1.1 201 Created\r\n",
        204 => b"HTTP/1.1 204 No Content\r\n",
        400 => b"HTTP/1.1 400 Bad Request\r\n",
        404 => b"HTTP/1.1 404 Not Found\r\n",
        500 => b"HTTP/1.1 500 Internal Server Error\r\n",
        _ => b"HTTP/1.1 200 OK\r\n",
    };

    let body_len = response.body.len();

    let mut cl_buf = itoa::Buffer::new();
    let content_length_str = cl_buf.format(body_len);

    let cl_header = b"Content-Length: ";
    let cl_suffix = b"\r\n";
    let connection: &[u8] = if keep_alive {
        b"Connection: keep-alive\r\n"
    } else {
        b"Connection: close\r\n"
    };

    let mut total_len = status_line.len()
        + cl_header.len()
        + content_length_str.len()
        + cl_suffix.len()
        + connection.len()
        + body_len
        + 2;

    for (k, v) in &response.headers {
        total_len += k.len() + v.len() + 4;
    }

    let mut result = Vec::with_capacity(total_len);

    result.extend_from_slice(status_line);
    result.extend_from_slice(cl_header);
    result.extend_from_slice(content_length_str.as_bytes());
    result.extend_from_slice(cl_suffix);
    result.extend_from_slice(connection);

    for (k, v) in &response.headers {
        result.extend_from_slice(k.as_bytes());
        result.extend_from_slice(b": ");
        result.extend_from_slice(v.as_bytes());
        result.extend_from_slice(b"\r\n");
    }

    result.extend_from_slice(b"\r\n");

    if body_len > 0 {
        result.extend_from_slice(&response.body);
    }

    result
}
