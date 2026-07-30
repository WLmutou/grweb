use crate::{
    AppConfig, ConnectionPool, Method, Response, Router, ServerConfig, SharedPool,
    WebSocket, LoggingConfig,
};
use gorust::{flush_go_batch, go, go_task, net::{AsyncTcpListener, AsyncTcpStream}};
use grorm::ConnectionPool as dbConnectionPool;
use grlog::{LoggerBuilder, Target, LevelFilter};
use grlog::{error, info};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

pub struct Server {
    config: AppConfig,
    router: Arc<Router>,
    pool: SharedPool,
}

impl Server {
    pub fn new(app_config: AppConfig, mut router: Router) -> Self {
        let log_config = &app_config.logging;
        init_logger(log_config);

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

    pub fn run(self) -> std::io::Result<()> {
        gorust::Runtime::init();

        // 保存主线程句柄，用于 Ctrl-C 时唤醒 accept()
        let main_thread = std::thread::current();

        // 1. 设置 Ctrl-C 信号处理器
        let shutdown_flag = Arc::new(AtomicBool::new(false));
        let flag_for_signal = shutdown_flag.clone();
        let _ = ctrlc::set_handler(move || {
            flag_for_signal.store(true, Ordering::SeqCst);
            // 设置 accept() 关闭标志，让 accept() 返回 Interrupted 错误
            gorust::net::shutdown_accept();
            // 唤醒主线程（accept() 中 thread::park()），否则会一直阻塞
            main_thread.unpark();
        });

        let addr = self.config.server.addr();
        let socket_addr: std::net::SocketAddr = addr.parse().map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, format!("Invalid address: {}", e))
        })?;
        let listener = match AsyncTcpListener::bind(socket_addr) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("Error: Failed to bind to {}: {}", addr, e);
                if e.kind() == std::io::ErrorKind::AddrInUse {
                    eprintln!("Hint: Port {} is already in use. Please stop the existing process or use a different port.", addr);
                }
                return Err(e);
            }
        };

        let addr_for_log = addr.clone();
        go(move || {println!("Server listening on {}", addr_for_log)});
        flush_go_batch();

        let router = self.router.clone();
        let config = Arc::new(self.config.server);
        let pool = self.pool.clone();

        // 2. 主循环：接受连接并处理
        loop {
            match listener.accept() {
                Ok((stream, _)) => {
                    if shutdown_flag.load(Ordering::SeqCst) {
                        break;
                    }
                    if !pool.try_acquire() {
                        let _ = send_503_and_close_async(stream);
                        continue;
                    }
                    let router = router.clone();
                    let config = config.clone();
                    let pool = pool.clone();
                    let mut task = HandleConnectionTask::new(stream, router, config.as_ref());
                    go_task(move || -> bool {
                        let done = task.poll();
                        if done {
                            pool.release();
                        }
                        done
                    });
                    // 刷新协程创建缓冲区，确保处理请求的协程被加入调度队列
                    flush_go_batch();
                }
                Err(e) => {
                    // 如果是 Ctrl-C 导致的 Interrupted 错误，直接退出
                    if e.kind() == std::io::ErrorKind::Interrupted || shutdown_flag.load(Ordering::SeqCst) {
                        break;
                    }
                    error!("Connection failed: {}", e);
                }
            }
        }

        // 3. 关闭运行时：停止所有工作线程和定时器，所有 goroutine 退出
        info!("Shutting down all goroutines...");
        gorust::shutdown();

        Ok(())
    }
}



fn send_503_and_close_async(stream: AsyncTcpStream) -> std::io::Result<()> {
    let response =
        b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
    stream.write_all(response)?;
    Ok(())
}

/// 状态机：处理单个 HTTP 请求，支持在 I/O 等待时让出 goroutine。
struct HandleConnectionTask {
    stream: Option<AsyncTcpStream>,
    buffer: Vec<u8>,
    read_len: usize,
    response_bytes: Vec<u8>,
    written: usize,
    state: ConnectionState,
    router: Arc<Router>,
}

enum ConnectionState {
    /// 正在读取请求（尚未完成）
    ReadRequest,
    /// 已读完请求，开始处理（CPU 计算阶段，不应 yield）
    ProcessRequest,
    /// 正在写入响应
    WriteResponse,
    /// 完成
    Done,
}

impl HandleConnectionTask {
    fn new(stream: AsyncTcpStream, router: Arc<Router>, config: &ServerConfig) -> Self {
        if config.tcp_nodelay {
            let _ = stream.set_nodelay(true);
        }
        HandleConnectionTask {
            stream: Some(stream),
            buffer: vec![0u8; config.read_buffer_size],
            read_len: 0,
            response_bytes: Vec::new(),
            written: 0,
            state: ConnectionState::ReadRequest,
            router,
        }
    }

