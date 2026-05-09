use std::io::{Read, Write};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use mio::{Events, Interest, Poll, Token, Waker};
use mio::net::{TcpListener, TcpStream};
use crossbeam_channel::{bounded, Sender};
use gorust::{go, runtime};
use log::{info, error, debug};
use crate::{Router, Response, Method};

// Token 分配
const SERVER_TOKEN: Token = Token(0);
const WAKER_TOKEN: Token = Token(1);
const FIRST_CONNECTION_TOKEN: usize = 2;

/// HTTP 连接状态
struct Connection {
    stream: TcpStream,
    buffer: Vec<u8>,        // 读缓冲区
    write_buffer: Vec<u8>,  // 写缓冲区
    closing: bool,
}

impl Connection {
    fn new(stream: TcpStream) -> Self {
        Self {
            stream,
            buffer: Vec::with_capacity(8192),
            write_buffer: Vec::new(),
            closing: false,
        }
    }
    
    fn append_read_data(&mut self, data: &[u8]) {
        self.buffer.extend_from_slice(data);
    }
    
    fn append_write_data(&mut self, data: &[u8]) {
        self.write_buffer.extend_from_slice(data);
    }
}

/// 事件驱动服务器
pub struct Server {
    addr: String,
    router: Arc<Router>,
    worker_pool_size: usize,
}

impl Server {
    pub fn new(addr: &str, router: Router) -> Self {
        Self {
            addr: addr.to_string(),
            router: Arc::new(router),
            worker_pool_size: num_cpus::get(),
        }
    }
    
    pub fn with_worker_pool(mut self, size: usize) -> Self {
        self.worker_pool_size = size;
        self
    }
    
    #[runtime]
    pub fn run(self) -> std::io::Result<()> {
        self.run_event_loop()
    }
    
