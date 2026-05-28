use crate::router::Handler;
use crate::{Context, Response};
use grlog::{info, error};
use std::sync::Arc;

pub trait Middleware: Send + Sync {
    fn call(&self, ctx: Context, next: &dyn Fn(Context) -> Response) -> Response;
}

pub struct MiddlewareChain;

impl MiddlewareChain {
    pub fn process(
        middlewares: &[Arc<dyn Middleware>],
        final_handler: &Handler,
        ctx: Context,
    ) -> Response {
        run_chain(middlewares, 0, final_handler, ctx)
    }
}

fn run_chain(
    middlewares: &[Arc<dyn Middleware>],
    index: usize,
    final_handler: &Handler,
    ctx: Context,
) -> Response {
    // 使用迭代方式实现中间件链，避免在gorust环境中的递归问题
    // 使用一个栈来存储中间件调用的状态
    struct MiddlewareCall {
        index: usize,
        ctx: Context,
        state: CallState,
    }

    enum CallState {
        Enter,  // 进入中间件
        Exit(Response),  // 退出中间件，携带响应
    }

    // 对于简单情况，使用递归实现（但确保在gorust中不会造成问题）
    // 实际上，使用闭包构建洋葱模型
    if index >= middlewares.len() {
        return final_handler(ctx);
    }

    // 构建中间件调用链 - 从最外层到最内层
    let mut current_handler: Box<dyn Fn(Context) -> Response> = Box::new(|ctx| {
        eprintln!("[DEBUG] current_handler: calling final_handler");
        let resp = final_handler(ctx);
        eprintln!("[DEBUG] current_handler: final_handler returned, status={}", resp.status);
        resp
    });

    // 从最后一个中间件向前构建处理链
    for i in (0..middlewares.len()).rev() {
        let middleware = middlewares[i].clone();
        let outer_handler = current_handler;

        current_handler = Box::new(move |ctx| {
            eprintln!("[DEBUG] middleware[{}] handler: calling middleware.call", i);
            
            let response = middleware.call(ctx, &|ctx| {
                eprintln!("[DEBUG] middleware[{}] next closure: calling outer_handler", i);
                let resp = outer_handler(ctx);
                eprintln!("[DEBUG] middleware[{}] next closure: outer_handler returned, status={}", i, resp.status);
                resp
            });
            
            eprintln!("[DEBUG] middleware[{}] handler: middleware.call returned, status={}", i, response.status);
            response
        });
    }

    // 执行整个中间件链
    eprintln!("[DEBUG middleware] About to call current_handler");
    let result = current_handler(ctx);
    eprintln!("[DEBUG middleware] current_handler returned, status={}", result.status);
    result
}

// 替代方案：使用迭代方式的完整实现
pub fn process(
    middlewares: &[Arc<dyn Middleware>],
    final_handler: &Handler,
    ctx: Context,
) -> Response {
    run_chain(middlewares, 0, final_handler, ctx)
}

pub struct LoggerMiddleware;

impl Middleware for LoggerMiddleware {
    fn call(&self, ctx: Context, next: &dyn Fn(Context) -> Response) -> Response {
        let start = std::time::Instant::now();
        let method = ctx.method.as_str().to_string();
        let path = ctx.path.clone();
        eprintln!("[INFO] --> {} {}", method, path);

        let response = next(ctx);
        let duration = start.elapsed();
        eprintln!("[INFO] <-- {} {} ({}ms)", response.status, method, duration.as_millis());

        response
    }
}

pub struct RecoveryMiddleware;

impl Middleware for RecoveryMiddleware {
    fn call(&self, ctx: Context, next: &dyn Fn(Context) -> Response) -> Response {
        use std::panic::AssertUnwindSafe;
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| next(ctx)));

        match result {
            Ok(response) => response,
            Err(err) => {
                error!("Panic recovered: {:?}", err);
                crate::error::Error::internal("Internal server error occurred").to_response()
            }
        }
    }
}

pub struct CORSMiddleware {
    allowed_origins: Vec<String>,
    allowed_methods: Vec<String>,
    allowed_headers: Vec<String>,
}

impl CORSMiddleware {
    pub fn new(
        allowed_origins: Vec<String>,
        allowed_methods: Vec<String>,
        allowed_headers: Vec<String>,
    ) -> Self {
        Self {
            allowed_origins,
            allowed_methods,
            allowed_headers,
        }
    }
}

impl Middleware for CORSMiddleware {
    fn call(&self, ctx: Context, next: &dyn Fn(Context) -> Response) -> Response {
        let mut response = next(ctx);

        let origin = "*".to_string();
        if self.allowed_origins.contains(&origin) || self.allowed_origins.contains(&"*".to_string())
        {
            response
                .headers
                .push(("Access-Control-Allow-Origin".to_string(), origin));
            response.headers.push((
                "Access-Control-Allow-Methods".to_string(),
                self.allowed_methods.join(", "),
            ));
            response.headers.push((
                "Access-Control-Allow-Headers".to_string(),
                self.allowed_headers.join(", "),
            ));
        }

        response
    }
}
