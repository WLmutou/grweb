use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::time::Duration;
use gorust::{go, runtime, yield_now};
use log::{info, error};
use crate::{Router, Response, Method};

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
        let listener = TcpListener::bind(&self.addr)?;
        listener.set_nonblocking(true)?;

        info!("Server listening on {}", self.addr);

        let router = self.router.clone();

        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    let router = router.clone();
                    go(move || {
                        handle_connection(stream, &router);
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_micros(100));
                }
                Err(e) => {
                    error!("Connection failed: {}", e);
                }
            }
        }

        Ok(())
    }
}

fn handle_connection(mut stream: TcpStream, router: &Router) {
    let _ = stream.set_nonblocking(true);
    let _ = stream.set_nodelay(true);

    let mut buffer = [0u8; 8192];
    let mut wait_us: u64 = 10;

    loop {
        match stream.read(&mut buffer) {
            Ok(0) => return,
            Ok(n) => {
                if let Some((method, path, body, _header_end)) = parse_http_request(&buffer[..n]) {
                    let response = router.handle_request(method, &path, body.to_vec());
                    let response_bytes = format_response_fast(&response);
                    let _ = stream.write_all(&response_bytes);
                    let _ = stream.flush();
                }
                return;
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_micros(wait_us));
                wait_us = (wait_us * 2).min(500);
                yield_now();
            }
            Err(_) => return,
        }
    }
}

fn parse_http_request(buffer: &[u8]) -> Option<(Method, String, &[u8], usize)> {
    let header_end = find_headers_end(buffer)?;

    let request_line_end = buffer.iter().position(|&b| b == b'\n')?;
    let request_line = &buffer[..request_line_end];

    let first_space = request_line.iter().position(|&b| b == b' ')?;
    let second_space = request_line[first_space + 1..]
        .iter()
        .position(|&b| b == b' ')
        .map(|p| first_space + 1 + p)?;

    let method_bytes = &request_line[..first_space];
    let path_bytes = &request_line[first_space + 1..second_space];

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

fn find_headers_end(buffer: &[u8]) -> Option<usize> {
    for i in 0..buffer.len().saturating_sub(3) {
        if buffer[i] == b'\r'
            && buffer[i + 1] == b'\n'
            && buffer[i + 2] == b'\r'
            && buffer[i + 3] == b'\n'
        {
            return Some(i + 4);
        }
    }
    None
}

fn format_response_fast(response: &Response) -> Vec<u8> {
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
    let content_length = format!("Content-Length: {}\r\n", body_len);
    let connection = b"Connection: close\r\n";

    let mut total_len = status_line.len() + content_length.len() + connection.len() + body_len + 2;
    for (k, v) in &response.headers {
        total_len += k.len() + v.len() + 4;
    }

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