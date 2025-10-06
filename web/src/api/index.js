import request from './request'

// ============= 用户认证 API =============

/**
 * 用户注册
 */
export function register(data) {
  return request({
    url: '/auth/register',
    method: 'post',
    data
  })
}

/**
 * 用户登录
 */
export function login(data) {
  return request({
    url: '/auth/login',
    method: 'post',
    data
  })
}

/**
 * 获取当前用户信息
 */
export function getCurrentUser(userId) {
  return request({
    url: `/auth/user/${userId}`,
    method: 'get'
  })
}

/**
 * 获取所有用户列表（管理员功能）
 */
export function listUsers() {
  return request({
    url: '/auth/users',
    method: 'get'
  })
}

// ============= 用户账户管理 API =============

/**
 * 为用户创建交易账户
 */
export function createUserAccount(userId, data) {
  return request({
    url: `/user/${userId}/account/create`,
    method: 'post',
    data
  })
}

/**
 * 获取用户的所有交易账户
 */
export function getUserAccounts(userId) {
  return request({
    url: `/user/${userId}/accounts`,
    method: 'get'
  })
}

// ============= 账户管理 API =============

/**
 * 开户
 */
export function openAccount(data) {
  return request({
    url: '/account/open',
    method: 'post',
    data
  })
}

/**
 * 查询账户
 */
export function queryAccount(userId) {
  return request({
    url: `/account/${userId}`,
    method: 'get'
  })
}

/**
 * 存款
 */
export function deposit(data) {
  return request({
    url: '/account/deposit',
    method: 'post',
    data
  })
}

/**
 * 取款
 */
export function withdraw(data) {
  return request({
    url: '/account/withdraw',
    method: 'post',
    data
  })
}

// ============= 订单管理 API =============

/**
 * 提交订单
 */
export function submitOrder(data) {
  return request({
    url: '/order/submit',
    method: 'post',
    data
  })
}

/**
 * 撤单
 */
export function cancelOrder(data) {
  return request({
    url: '/order/cancel',
    method: 'post',
    data
  })
}

/**
 * 查询订单
 */
export function queryOrder(orderId) {
  return request({
    url: `/order/${orderId}`,
    method: 'get'
  })
}

/**
 * 查询用户订单
 */
export function queryUserOrders(userId) {
  return request({
    url: `/order/user/${userId}`,
    method: 'get'
  })
}

// ============= 持仓查询 API =============

/**
 * 查询持仓
 */
export function queryPosition(userId) {
  return request({
    url: `/position/${userId}`,
    method: 'get'
  })
}

// ============= 成交记录查询 API =============

/**
 * 查询用户成交记录
 */
export function queryUserTrades(userId) {
  return request({
    url: `/trades/user/${userId}`,
    method: 'get'
  })
}

// ============= 监控 API =============

/**
 * 系统监控（全部）
 */
export function getSystemMonitoring() {
  return request({
    url: '/monitoring/system',
    method: 'get'
  })
}

/**
 * 账户统计
 */
export function getAccountsMonitoring() {
  return request({
    url: '/monitoring/accounts',
    method: 'get'
  })
}

/**
 * 订单统计
 */
export function getOrdersMonitoring() {
  return request({
    url: '/monitoring/orders',
    method: 'get'
  })
}

/**
 * 成交统计
 */
export function getTradesMonitoring() {
  return request({
    url: '/monitoring/trades',
    method: 'get'
  })
}

/**
 * 存储统计
 */
export function getStorageMonitoring() {
  return request({
    url: '/monitoring/storage',
    method: 'get'
  })
}

/**
 * 生成报告
 */
export function generateReport() {
  return request({
    url: '/monitoring/report',
    method: 'get'
  })
}

/**
 * 健康检查
 */
export function healthCheck() {
  return request({
    url: '/health',
    method: 'get',
    baseURL: '/'
  })
}

// ============= 市场数据 API =============

/**
 * 获取合约列表
 */
export function getInstruments() {
  return request({
    url: '/market/instruments',
    method: 'get'
  })
}

/**
 * 获取订单簿
 */
export function getOrderBook(instrumentId, depth = 5) {
  return request({
    url: `/market/orderbook/${instrumentId}`,
    method: 'get',
    params: { depth }
  })
}

