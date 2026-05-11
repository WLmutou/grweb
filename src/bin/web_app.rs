
use gorust::go;
use grweb::{Server, Router, Context, Response, AppConfig, middleware::{LoggerMiddleware, RecoveryMiddleware, CORSMiddleware}};
use serde_json::json;



fn hello_handler(ctx: Context) -> Response {
    let default_name = "World".to_string();
    let name = ctx.param("name").unwrap_or(&default_name);
    Response::html(format!("<h1>Hello, {}!</h1>", name))
}

fn headers_handler(ctx: Context) -> Response {
    let mut body = String::from("<h1>Request Headers</h1><ul>");
    for (k, v) in &ctx.headers {
        body.push_str(&format!("<li><b>{}</b>: {}</li>", k, v));
    }
    body.push_str("</ul>");
    Response::html(body)
}

fn create_user_handler(ctx: Context) -> Response {
    let body_str = ctx.body_string();
    let response = json!({
        "status": "success",
        "message": format!("Received: {}", body_str),
        "data": body_str
    });
    Response::json(response.to_string())
}

fn get_users_handler(_ctx: Context) -> Response {
    let users = json!([
        {"id": 1, "name": "Alice"},
        {"id": 2, "name": "Bob"}
    ]);
    Response::json(users.to_string())
}

fn slow_handler(_ctx: Context) -> Response {
    use std::time::Duration;
    std::thread::sleep(Duration::from_secs(20));
    Response::html("<h1>Slow response completed!</h1>".to_string())
}




// ============== 静态路由处理器 ==============
fn handle_home(_ctx: Context) -> Response{
    let body = r#"<html>
        <head>
            <title>GRweb Web Server</title> 
            <meta http-equiv="Content-Type" content="text/html;charset=utf-8">
        </head>
        <body>
            <h1>🚀 Welcome to GRweb Web Server</h1>
            <p>High-performance web server powered by GRweb!</p>
            
            <h2>Available Routes:</h2>
            <ul>
                <li><a href="/">/</a> - Home</li>
                <li><a href="/hello">/hello</a> - Hello page</li>
                <li><a href="/json">/json</a> - JSON response</li>
                <li><a href="/about">/about</a> - About page</li>
                <li><a href="/status">/status</a> - Server status</li>
                <li><a href="/user/123">/user/123</a> - Dynamic user (123)</li>
                <li><a href="/user/alice">/user/alice</a> - Dynamic user (alice)</li>
                <li><a href="/post/2024/01/hello-world">/post/2024/01/hello-world</a> - Dynamic post</li>
            </ul>
            
            <h2>Performance:</h2>
            <ul>
                <li>QPS: ~95,000</li>
                <li>Latency: ~0.59ms</li>
                <li>Memory: ~3MB</li>
            </ul>
        </body>
    </html>"#;
    Response::html(body)
}

fn handle_hello(_ctx: Context) -> Response {
    let body = r#"<html>
        <head>
            <meta http-equiv="Content-Type" content="text/html;charset=utf-8">
        </head>
        <body>
            <h1>👋 Hello from GRweb!</h1>
            <p>Served by goroutine with high performance!</p>
            <p>This request was handled by a lightweight M:N scheduler.</p>
            <p><a href="/">← Back to home</a></p>
        </body>
    </html>"#;
    Response::html(body)
}

fn handle_json(_ctx: Context) -> Response {
    let body = r#"{
        "status": "ok",
        "message": "Hello from GRweb",
        "framework": "GRweb",
        "version": "0.2.0",
        "performance": {
            "qps": 95000,
            "latency_ms": 0.59,
            "max_latency_ms": 4.82
        },
        "features": [
            "goroutine",
            "channel",
            "M:N scheduling",
            "work stealing",
            "non-blocking I/O",
            "dynamic routing"
        ]
    }"#;
    Response::json(body)
}

fn handle_about(_ctx: Context) -> Response {
    let body = r#"<html>
        <head>
            <meta http-equiv="Content-Type" content="text/html;charset=utf-8">
        </head>
        <body>
            <h1>About GRweb</h1>
            <p>GRweb is a lightweight, high-performance runtime for Rust that provides Go-like concurrency.</p>
            
            <h2>Core Features:</h2>
            <ul>
                <li><strong>M:N Goroutine Scheduling</strong> - Efficient user-space threads</li>
                <li><strong>Work Stealing</strong> - Automatic load balancing</li>
                <li><strong>Channel-based Communication</strong> - Safe concurrent message passing</li>
                <li><strong>Non-blocking I/O</strong> - High throughput networking</li>
                <li><strong>Low Memory Footprint</strong> - ~3MB base memory</li>
                <li><strong>High Throughput</strong> - 95,000 req/s</li>
                <li><strong>Low Latency</strong> - ~0.59ms average</li>
            </ul>
            
            <h2>Architecture:</h2>
            <ul>
                <li>G (Goroutine) - User-space task</li>
                <li>P (Processor) - Logical CPU core</li>
                <li>M (Machine) - OS thread</li>
                <li>Scheduler - Work stealing across Ps</li>
            </ul>
            
            <p><a href="/">← Back to home</a></p>
        </body>
    </html>"#;
    Response::html(body)
}

