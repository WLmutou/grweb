# grweb

[![Rust](https://img.shields.io/badge/rust-2024%20edition-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

基于 [gorust](https://github.com/WLmutou/gorust) 协程运行时的高性能 Rust Web 框架，结合 Go 风格的并发模型与 Rust 的零成本抽象。

## 特性

- **Go 风格并发** — 基于 gorust 的 goroutine-per-connection 模型，每个连接一个轻量级协程
- **树形路由** — 支持路径参数 `/hello/:name`，精确匹配优先于参数匹配
- **中间件链** — 零堆分配的洋葱模型中间件，内置 Logger / Recovery / CORS
- **TOML 配置** — 所有参数通过 `config.toml` 统一管理，均有合理默认值
- **低 CPU 空闲** — 阻塞 I/O + 指数退避调度，空闲 CPU < 1%
- **高性能** — release 模式 ~89,000 QPS（与 gorust 原生示例差距 < 3%）

## 快速开始

### 1. 添加依赖

```toml
[dependencies]
grweb = { path = "http://github.com/WLmutou/grweb.git" }
serde_json = "1.0"
env_logger = "0.10"
```

### 2. 编写应用

```rust
use grweb::{Server, Router, Context, Response, ServerConfig,
    middleware::{LoggerMiddleware, RecoveryMiddleware, CORSMiddleware}};

fn main() {
    env_logger::init();

    let mut router = Router::new();

    // 全局中间件
    router.use_middleware(LoggerMiddleware);
    router.use_middleware(RecoveryMiddleware);
    router.use_middleware(CORSMiddleware::new(
        vec!["*".to_string()],
        vec!["GET".to_string(), "POST".to_string()],
        vec!["Content-Type".to_string()],
    ));

    // 路由
    router.get("/", |_ctx: Context| {
        Response::html("<h1>Hello grweb!</h1>")
    });

    router.get("/hello/:name", |ctx: Context| {
        let name = ctx.param("name").unwrap_or(&"World".to_string());
        Response::html(format!("<h1>Hello, {}!</h1>", name))
    });

    // 启动
    let config = ServerConfig::default();
    Server::new(config, router).run().unwrap();
}
```

### 3. 运行

```bash
RUST_LOG=info cargo run --release
```

访问 `http://127.0.0.1:9030/hello/grweb` 看到 `Hello, grweb!`

---

## 配置文件

创建 `config.toml`（所有字段均可选）：

```toml
[server]
host = "127.0.0.1"       # 监听地址（默认 127.0.0.1）
port = 9030              # 监听端口（默认 9030）
worker_pool_size = 4     # 工作线程数（默认 CPU 核心数）
read_buffer_size = 8192  # 读缓冲区字节数（默认 8192）
tcp_nodelay = true       # TCP_NODELAY（默认 true）

[logging]
level = "error"          # trace | debug | info | warn | error（默认 info）

[cors]
allowed_origins = ["*"]
allowed_methods = ["GET", "POST", "PUT", "DELETE", "OPTIONS"]
allowed_headers = ["Content-Type"]
```

加载配置：

```rust
use grweb::AppConfig;

let config = AppConfig::load("config.toml").expect("Failed to load config");

// 日志级别自动设置
unsafe { std::env::set_var("RUST_LOG", &config.logging.level); }
env_logger::init();

// CORS 从配置读取
router.use_middleware(CORSMiddleware::new(
    config.cors.allowed_origins,
    config.cors.allowed_methods,
    config.cors.allowed_headers,
));

// 服务器从配置构建
let server = Server::new(config.server, router);
```

---

## 路由

### 基本路由

```rust
router.get("/", handler);
router.post("/api/user", handler);
router.put("/api/user/:id", handler);
router.delete("/api/user/:id", handler);
```

### 路径参数

以 `:` 前缀定义参数，通过 `ctx.param()` 获取：

```rust
router.get("/user/:id", |ctx: Context| {
    let user_id = ctx.param("id").unwrap();
    Response::html(format!("User: {}", user_id))
});

router.get("/post/:year/:month", |ctx: Context| {
    let year = ctx.param("year").unwrap();
    let month = ctx.param("month").unwrap();
    // ...
});
```

### 路由优先级

精确匹配优先于参数匹配。例如同时注册 `/user/me` 和 `/user/:id`：

```rust
router.get("/user/me", |_| Response::html("It's me!"));
router.get("/user/:id", |ctx| {
    // /user/me 走上面，/user/123 走这里
});
```

---

## 中间件

### 内置中间件

| 中间件 | 说明 |
|--------|------|
| `LoggerMiddleware` | 记录请求方法、路径、状态码、耗时 |
| `RecoveryMiddleware` | 捕获 handler panic，返回 500 |
| `CORSMiddleware` | 添加跨域响应头 |

### 自定义中间件

实现 `Middleware` trait：

```rust
use grweb::{Middleware, Context, Response};

struct AuthMiddleware;

impl Middleware for AuthMiddleware {
    fn call(&self, ctx: Context, next: &dyn Fn(Context) -> Response) -> Response {
        // 前置逻辑
        if ctx.path.starts_with("/admin") {
            return Response::new(403, "Forbidden");
        }

        // 调用下一个中间件 / handler
        let mut response = next(ctx);

        // 后置逻辑
        response.headers.push(("X-Powered-By".to_string(), "grweb".to_string()));

        response
    }
}

// 注册
router.use_middleware(AuthMiddleware);
```

### 中间件链执行顺序

注册顺序即执行顺序（洋葱模型）：

```
请求 → Logger → Recovery → CORS → Handler → CORS → Recovery → Logger → 响应
```

---

## Context API

```rust
pub struct Context {
    pub method: Method,                    // HTTP 方法
    pub path: String,                      // 请求路径
    pub params: HashMap<String, String>,   // 路径参数
    pub body: Vec<u8>,                     // 请求体
}

impl Context {
    // 获取路径参数
    pub fn param(&self, key: &str) -> Option<&String>;

    // 请求体转字符串
    pub fn body_string(&self) -> String;

    // 请求体转 JSON（需 serde::Deserialize）
    pub fn body_json<T: DeserializeOwned>(&self) -> Result<T, serde_json::Error>;
}
```

---

## Response API

```rust
// 基础响应
Response::new(200, "OK")
Response::new(404, "Not Found")

// 快捷方法
Response::html("<h1>Title</h1>")          // Content-Type: text/html
Response::json(r#"{"key":"val"}"#)        // Content-Type: application/json
Response::not_found()                      // 404
Response::internal_error()                 // 500

// 自动类型转换
"hello"           → Response::html("hello")
"hello".to_string() → Response::html("hello")
vec![1,2,3]       → Response::new(200, vec![1,2,3])

// 自定义响应头
let mut resp = Response::html("<h1>OK</h1>");
resp.headers.push(("X-Custom".to_string(), "value".to_string()));
```

---

## 性能

wrk 压测（4 线程 / 100 连接 / 5 秒，AMD Ryzen）：

| 服务 | QPS | 延迟 | 备注 |
|------|-----|------|------|
| gorust web_server_yield | 89,300 | 626μs | 无路由/无解析/预构建响应 |
| gorust web_server_router | 72,900 | 816μs | HashMap 路由 |
| **grweb** | **88,000** | 660μs | 树形路由 + 3 层中间件 + 完整解析 |

空闲 CPU：**< 1%**（阻塞 I/O + 指数退避调度）

---

## features
- Keep-Alive 连接复用
- 请求头解析（无法获取 Cookie/Authorization）
- 静态文件服务
- WebSocket 支持
- 表单数据解析（multipart/urlencoded）
- HTTPS 支持
- 连接池管理
- 优雅降级和限流
- 测试用例（未见 tests/ 目录）

## License

MIT © 2026 WLmutou