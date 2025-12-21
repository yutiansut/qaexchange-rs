// @yutiansut @quantaxis
// 代理目标配置
// 可通过环境变量 VUE_APP_API_HOST 和 VUE_APP_WS_HOST 配置后端地址
// 例如: VUE_APP_API_HOST=192.168.2.27 npm run serve
const API_HOST = process.env.VUE_APP_API_HOST || '127.0.0.1'
const API_PORT = process.env.VUE_APP_API_PORT || '8094'
const WS_HOST = process.env.VUE_APP_WS_HOST || process.env.VUE_APP_API_HOST || '127.0.0.1'
const WS_PORT = process.env.VUE_APP_WS_PORT || '8095'

module.exports = {
  publicPath: '/',
  outputDir: 'dist',
  assetsDir: 'static',
  lintOnSave: false,
  productionSourceMap: false,
  devServer: {
    port: 8096,
    host: '0.0.0.0',  // 允许外部访问
    open: true,
    proxy: {
      '/api': {
        target: `http://${API_HOST}:${API_PORT}`,
        changeOrigin: true,
        secure: false,
        pathRewrite: {
          '^/api': '/api'
        },
        // ✨ 修复 ECONNRESET 问题 @yutiansut @quantaxis
        // 禁用连接复用，避免连接池问题
        agent: false,
        // 超时配置
        timeout: 30000,
        proxyTimeout: 30000,
        // 错误处理：返回 JSON 错误而不是中断连接
        onError: (err, req, res) => {
          console.error(`[Proxy Error] ${req.method} ${req.url}:`, err.message)
          if (res && !res.headersSent) {
            res.writeHead(502, { 'Content-Type': 'application/json' })
            res.end(JSON.stringify({
              success: false,
              error: {
                code: 502,
                message: `Proxy error: ${err.message}`
              }
            }))
          }
        },
        // 代理请求前的处理
        onProxyReq: (proxyReq, req, res) => {
          // 确保 Content-Length 正确
          if (req.body) {
            const bodyData = JSON.stringify(req.body)
            proxyReq.setHeader('Content-Length', Buffer.byteLength(bodyData))
          }
        }
      },
      '/ws': {
        target: `http://${WS_HOST}:${WS_PORT}`,
        changeOrigin: true,
        ws: true,  // 启用 WebSocket 代理
        pathRewrite: {
          '^/ws': '/ws'
        }
      }
    }
  }
}
