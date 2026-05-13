use crate::router::Handler;
use crate::{Context, Response};
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
    if index >= middlewares.len() {
        return final_handler(ctx);
    }

    let next = |ctx: Context| -> Response { run_chain(middlewares, index + 1, final_handler, ctx) };

    middlewares[index].call(ctx, &next)
}

pub struct LoggerMiddleware;

impl Middleware for LoggerMiddleware {
    fn call(&self, ctx: Context, next: &dyn Fn(Context) -> Response) -> Response {
        let start = std::time::Instant::now();
        let method = ctx.method.as_str().to_string();
        let path = ctx.path.clone();
        log::info!("--> {} {}", method, path);

        let response = next(ctx);

        let duration = start.elapsed();
        log::info!(
            "<-- {} {} ({}ms)",
            response.status,
            method,
            duration.as_millis()
        );

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
                log::error!("Panic recovered: {:?}", err);
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
