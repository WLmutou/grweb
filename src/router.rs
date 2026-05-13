use crate::static_files;
use crate::{
    Context, Error, Method, Middleware, MiddlewareChain, PoolStats, Response, SharedPool, WebSocket,
};
use grorm::ConnectionPool;
use log::debug;
use std::collections::HashMap;
use std::sync::Arc;

pub type Handler = Arc<dyn Fn(Context) -> Response + Send + Sync>;
pub type ResultHandler = Arc<dyn Fn(Context) -> Result<Response, Error> + Send + Sync>;
pub type WsHandler = Arc<dyn Fn(WebSocket) + Send + Sync>;

/// 路由节点（支持路径参数）
pub struct RouterNode {
    handlers: HashMap<Method, Handler>,
    children: HashMap<String, RouterNode>,
    param_child: Option<Box<RouterNode>>,
    param_name: Option<String>,
}

impl RouterNode {
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
            children: HashMap::new(),
            param_child: None,
            param_name: None,
        }
    }

    fn insert(&mut self, method: Method, path: &str, handler: Handler) {
        let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
        self.insert_parts(method, &parts, handler);
    }

    fn insert_parts(&mut self, method: Method, parts: &[&str], handler: Handler) {
        if parts.is_empty() {
            self.handlers.insert(method, handler);
            return;
        }

        let part = parts[0];
        let remaining = &parts[1..];

        if part.starts_with(":") {
            let param_name = part[1..].to_string();
            if self.param_child.is_none() {
                self.param_child = Some(Box::new(RouterNode::new()));
                self.param_name = Some(param_name.clone());
            }
            if let Some(ref mut child) = self.param_child {
                child.insert_parts(method, remaining, handler);
            }
        } else {
            let child = self
                .children
                .entry(part.to_string())
                .or_insert_with(RouterNode::new);
            child.insert_parts(method, remaining, handler);
        }
    }

    fn find(&self, method: &Method, path: &str) -> Option<(Handler, HashMap<String, String>)> {
        let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
        let mut params = HashMap::new();
        if let Some(handler) = self.find_parts(method, &parts, &mut params) {
            Some((handler, params))
        } else {
            None
        }
    }

    fn find_parts(
        &self,
        method: &Method,
        parts: &[&str],
        params: &mut HashMap<String, String>,
    ) -> Option<Handler> {
        if parts.is_empty() {
            if let Some(handler) = self.handlers.get(method) {
                return Some(handler.clone());
            }
            return None;
        }

        let part = parts[0];
        let remaining = &parts[1..];

        // 尝试精确匹配
        if let Some(child) = self.children.get(part) {
            if let Some(handler) = child.find_parts(method, remaining, params) {
                return Some(handler);
            }
        }

        // 尝试参数匹配
        if let Some(ref param_child) = self.param_child {
            if let Some(ref param_name) = self.param_name {
                params.insert(param_name.clone(), part.to_string());
                return param_child.find_parts(method, remaining, params);
            }
        }

        None
    }
}

/// 路由器
pub struct Router {
    root: RouterNode,
    global_middlewares: Arc<Vec<Arc<dyn Middleware>>>,
    static_dirs: Vec<(String, String)>,
    ws_routes: HashMap<String, WsHandler>,
    pool: Option<SharedPool>,
    /// 数据库连接池
    db_pool: Arc<ConnectionPool>,
}

impl Router {
    pub fn new() -> Self {
        Self {
            root: RouterNode::new(),
            global_middlewares: Arc::new(Vec::new()),
            static_dirs: Vec::new(),
            ws_routes: HashMap::new(),
            pool: None,
            db_pool: Arc::new(ConnectionPool::default()),
        }
    }
    pub fn new_with_db(db_pool: Arc<ConnectionPool>) -> Self {
        Self {
            root: RouterNode::new(),
            global_middlewares: Arc::new(Vec::new()),
            static_dirs: Vec::new(),
            ws_routes: HashMap::new(),
            pool: None,
            db_pool: db_pool,
        }
    }

    pub fn get_db_pool(&self) -> Arc<ConnectionPool> {
        self.db_pool.clone()
    }

    pub fn set_pool(&mut self, pool: SharedPool) {
        self.pool = Some(pool);
    }

    pub fn set_db_pool(&mut self, db_pool: Arc<ConnectionPool>) {
        self.db_pool = db_pool;
    }

    pub fn pool_stats(&self) -> Option<PoolStats> {
        self.pool.as_ref().map(|p| p.stats())
    }