/**
 * 获取 Tick 行情
 */
export function getTick(instrumentId) {
  return request({
    url: `/market/tick/${instrumentId}`,
    method: 'get'
  })
}

/**
 * 获取最近成交
 */
export function getRecentTrades(instrumentId, limit = 20) {
  return request({
    url: `/market/trades/${instrumentId}`,
    method: 'get',
    params: { limit }
  })
}

/**
 * 获取市场订单统计（管理员）
 */
export function getMarketOrderStats() {
  return request({
    url: '/admin/market/order-stats',
    method: 'get'
  })
}

// ============= 管理端 - 账户管理 API =============

/**
 * 获取所有账户列表（管理端）
 */
export function listAllAccounts(params) {
  return request({
    url: '/management/accounts',
    method: 'get',
    params
  })
}

/**
 * 获取账户详情（管理端）
 */
export function getAccountDetail(userId) {
  return request({
    url: `/management/account/${userId}/detail`,
    method: 'get'
  })
}

// ============= 管理端 - 资金管理 API =============

/**
 * 入金（管理端）
 */
export function managementDeposit(data) {
  return request({
    url: '/management/deposit',
    method: 'post',
    data
  })
}

/**
 * 出金（管理端）
 */
export function managementWithdraw(data) {
  return request({
    url: '/management/withdraw',
    method: 'post',
    data
  })
}

/**
 * 获取资金流水（管理端）
 */
export function getTransactions(userId, params) {
  return request({
    url: `/management/transactions/${userId}`,
    method: 'get',
    params
  })
}

// ============= 管理端 - 风控监控 API =============

/**
 * 获取风险账户列表
 */
export function getRiskAccounts(params) {
  return request({
    url: '/management/risk/accounts',
    method: 'get',
    params
  })
}

/**
 * 获取保证金监控汇总
 */
export function getMarginSummary() {
  return request({
    url: '/management/risk/margin-summary',
    method: 'get'
  })
}

/**
 * 获取强平记录
 */
export function getLiquidationRecords(params) {
  return request({
    url: '/management/risk/liquidations',
    method: 'get',
    params
  })
}

// ============= 管理端 - 合约管理 API =============

/**
 * 获取所有合约列表
 */
export function getAllInstruments() {
  return request({
    url: '/admin/instruments',
    method: 'get'
  })
}

/**
 * 创建新合约
 */
export function createInstrument(data) {
  return request({
    url: '/admin/instrument/create',
    method: 'post',
    data
  })
}

/**
 * 更新合约信息
 */
export function updateInstrument(instrumentId, data) {
  return request({
    url: `/admin/instrument/${instrumentId}/update`,
    method: 'put',
    data
  })
}

/**
 * 暂停合约交易
 */
export function suspendInstrument(instrumentId) {
  return request({
    url: `/admin/instrument/${instrumentId}/suspend`,
    method: 'put'
  })
}

/**
 * 恢复合约交易
 */
export function resumeInstrument(instrumentId) {
  return request({
    url: `/admin/instrument/${instrumentId}/resume`,
    method: 'put'
  })
}

/**
 * 下市合约
 */
export function delistInstrument(instrumentId) {
  return request({
    url: `/admin/instrument/${instrumentId}/delist`,
    method: 'delete'
  })
}

// ============= 管理端 - 结算管理 API =============

/**
 * 设置结算价
 */
export function setSettlementPrice(data) {
  return request({
    url: '/admin/settlement/set-price',
    method: 'post',
    data
  })
}

/**
 * 批量设置结算价
 */
export function batchSetSettlementPrices(data) {
  return request({
    url: '/admin/settlement/batch-set-prices',
    method: 'post',
    data
  })
}

/**
 * 执行日终结算
 */
export function executeSettlement() {
  return request({
    url: '/admin/settlement/execute',
    method: 'post'
  })
}

/**
 * 获取结算历史
 */
export function getSettlementHistory(params) {
  return request({
    url: '/admin/settlement/history',
    method: 'get',
    params
  })
}

/**
 * 获取结算详情
 */
export function getSettlementDetail(date) {
  return request({
    url: `/admin/settlement/detail/${date}`,
    method: 'get'
  })
}
