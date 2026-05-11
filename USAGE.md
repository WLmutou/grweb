# 使用 grweb 框架

## 快速开始

### 1. 添加依赖

```toml
[dependencies]
grweb = "0.1.0"
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