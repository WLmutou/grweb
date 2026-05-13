use serde::Serialize;
use gorust::go;
use grweb::{Server, Router, Context, Response, AppConfig, WebSocket, Message, middleware::{LoggerMiddleware, RecoveryMiddleware, CORSMiddleware}};
use serde_json::json;
use grorm::{ConnectionConfig, ConnectionPool as DbConnectionPool, PostgresDriverFactory, QueryBuilder, Error};
use grorm::DeriveModel;
use std::sync::Arc;


#[derive(Debug, DeriveModel, Serialize)]
#[table("res_user")]
struct ResUser {
    id: i64,                          // 自动主键
    #[index]                          // 单列索引
    name: String,
    #[unique]                         // 单列唯一约束
    email: String,
    age: i32,
}
impl ResUser {
    fn new(name: String, email: String, age: i32) -> Self {
        Self { id: 0, name, email, age }
    }

    fn create(&self, ctx: &Context) -> Result<(), Error> {
        let db_pool = ctx.get_db_pool();
        let mut conn = db_pool.get()?;
        let mut qb = QueryBuilder::<ResUser>::new(conn.driver_mut());
        qb.create_table()?;
        qb.insert(self)?;
        Ok(())
    }
}



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

fn form_handler(ctx: Context) -> Response {
    let values = ctx.form_values();
    let mut body = String::from("<h1>Form Data</h1><ul>");
    for (k, v) in &values {
        body.push_str(&format!("<li><b>{}</b>: {}</li>", k, v));
    }
    body.push_str("</ul>");
    Response::html(body)
}

fn form_page_handler(_ctx: Context) -> Response {
    let html = r#"<!DOCTYPE html>
<html>
<head><meta charset="utf-8"><title>Form Test</title></head>
<body>
<h1>Form Test</h1>
<h2>URL-Encoded Form</h2>
<form action="/form" method="post">
    <input name="username" placeholder="Username"><br>
    <input name="email" placeholder="Email"><br>
    <input name="age" placeholder="Age"><br>
    <button type="submit">Submit URL-Encoded</button>
</form>
<h2>Multipart Form，create user</h2>
<form action="/api/user" method="post" enctype="multipart/form-data">
    <input name="name" placeholder="Name"><br>
    <input name="email" placeholder="Email"><br>
    <input name="age" placeholder="Age"><br>
    <button type="submit">Submit Multipart</button>
</form>
</body>
</html>"#;
    Response::html(html)
}

fn ws_handler(mut ws: WebSocket) {
    let welcome = r#"{"type":"welcome","message":"Connected to WebSocket server"}"#;
    ws.send_text(welcome);

    loop {
        match ws.read_message() {
            Some(Message::Text(text)) => {
                let reply = format!(r#"{{"type":"echo","data":"{}"}}"#, text);
                ws.send_text(&reply);
            }
            Some(Message::Binary(data)) => {
                ws.send_binary(&data);
            }
            Some(Message::Ping(data)) => {
                ws.send_pong(&data);
            }
            Some(Message::Close(_)) => {
                break;
            }
            _ => break,
        }
    }
}

fn pool_stats_handler(ctx: Context) -> Response {
    match ctx.pool_stats() {
        Some(stats) => {
            let json = json!({
                "active_connections": stats.active_connections,
                "total_connections": stats.total_connections,
                "rejected_connections": stats.rejected_connections,
                "max_connections": if stats.max_connections == 0 { "unlimited".to_string() } else { stats.max_connections.to_string() },
            });
            Response::json(json.to_string())
        }
        None => Response::json(r#"{"error":"pool not available"}"#),
    }
}

fn create_user_handler(ctx: Context) -> Result<Response, Error> {
     let values = ctx.form_values();
    let user = ResUser::new(
        values["name"].to_string(),
        values["email"].to_string(),
        values["age"].parse::<i32>().unwrap(),
    );
    user.create(&ctx)?;
    Ok(Response::json(user))
}
    

fn get_users_handler(ctx: Context) -> Result<Response, Error> {
    // ctx.get_db_pool()?;
    let mut conn = ctx.get_db_pool().get()?;
    let mut db = QueryBuilder::<ResUser>::new(conn.driver_mut());
    let users = db.find_all()?;
    Ok(Response::json(users))
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
                <li><a href="/form">/form</a> - Form test</li>
                <li><a href="/headers">/headers</a> - Request headers test</li>
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
        ],
        "database_connected": true
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
            <li><strong>Database Integration</strong> - Integrated connection pooling</li>
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
                <tr><td>Database Pool</td><td>Integrated</td></tr>
            </table>
            <p><a href="/">← Back to home</a></p>
        </body>
    </html>"#,
    );
    Response::html(body)
}


fn handle_user(ctx: Context) -> Response {
    let user_id = ctx.param("id").map(|s| s.as_str()).unwrap_or("unknown");
        // 如果没有数据库连接池，则使用原始逻辑
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
    println!("   │   GET  /form          - Form page                        │");
    println!("   │   GET  /headers       - Headers page                     │");
    println!("   │   GET  /static/*      - Static file server             │");
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
    println!("📚 Database: Integrated connection pool");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!();
}


#[gorust::runtime]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = AppConfig::load("src/bin/config.toml").expect("Failed to load config");

    unsafe {
        std::env::set_var("RUST_LOG", &config.logging.level);
    }

    let dbconf = config.database;
    println!("{:?}", dbconf);
    let dbconfig = ConnectionConfig::new(&dbconf.host, 
                    dbconf.port, 
                    &dbconf.username, 
                    &dbconf.password, 
                    &dbconf.database);
    
    // 创建数据库连接池
    let db_pool = Arc::new(DbConnectionPool::new(PostgresDriverFactory, dbconfig, dbconf.max_size));

    env_logger::init();

    let mut router = Router::new_with_db(db_pool.clone());
    
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
    router.get("/form", form_page_handler);
    router.post("/form", form_handler);

    router.websocket("/ws", ws_handler);

    router.get("/pool/stats", pool_stats_handler);

    router.serve_static("/static", &config.server.static_dir);

    // print routes info
    go(grweb_print);
    // run server with database pool
    let server = Server::new(config.server, router);
    if let Err(e) = server.run() {
        eprintln!("Server error: {}", e);
    }
    Ok(())
}