fn handle_status(_ctx: Context) -> Response {
    let mut body = String::new();
    body.push_str(
        r#"<html>
        <head>
            <meta http-equiv="Content-Type" content="text/html;charset=utf-8">
            <title>Server Status</title>
        </head> 
        <body>
            <h1>📊 Server Status</h1>
            <table border="1" cellpadding="10">
                <tr><th>Metric</th><th>Value</th></tr>
                <tr><td>Framework</td><td>GRweb v0.2.0</td></tr>
                <tr><td>Status</td><td style="color:green">✓ Running</td></tr>
                <tr><td>QPS</td><td>~95,000</td></tr>
                <tr><td>Average Latency</td><td>~0.59ms</td></tr>
                <tr><td>Max Latency</td><td>~4.82ms</td></tr>
                <tr><td>Memory Usage</td><td>~3MB</td></tr>
                <tr><td>Concurrency Model</td><td>M:N Goroutines</td></tr>
                <tr><td>Scheduler</td><td>Work Stealing</td></tr>
            </table>
            <p><a href="/">← Back to home</a></p>
        </body>
    </html>"#,
    );
    Response::html(body)
}


fn handle_user(ctx: Context) -> Response {
    let user_id = ctx.param("id").map(|s| s.as_str()).unwrap_or("unknown");

    let mut body = String::new();
    body.push_str(
        r#"<html>
        <head>
            <meta http-equiv="Content-Type" content="text/html;charset=utf-8">
        </head>
        <body>
            <h1>👤 User Profile</h1>
            <p><strong>User ID:</strong> "#,
    );
    body.push_str(user_id);
    body.push_str(
        r#"</p>
            <p><strong>Page:</strong> Dynamic route handling</p>
            <p>This page demonstrates dynamic routing with path parameters.</p>
            <p><a href="/">← Back to home</a></p>
        </body>
    </html>"#,
    );

    Response::html(body)    
}



fn handle_post(ctx: Context) -> Response {
    let year = ctx.param("year").map(|s| s.as_str()).unwrap_or("unknown");
    let month = ctx.param("month").map(|s| s.as_str()).unwrap_or("unknown");
    let slug = ctx.param("slug").map(|s| s.as_str()).unwrap_or("unknown");

    let mut body = String::new();
    body.push_str(
        r#"<html>
        <head>
            <meta http-equiv="Content-Type" content="text/html;charset=utf-8">
        </head>
        <body>
            <h1>📝 Blog Post</h1>
            <p><strong>Date:</strong> "#,
    );
    body.push_str(year);
    body.push_str("-");
    body.push_str(month);
    body.push_str(
        r#"</p>
            <p><strong>Slug:</strong> "#,
    );
    body.push_str(slug);
    body.push_str(
        r#"</p>
            <p>This is a dynamically routed blog post page.</p>
            <p>The routing pattern <code>/post/:year/:month/:slug</code> matches this URL.</p>
            <p><a href="/">← Back to home</a></p>
        </body>
    </html>"#,
    );

    Response::html(body)
}




fn grweb_print() {
    println!("╔════════════════════════════════════════════════════════════╗");
    println!("║                  grweb   Web Server                        ║");
    println!("║             High-performance HTTP Server                   ║");
    println!("╚════════════════════════════════════════════════════════════╝");
    println!();
    println!("📍 Server running at: http://127.0.0.1:9030");
    println!();
    println!("📋 Available Routes:");
    println!("   ┌─────────────────────────────────────────────────────────┐");
    println!("   │ Static Routes:                                          │");
    println!("   │   GET  /              - Home page                       │");
    println!("   │   GET  /hello         - Hello page                      │");
    println!("   │   GET  /json          - JSON response                   │");
    println!("   │   GET  /about         - About page                      │");
    println!("   │   GET  /status        - Server status                   │");
    println!("   │                                                        │");
    println!("   │ Dynamic Routes:                                         │");
    println!("   │   GET  /user/:id      - User profile (dynamic ID)       │");
    println!("   │   GET  /post/:year/:month/:slug - Blog post            │");
    println!("   └─────────────────────────────────────────────────────────┘");
    println!();
    println!("💡 Examples:");
    println!("   curl http://127.0.0.1:9030/user/123");
    println!("   curl http://127.0.0.1:9030/post/2024/01/hello-world");
    println!();
    println!("⚡ Performance: ~95,000 req/s | Latency: ~0.59ms");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!();
}


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

    // 使用集中的路由设置函数
    router.get("/", handle_home);
    router.get("/hello/:name", hello_handler);
    router.post("/api/user", create_user_handler);
    router.get("/api/users", get_users_handler);
    router.get("/slow", slow_handler);
    router.get("/about", handle_about);
    router.get("/status", handle_status);
    router.get("/json", handle_json);
    router.get("/user/:id", handle_user);
    router.get("/post/:year/:month/:slug", handle_post);
    router.get("/hello", handle_hello);
    router.get("/headers", headers_handler);

    // print routes info
    go(grweb_print);
    // run server
    let server = Server::new(config.server, router);
    if let Err(e) = server.run() {
        eprintln!("Server error: {}", e);
    }
}