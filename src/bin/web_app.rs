use grweb::{Server, Router, Context, Response, AppConfig, middleware::{LoggerMiddleware, RecoveryMiddleware, CORSMiddleware}};
use serde_json::json;

fn main() {
    let config = AppConfig::load("config.toml").expect("Failed to load config");

    unsafe {
        std::env::set_var("RUST_LOG", &config.logging.level);
    }
    env_logger::init();

    let mut router = Router::new();

    router.use_middleware(LoggerMiddleware);
    router.use_middleware(RecoveryMiddleware);
    router.use_middleware(CORSMiddleware::new(
        config.cors.allowed_origins.clone(),
        config.cors.allowed_methods.clone(),
        config.cors.allowed_headers.clone(),
    ));

    router.get("/", |_ctx: Context| {
        Response::html("<h1>Welcome to Gorust Web Framework!</h1>".to_string())
    });

    router.get("/hello/:name", |ctx: Context| {
        let default_name = "World".to_string();
        let name = ctx.param("name").unwrap_or(&default_name);
        Response::html(format!("<h1>Hello, {}!</h1>", name))
    });

    router.post("/api/user", |ctx: Context| {
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
        use std::time::Duration;
        std::thread::sleep(Duration::from_secs(2));
        Response::html("<h1>Slow response completed!</h1>".to_string())
    });

    let addr = config.server.addr();
    let server = Server::new(config.server, router);

    println!("Server running at http://{}", addr);
    if let Err(e) = server.run() {
        eprintln!("Server error: {}", e);
    }
}