    pub fn use_middleware<M: Middleware + 'static>(&mut self, middleware: M) {
        Arc::make_mut(&mut self.global_middlewares).push(Arc::new(middleware));
    }

    pub fn add_route<F>(&mut self, method: Method, path: &str, handler: F)
    where
        F: Fn(Context) -> Response + Send + Sync + 'static,
    {
        let handler = Arc::new(handler);
        debug!("Adding route: {} {}", method.as_str(), path);
        self.root.insert(method, path, handler);
    }

    pub fn get<F, R>(&mut self, path: &str, handler: F)
    where
        F: Fn(Context) -> R + Send + Sync + 'static,
        R: Into<Response>,
    {
        self.add_route(Method::GET, path, move |ctx| handler(ctx).into());
    }

    pub fn post<F, R>(&mut self, path: &str, handler: F)
    where
        F: Fn(Context) -> R + Send + Sync + 'static,
        R: Into<Response>,
    {
        self.add_route(Method::POST, path, move |ctx| handler(ctx).into());
    }

    pub fn put<F, R>(&mut self, path: &str, handler: F)
    where
        F: Fn(Context) -> R + Send + Sync + 'static,
        R: Into<Response>,
    {
        self.add_route(Method::PUT, path, move |ctx| handler(ctx).into());
    }

    pub fn delete<F, R>(&mut self, path: &str, handler: F)
    where
        F: Fn(Context) -> R + Send + Sync + 'static,
        R: Into<Response>,
    {
        self.add_route(Method::DELETE, path, move |ctx| handler(ctx).into());
    }

    pub fn get_result<F>(&mut self, path: &str, handler: F)
    where
        F: Fn(Context) -> Result<Response, Error> + Send + Sync + 'static,
    {
        let handler = Arc::new(handler);
        debug!("Adding route: GET {}", path);
        self.root.insert(
            Method::GET,
            path,
            Arc::new(move |ctx| match handler(ctx) {
                Ok(resp) => resp,
                Err(err) => err.to_response(),
            }),
        );
    }

    pub fn post_result<F>(&mut self, path: &str, handler: F)
    where
        F: Fn(Context) -> Result<Response, Error> + Send + Sync + 'static,
    {
        let handler = Arc::new(handler);
        debug!("Adding route: POST {}", path);
        self.root.insert(
            Method::POST,
            path,
            Arc::new(move |ctx| match handler(ctx) {
                Ok(resp) => resp,
                Err(err) => err.to_response(),
            }),
        );
    }

    pub fn put_result<F>(&mut self, path: &str, handler: F)
    where
        F: Fn(Context) -> Result<Response, Error> + Send + Sync + 'static,
    {
        let handler = Arc::new(handler);
        debug!("Adding route: PUT {}", path);
        self.root.insert(
            Method::PUT,
            path,
            Arc::new(move |ctx| match handler(ctx) {
                Ok(resp) => resp,
                Err(err) => err.to_response(),
            }),
        );
    }

    pub fn delete_result<F>(&mut self, path: &str, handler: F)
    where
        F: Fn(Context) -> Result<Response, Error> + Send + Sync + 'static,
    {
        let handler = Arc::new(handler);
        debug!("Adding route: DELETE {}", path);
        self.root.insert(
            Method::DELETE,
            path,
            Arc::new(move |ctx| match handler(ctx) {
                Ok(resp) => resp,
                Err(err) => err.to_response(),
            }),
        );
    }

    pub fn serve_static(&mut self, url_prefix: &str, dir_path: &str) {
        let prefix = url_prefix.trim_end_matches('/').to_string();
        self.static_dirs.push((prefix, dir_path.to_string()));
    }

    pub fn websocket<F>(&mut self, path: &str, handler: F)
    where
        F: Fn(WebSocket) + Send + Sync + 'static,
    {
        self.ws_routes.insert(path.to_string(), Arc::new(handler));
    }

    pub fn find_ws(&self, path: &str) -> Option<WsHandler> {
        self.ws_routes.get(path).cloned()
    }

    pub fn handle_request(
        &self,
        method: Method,
        path: String,
        req_data: Vec<u8>,
        headers: HashMap<String, String>,
    ) -> Response {
        for (prefix, dir_path) in &self.static_dirs {
            if path.starts_with(prefix)
                && (path.len() == prefix.len() || path.as_bytes()[prefix.len()] == b'/')
            {
                return static_files::serve_file(dir_path, &path[prefix.len()..]);
            }
        }

        if let Some((handler, params)) = self.root.find(&method, &path) {
            let mut ctx = Context::new(method, path, params, headers, req_data);
            if let Some(ref pool) = self.pool {
                ctx = ctx.with_pool(pool.clone());
            }
            ctx = ctx.with_db_pool(self.db_pool.clone());
            MiddlewareChain::process(&self.global_middlewares, &handler, ctx)
        } else {
            Response::not_found()
        }
    }
}

impl Default for Router {
    fn default() -> Self {
        Self::new()
    }
}