    fn run_event_loop(&self) -> std::io::Result<()> {
        let mut poll = Poll::new()?;
        let mut events = Events::with_capacity(1024);
        
        // 创建 TCP 监听器
        let mut listener = TcpListener::bind(self.addr.parse().unwrap())?;
        poll.registry().register(&mut listener, SERVER_TOKEN, Interest::READABLE)?;
        
        // 创建唤醒器，用于跨线程通信
        let waker = Waker::new(poll.registry(), WAKER_TOKEN)?;
        let waker = Arc::new(Mutex::new(waker));
        
        // 创建工作通道
        let (request_tx, request_rx) = bounded::<(usize, Request)>(self.worker_pool_size * 4);
        let (response_tx, response_rx) = bounded::<(usize, Response)>(self.worker_pool_size * 4);
        
        // 连接存储
        let connections = Arc::new(Mutex::new(HashMap::<usize, Connection>::new()));
        let next_token = Arc::new(Mutex::new(FIRST_CONNECTION_TOKEN));
        
        // 启动工作线程池
        let router = self.router.clone();
        for i in 0..self.worker_pool_size {
            let request_rx = request_rx.clone();
            let response_tx = response_tx.clone();
            let router = router.clone();
            
            std::thread::spawn(move || {
                info!("Worker thread {} started", i);
                for (conn_id, req) in request_rx {
                    debug!("Worker {} processing request from connection {}", i, conn_id);
                    let response = router.handle_request(req.method, &req.path, req.body);
                    if let Err(e) = response_tx.send((conn_id, response)) {
                        error!("Worker {} failed to send response: {}", i, e);
                    }
                }
            });
        }
        
        // 启动响应处理 goroutine
        let connections_clone = connections.clone();
        let waker_clone = waker.clone();
        
        go(move || {
            for (conn_id, response) in response_rx {
                debug!("Response ready for connection {}", conn_id);
                
                let mut conns = connections_clone.lock().unwrap();
                if let Some(conn) = conns.get_mut(&conn_id) {
                    let response_bytes = format_response_fast(&response);
                    conn.append_write_data(&response_bytes);
                    
                    // 唤醒主循环来处理注册
                    let _ = waker_clone.lock().unwrap().wake();
                } else {
                    debug!("Connection {} not found, dropping response", conn_id);
                }
            }
        });
        
        info!("Event-driven server listening on {}", self.addr);
        
        // 主事件循环
        let next_token_clone = next_token.clone();
        let connections_clone_for_accept = connections.clone();
        let mut request_tx_clone = request_tx.clone();
        
        // 记录需要注册写事件的连接
        let mut connections_need_write = std::collections::HashSet::new();
        
        loop {
            // 处理需要注册写事件的连接
            for &conn_id in &connections_need_write {
                let mut conns = connections.lock().unwrap();
                if let Some(conn) = conns.get_mut(&conn_id) {
                    if !conn.write_buffer.is_empty() {
                        debug!("Re-registering connection {} for writable", conn_id);
                        let _ = poll.registry().reregister(
                            &mut conn.stream,
                            Token(conn_id),
                            Interest::READABLE | Interest::WRITABLE
                        );
                    }
                }
            }
            connections_need_write.clear();
            
            // 等待事件
            match poll.poll(&mut events, Some(Duration::from_millis(100))) {
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {
                    continue;
                }
                Err(e) => {
                    error!("Poll error: {}", e);
                    return Err(e);  // 修复：返回错误而不是 break
                }
            }
            
            for event in events.iter() {
                match event.token() {
                    SERVER_TOKEN => {
                        // 接受新连接
                        while let Ok((mut stream, addr)) = listener.accept() {
                            debug!("New connection from {}", addr);
                            
                            let conn_id = {
                                let mut id = next_token_clone.lock().unwrap();
                                let current = *id;
                                *id += 1;
                                current
                            };
                            
                            // mio::net::TcpStream 默认就是非阻塞的
                            let _ = stream.set_nodelay(true);
                            
                            let token = Token(conn_id);
                            if let Err(e) = poll.registry().register(&mut stream, token, Interest::READABLE) {
                                error!("Failed to register connection {}: {}", conn_id, e);
                                continue;
                            }
                            
                            let conn = Connection::new(stream);
                            connections_clone_for_accept.lock().unwrap().insert(conn_id, conn);
                            
                            debug!("Connection {} assigned and registered", conn_id);
                        }
                    }
                    
                    WAKER_TOKEN => {
                        debug!("Event loop woken up");
                        // 收集需要写事件的连接
                        let conns = connections.lock().unwrap();
                        for (&conn_id, conn) in conns.iter() {
                            if !conn.write_buffer.is_empty() {
                                connections_need_write.insert(conn_id);
                            }
                        }
                    }
                    
                    Token(token_id) => {
                        // 处理现有连接
                        let mut conns = connections.lock().unwrap();
                        if let Some(conn) = conns.get_mut(&token_id) {
                            if event.is_readable() {
                                if let Err(e) = handle_read(conn, token_id, &mut request_tx_clone) {
                                    debug!("Read error for connection {}: {}", token_id, e);
                                    let _ = poll.registry().deregister(&mut conn.stream);
                                    conns.remove(&token_id);
                                    continue;
                                }
                            }
                            
                            if event.is_writable() && !conn.write_buffer.is_empty() {
                                match handle_write(conn, token_id, poll.registry()) {
                                    Ok(need_close) => {
                                        if need_close {
                                            debug!("Closing connection {}", token_id);
                                            let _ = poll.registry().deregister(&mut conn.stream);
                                            conns.remove(&token_id);
                                            continue;
                                        }
                                    }
                                    Err(e) => {
                                        debug!("Write error for connection {}: {}", token_id, e);
                                        let _ = poll.registry().deregister(&mut conn.stream);
                                        conns.remove(&token_id);
                                        continue;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// 请求结构
struct Request {
    method: Method,
    path: String,
    body: Vec<u8>,
}

/// 处理读事件
fn handle_read(
    conn: &mut Connection, 
    conn_id: usize, 
    request_tx: &mut Sender<(usize, Request)>
) -> std::io::Result<()> {
    let mut buf = [0; 8192];
    
    loop {
        match conn.stream.read(&mut buf) {
            Ok(0) => {
                // 连接关闭
                conn.closing = true;
                break;
            }
            Ok(n) => {
                conn.append_read_data(&buf[..n]);
                
                // 尝试解析完整的 HTTP 请求
                if let Some((method, path, body, header_end)) = parse_http_request(&conn.buffer) {
                    debug!("Received complete request from connection {}: {} {}", conn_id, method.as_str(), path);
                    
                    let request = Request {
                        method,
                        path,
                        body: body.to_vec(),
                    };
                    
                    // 发送到工作线程
                    if let Err(e) = request_tx.send((conn_id, request)) {
                        error!("Failed to send request to worker: {}", e);
                        break;
                    }
                    
                    // 保留剩余数据（可能有 pipelining）
                    let remaining = conn.buffer[header_end..].to_vec();
                    conn.buffer.clear();
                    if !remaining.is_empty() {
                        conn.buffer.extend_from_slice(&remaining);
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                break;
            }
            Err(e) => {
                return Err(e);
            }
        }
    }
    
    Ok(())
}

/// 处理写事件，返回是否需要关闭连接
fn handle_write(conn: &mut Connection, conn_id: usize, registry: &mio::Registry) -> std::io::Result<bool> {
    while !conn.write_buffer.is_empty() {
        match conn.stream.write(&conn.write_buffer) {
            Ok(n) => {
                let _ = conn.write_buffer.drain(..n);
                debug!("Wrote {} bytes to connection {}", n, conn_id);
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                break;
            }
            Err(e) => {
                return Err(e);
            }
        }
    }
    
    // 如果写缓冲区已经清空
    if conn.write_buffer.is_empty() {
        if conn.closing {
            // 连接要关闭
            return Ok(true);
        }
        
        // 只需要 READABLE 事件
        debug!("Write buffer empty for connection {}, switching to READABLE only", conn_id);
        let _ = registry.reregister(&mut conn.stream, Token(conn_id), Interest::READABLE);
    }
    
    Ok(false)
}

/// 解析 HTTP 请求
fn parse_http_request(buffer: &[u8]) -> Option<(Method, String, &[u8], usize)> {
    // 查找请求结束位置
    let header_end = find_headers_end(buffer)?;
    
    // 解析请求行
    let request_line_end = buffer.iter().position(|&b| b == b'\n')?;
    let request_line = &buffer[..request_line_end];
    
    // 查找方法和路径
    let first_space = request_line.iter().position(|&b| b == b' ')?;
    let second_space = request_line[first_space+1..].iter().position(|&b| b == b' ').map(|p| first_space + 1 + p)?;
    
    let method_bytes = &request_line[..first_space];
    let path_bytes = &request_line[first_space+1..second_space];
    
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
    
    Some((method, path, body, header_end))
}

/// 查找 HTTP 头结束位置 (\r\n\r\n)
fn find_headers_end(buffer: &[u8]) -> Option<usize> {
    for i in 0..buffer.len().saturating_sub(3) {
        if buffer[i] == b'\r' && buffer[i+1] == b'\n' && 
           buffer[i+2] == b'\r' && buffer[i+3] == b'\n' {
            return Some(i + 4);
        }
    }
    None
}

/// 快速格式化响应
fn format_response_fast(response: &Response) -> Vec<u8> {
    let status_line: &'static [u8] = match response.status {
        200 => b"HTTP/1.1 200 OK\r\n",
        201 => b"HTTP/1.1 201 Created\r\n",
        204 => b"HTTP/1.1 204 No Content\r\n",
        400 => b"HTTP/1.1 400 Bad Request\r\n",
        404 => b"HTTP/1.1 404 Not Found\r\n",
        500 => b"HTTP/1.1 500 Internal Server Error\r\n",
        _ => b"HTTP/1.1 200 OK\r\n",
    };
    
    let body_len = response.body.len();
    
    // 预计算容量
    let mut total_len = status_line.len() + 50 + body_len;
    
    let content_length = format!("Content-Length: {}\r\n", body_len);
    total_len += content_length.len();
    
    let connection = b"Connection: close\r\n";
    total_len += connection.len();
    
    for (k, v) in &response.headers {
        total_len += k.len() + v.len() + 4;
    }
    
    total_len += 2; // 最后的空行
    
    let mut result = Vec::with_capacity(total_len);
    
    result.extend_from_slice(status_line);
    result.extend_from_slice(content_length.as_bytes());
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