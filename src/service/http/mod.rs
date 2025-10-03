//! HTTP API 服务

pub struct HttpServer {}

impl HttpServer {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for HttpServer {
    fn default() -> Self {
        Self::new()
    }
}
