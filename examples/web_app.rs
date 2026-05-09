use grweb::{Server, Router, Context, Response, middleware::{LoggerMiddleware, RecoveryMiddleware, CORSMiddleware}};
use serde_json::json;

fn main() {
    env_logger::init();
    
    let mut router = Router::new();
    
    // 添加全局中间件
    router.use_middleware(LoggerMiddleware);
    router.use_middleware(RecoveryMiddleware);
    router.use_middleware(CORSMiddleware::new(vec!["*".to_string()]));
    
    // 路由配置
    router.get("/", |_ctx: Context| {
        Response::html("<h1>Welcome to Gorust Web Framework!</h1>".to_string())
    });
    
    router.get("/hello/:name", |ctx: Context| {
        let default_name = "World".to_string();
        let name = ctx.param("name").unwrap_or(&default_name);
        Response::html(format!("<h1>Hello, {}!</h1>", name))
    });
    
    router.post("/api/user", |ctx: Context| {
        // 处理 JSON 请求
        let body_str = ctx.body_string();
        let response = json!({
            "status": "success",
            "message": format!("Received: {}", body_str),
            "data": body_str
        });
        Response::json(response.to_string())
    });
    
    router.get("/api/users", |_ctx: Context| {
        let users = json!([
            {"id": 1, "name": "Alice"},
            {"id": 2, "name": "Bob"}
        ]);
        Response::json(users.to_string())
    });
    
    router.get("/slow", |_ctx: Context| {
        // 演示 goroutine 特性：模拟慢请求
        use std::time::Duration;
        std::thread::sleep(Duration::from_secs(2));
        Response::html("<h1>Slow response completed!</h1>".to_string())
    });
    
    // 启动服务器
    let server = Server::new("127.0.0.1:8080", router)
        .with_worker_pool(4);  // 4个 worker goroutines
    
    println!("🚀 Server running at http://127.0.0.1:8080");
    if let Err(e) = server.run() {
        eprintln!("Server error: {}", e);
    }
}