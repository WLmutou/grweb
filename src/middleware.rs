use std::sync::Arc;
use crate::{Context, Response};
use crate::router::Handler;

pub type Next = Box<dyn FnOnce(Context) -> Response + Send>;

/// 中间件 trait
pub trait Middleware: Send + Sync {
    fn call(&self, ctx: Context, next: Next) -> Response;
}

/// 中间件链
pub struct MiddlewareChain {
    middlewares: Vec<Arc<dyn Middleware>>,
    final_handler: Handler,
    index: usize,
}

impl MiddlewareChain {
    pub fn new(middlewares: Vec<Arc<dyn Middleware>>, final_handler: Handler) -> Self {
        Self {
            middlewares,
            final_handler,
            index: 0,
        }
    }
    
    pub fn process(mut self, ctx: Context) -> Response {
        if self.index < self.middlewares.len() {
            let middleware = self.middlewares[self.index].clone();
            self.index += 1;
            
            let next = Box::new(move |ctx: Context| {
                self.process(ctx)
            });
            
            middleware.call(ctx, next)
        } else {
            (self.final_handler)(ctx)
        }
    }
}

/// 日志中间件示例
pub struct LoggerMiddleware;

impl Middleware for LoggerMiddleware {
    fn call(&self, ctx: Context, next: Next) -> Response {
        let start = std::time::Instant::now();
        log::info!("--> {} {}", ctx.method.as_str(), ctx.path);
        
        let ctx_clone = ctx.clone();
        let response = next(ctx_clone);
        
        let duration = start.elapsed();
        log::info!("<-- {} {} ({}ms)", 
            response.status, 
            ctx.method.as_str(),
            duration.as_millis()
        );
        
        response
    }
}

/// 恢复中间件（panic 处理）
pub struct RecoveryMiddleware;

impl Middleware for RecoveryMiddleware {
    fn call(&self, ctx: Context, next: Next) -> Response {
        use std::panic::AssertUnwindSafe;
        let ctx_clone = ctx.clone();
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            next(ctx_clone)
        }));
        
        match result {
            Ok(response) => response,
            Err(err) => {
                log::error!("Panic recovered: {:?}", err);
                Response::internal_error()
            }
        }
    }
}

/// CORS 中间件
pub struct CORSMiddleware {
    allowed_origins: Vec<String>,
}

impl CORSMiddleware {
    pub fn new(allowed_origins: Vec<String>) -> Self {
        Self { allowed_origins }
    }
}

impl Middleware for CORSMiddleware {
    fn call(&self, ctx: Context, next: Next) -> Response {
        let mut response = next(ctx);
        
        let origin = "*".to_string(); // 简化实现，实际应从请求头获取
        if self.allowed_origins.contains(&origin) || self.allowed_origins.contains(&"*".to_string()) {
            response.headers.push(("Access-Control-Allow-Origin".to_string(), origin));
            response.headers.push(("Access-Control-Allow-Methods".to_string(), "GET, POST, PUT, DELETE, OPTIONS".to_string()));
            response.headers.push(("Access-Control-Allow-Headers".to_string(), "Content-Type".to_string()));
        }
        
        response
    }
}