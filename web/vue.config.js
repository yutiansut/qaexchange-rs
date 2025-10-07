module.exports = {
  publicPath: '/',
  outputDir: 'dist',
  assetsDir: 'static',
  lintOnSave: false,
  productionSourceMap: false,
  devServer: {
    port: 8096,
    open: true,
    proxy: {
      '/api': {
        target: 'http://127.0.0.1:8094',
        changeOrigin: true,
        pathRewrite: {
          '^/api': '/api'
        }
      },
      '/ws': {
        target: 'http://127.0.0.1:8095',
        changeOrigin: true,
        ws: true,  // 启用 WebSocket 代理
        pathRewrite: {
          '^/ws': '/ws'
        }
      }
    }
  }
}