    /// 推进状态机。
    /// 返回 `true` = 处理完成，`false` = 让出（等待 I/O）。
    fn poll(&mut self) -> bool {
        loop {
            match self.state {
                ConnectionState::ReadRequest => {
                    let stream = match self.stream.as_ref() {
                        Some(s) => s,
                        None => {
                            self.state = ConnectionState::Done;
                            return true;
                        }
                    };
                    let buf_len = self.buffer.len();
                    if self.read_len >= buf_len {
                        // buffer 满了但还没完整请求，扩容
                        self.buffer.resize(buf_len * 2, 0);
                    }
                    match stream.try_read(&mut self.buffer[self.read_len..]) {
                        Ok(0) => {
                            self.state = ConnectionState::Done;
                            return true;
                        }
                        Ok(n) => {
                            self.read_len += n;
                            self.state = ConnectionState::ProcessRequest;
                            continue;
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            stream.wait_readable_yield();
                            return false;
                        }
                        Err(_) => {
                            self.state = ConnectionState::Done;
                            return true;
                        }
                    }
                }
                ConnectionState::ProcessRequest => {
                    match parse_http_request(&self.buffer[..self.read_len]) {
                        Some((method, path, body, _header_end, _req_keep_alive, headers, query_string)) => {
                            // WebSocket 升级：同步处理，转移 stream 所有权
                            if is_websocket_upgrade(&method, &headers) {
                                if let Some(ws_handler) = self.router.find_ws(&path) {
                                    if let Some(key) = headers.get("Sec-WebSocket-Key") {
                                        if let Some(stream) = self.stream.take() {
                                            if let Some(ws) = WebSocket::accept(stream, key) {
                                                ws_handler(ws);
                                            }
                                        }
                                    }
                                }
                                self.state = ConnectionState::Done;
                                return true;
                            }

                            let req_data = if body.is_empty() {
                                Vec::new()
                            } else {
                                body.to_vec()
                            };
                            let response = self.router.handle_request(method, path, req_data, headers, query_string);
                            self.response_bytes = format_response_fast(&response, false);
                            grlog::debug!(
                                "Generated response {} ({} bytes)",
                                response.status,
                                self.response_bytes.len()
                            );
                            self.state = ConnectionState::WriteResponse;
                            self.written = 0;
                            continue;
                        }
                        None => {
                            // 请求还不完整，继续读取
                            self.state = ConnectionState::ReadRequest;
                            continue;
                        }
                    }
                }
                ConnectionState::WriteResponse => {
                    if self.written >= self.response_bytes.len() {
                        self.state = ConnectionState::Done;
                        return true;
                    }
                    let stream = match self.stream.as_ref() {
                        Some(s) => s,
                        None => {
                            self.state = ConnectionState::Done;
                            return true;
                        }
                    };
                    match stream.try_write(&self.response_bytes[self.written..]) {
                        Ok(0) => {
                            self.state = ConnectionState::Done;
                            return true;
                        }
                        Ok(n) => {
                            self.written += n;
                            continue;
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            stream.wait_writable_yield();
                            return false;
                        }
                        Err(_) => {
                            self.state = ConnectionState::Done;
                            return true;
                        }
                    }
                }
                ConnectionState::Done => {
                    return true;
                }
            }
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
) -> Option<(
    Method,
    String,
    &[u8],
    usize,
    bool,
    HashMap<String, String>,
    String,
)> {
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
    let query_string = match path.split_once('?') {
        Some((_, qs)) => qs.to_string(),
        None => String::new(),
    };
    let path = path.split('?').next().unwrap_or(&path).to_string();
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

    Some((method, path, body, header_end, keep_alive, headers, query_string))
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


pub fn init_logger(log_config: &LoggingConfig) {
    let level = match log_config.level.to_lowercase().as_str() {
            "trace" => LevelFilter::Trace,
            "debug" => LevelFilter::Debug,
            "info" => LevelFilter::Info,
            "warn" => LevelFilter::Warn,
            "error" => LevelFilter::Error,
            "off" => LevelFilter::Off,
            _ => LevelFilter::Info,
        };

        let target = match log_config.output.to_lowercase().as_str() {
            "file" => {
                if let Some(ref file_path) = log_config.file {
                    Target::File(PathBuf::from(file_path))
                } else {
                    Target::Stderr
                }
            }
            "stdout" => Target::Stdout,
            _ => Target::Stderr,
        };

        let mut builder = LoggerBuilder::new();
        builder.filter_level(level).target(target);
        builder.try_init();
}