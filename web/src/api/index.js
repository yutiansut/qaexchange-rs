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

/**
 * 获取账户权益曲线
 */
export function getEquityCurve(userId) {
  return request({
    url: `/account/${userId}/equity-curve`,
    method: 'get'
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
 * 查询用户持仓（跨所有账户）
 */
export function queryPosition(userId) {
  return request({
    url: `/position/user/${userId}`,
    method: 'get'
  })
}

/**
 * 查询账户持仓（单个账户）
 */
export function queryAccountPosition(accountId) {
  return request({
    url: `/position/account/${accountId}`,
    method: 'get'
  })
}

// ============= 成交记录查询 API =============

/**
 * 查询用户成交记录（跨所有账户）
 */
export function queryUserTrades(userId) {
  return request({
    url: `/trades/user/${userId}`,
    method: 'get'
  })
}

/**
 * 查询账户成交记录（单个账户）
 */
export function queryAccountTrades(accountId) {
  return request({
    url: `/trades/account/${accountId}`,
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

/**
 * 获取系统运行状态（运行时间、WebSocket连接数等）@yutiansut @quantaxis
 */
export function getSystemStatus() {
  return request({
    url: '/monitoring/status',
    method: 'get'
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

// ============= 管理端 - 全市场订单/成交查询 API @yutiansut @quantaxis =============

/**
 * 获取全市场所有订单（管理端）
 * @param {Object} params - 查询参数
 * @param {number} params.page - 页码
 * @param {number} params.page_size - 每页数量
 * @param {string} params.status - 状态过滤
 * @param {string} params.instrument_id - 合约过滤
 */
export function listAllOrders(params) {
  return request({
    url: '/management/orders',
    method: 'get',
    params
  })
}

/**
 * 获取全市场所有成交（管理端）
 * @param {Object} params - 查询参数
 * @param {number} params.page - 页码
 * @param {number} params.page_size - 每页数量
 * @param {string} params.instrument_id - 合约过滤
 */
export function listAllTrades(params) {
  return request({
    url: '/management/trades',
    method: 'get',
    params
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

/**
 * 获取所有资金流水（管理端，默认加载全部）
 * @yutiansut @quantaxis
 * @param {Object} params - { transaction_type, page, page_size }
 */
export function getAllTransactions(params) {
  return request({
    url: '/management/transactions',
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

/**
 * 触发强平
 */
export function forceLiquidateAccount(data) {
  return request({
    url: '/management/risk/force-liquidate',
    method: 'post',
    data
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

// ============= 银期转账 API @yutiansut @quantaxis =============

/**
 * 执行转账（入金/出金）
 * @param {Object} data - 转账信息
 * @param {string} data.account_id - 账户ID
 * @param {string} data.bank_id - 银行ID
 * @param {number} data.amount - 金额（正数入金，负数出金）
 * @param {string} data.bank_password - 银行密码
 * @param {string} data.future_password - 期货密码
 */
export function doTransfer(data) {
  return request({
    url: '/account/transfer',
    method: 'post',
    data
  })
}

/**
 * 获取账户签约银行列表
 */
export function getBanks(accountId) {
  return request({
    url: `/account/${accountId}/banks`,
    method: 'get'
  })
}

/**
 * 获取账户转账记录
 */
export function getTransferRecords(accountId) {
  return request({
    url: `/account/${accountId}/transfers`,
    method: 'get'
  })
}

// ============= 批量下单 API @yutiansut @quantaxis =============

/**
 * 批量提交订单
 * @param {Object} data - 批量下单请求
 * @param {string} data.account_id - 账户ID
 * @param {Array} data.orders - 订单列表
 * @param {string} data.orders[].instrument_id - 合约ID
 * @param {string} data.orders[].direction - 方向 BUY/SELL
 * @param {string} data.orders[].offset - 开平 OPEN/CLOSE/CLOSE_TODAY/CLOSE_YESTERDAY
 * @param {number} data.orders[].volume - 数量
 * @param {number} data.orders[].price - 价格
 * @param {string} data.orders[].order_type - 订单类型 LIMIT/MARKET
 */
export function batchSubmitOrders(data) {
  return request({
    url: '/order/batch',
    method: 'post',
    data
  })
}

/**
 * 批量撤单
 * @param {Object} data - 批量撤单请求
 * @param {string} data.account_id - 账户ID
 * @param {Array<string>} data.order_ids - 订单ID列表
 */
export function batchCancelOrders(data) {
  return request({
    url: '/order/batch-cancel',
    method: 'post',
    data
  })
}

// ============= 订单修改 API @yutiansut @quantaxis =============

/**
 * 修改订单
 * @param {string} orderId - 订单ID
 * @param {Object} data - 修改信息
 * @param {string} data.account_id - 账户ID
 * @param {number} data.new_price - 新价格（可选）
 * @param {number} data.new_volume - 新数量（可选）
 */
export function modifyOrder(orderId, data) {
  return request({
    url: `/order/modify/${orderId}`,
    method: 'put',
    data
  })
}

// ============= 条件单 API @yutiansut @quantaxis =============

/**
 * 创建条件单
 * @param {Object} data - 条件单信息
 * @param {string} data.account_id - 账户ID
 * @param {string} data.instrument_id - 合约ID
 * @param {string} data.direction - 方向 BUY/SELL
 * @param {string} data.offset - 开平 OPEN/CLOSE/CLOSE_TODAY/CLOSE_YESTERDAY
 * @param {number} data.volume - 数量
 * @param {string} data.order_type - 订单类型 LIMIT/MARKET
 * @param {number} data.limit_price - 限价（order_type为LIMIT时必填）
 * @param {string} data.condition_type - 条件类型 StopLoss/TakeProfit/PriceTouch
 * @param {number} data.trigger_price - 触发价
 * @param {string} data.trigger_condition - 触发条件 GreaterOrEqual/LessOrEqual
 * @param {number} data.valid_until - 有效期（时间戳，可选）
 */
export function createConditionalOrder(data) {
  return request({
    url: '/order/conditional',
    method: 'post',
    data
  })
}

/**
 * 获取条件单列表
 * @param {string} accountId - 账户ID
 */
export function getConditionalOrders(accountId) {
  return request({
    url: '/order/conditional/list',
    method: 'get',
    params: { account_id: accountId }
  })
}

/**
 * 取消条件单
 * @param {string} conditionalOrderId - 条件单ID
 */
export function cancelConditionalOrder(conditionalOrderId) {
  return request({
    url: `/order/conditional/${conditionalOrderId}`,
    method: 'delete'
  })
}

/**
 * 获取条件单统计
 */
export function getConditionalOrderStatistics() {
  return request({
    url: '/order/conditional/statistics',
    method: 'get'
  })
}

// ============= K线数据 API @yutiansut @quantaxis =============

/**
 * 获取K线数据
 * @param {string} instrumentId - 合约ID
 * @param {Object} params - 查询参数
 * @param {string} params.period - 周期（1m/5m/15m/30m/1h/4h/1d）
 * @param {number} params.limit - 数量（默认100）
 * @param {number} params.start_time - 开始时间戳（可选）
 * @param {number} params.end_time - 结束时间戳（可选）
 */
export function getKlineData(instrumentId, params = {}) {
  return request({
    url: `/market/kline/${instrumentId}`,
    method: 'get',
    params
  })
}

// ============= Phase 12: 密码管理 API @yutiansut @quantaxis =============

/**
 * 修改密码
 * @param {Object} data - 密码修改信息
 * @param {string} data.account_id - 账户ID
 * @param {string} data.old_password - 旧密码
 * @param {string} data.new_password - 新密码
 * @param {string} data.password_type - 密码类型 Trading/Fund
 */
export function changePassword(data) {
  return request({
    url: '/account-admin/password/change',
    method: 'post',
    data
  })
}

/**
 * 重置密码（管理员）
 * @param {Object} data - 密码重置信息
 * @param {string} data.admin_token - 管理员令牌
 * @param {string} data.account_id - 账户ID
 * @param {string} data.new_password - 新密码
 * @param {string} data.password_type - 密码类型 Trading/Fund
 */
export function resetPassword(data) {
  return request({
    url: '/account-admin/password/reset',
    method: 'post',
    data
  })
}

// ============= Phase 12: 手续费查询 API @yutiansut @quantaxis =============

/**
 * 查询手续费率
 * @param {Object} params - 查询参数
 * @param {string} params.product_id - 品种ID（可选）
 */
export function getCommissionRates(params = {}) {
  return request({
    url: '/account-admin/commission/rates',
    method: 'get',
    params
  })
}

/**
 * 查询账户手续费统计
 * @param {string} accountId - 账户ID
 */
export function getCommissionStatistics(accountId) {
  return request({
    url: `/account-admin/commission/statistics/${accountId}`,
    method: 'get'
  })
}

// ============= Phase 12: 保证金率管理 API @yutiansut @quantaxis =============

/**
 * 查询保证金率
 * @param {Object} params - 查询参数
 * @param {string} params.instrument_id - 合约ID（可选）
 */
export function getMarginRates(params = {}) {
  return request({
    url: '/account-admin/margin/rates',
    method: 'get',
    params
  })
}

/**
 * 查询账户保证金汇总
 * @param {string} accountId - 账户ID
 */
export function getAccountMarginSummary(accountId) {
  return request({
    url: `/account-admin/margin/summary/${accountId}`,
    method: 'get'
  })
}

// ============= Phase 13: 账户冻结 API @yutiansut @quantaxis =============

/**
 * 查询账户状态
 * @param {string} accountId - 账户ID
 */
export function getAccountStatus(accountId) {
  return request({
    url: `/account-admin/status/${accountId}`,
    method: 'get'
  })
}

/**
 * 冻结账户
 * @param {Object} data - 冻结信息
 * @param {string} data.admin_token - 管理员令牌
 * @param {string} data.account_id - 账户ID
 * @param {string} data.freeze_type - 冻结类型 TradingOnly/WithdrawOnly/Full
 * @param {string} data.reason - 冻结原因
 */
export function freezeAccount(data) {
  return request({
    url: '/account-admin/freeze',
    method: 'post',
    data
  })
}

/**
 * 解冻账户
 * @param {Object} data - 解冻信息
 * @param {string} data.admin_token - 管理员令牌
 * @param {string} data.account_id - 账户ID
 * @param {string} data.reason - 解冻原因
 */
export function unfreezeAccount(data) {
  return request({
    url: '/account-admin/unfreeze',
    method: 'post',
    data
  })
}

// ============= Phase 13: 审计日志 API @yutiansut @quantaxis =============

/**
 * 查询审计日志
 * @param {Object} params - 查询参数
 * @param {string} params.account_id - 账户ID（可选）
 * @param {string} params.log_type - 日志类型（可选）
 * @param {number} params.start_time - 开始时间戳（可选）
 * @param {number} params.end_time - 结束时间戳（可选）
 * @param {number} params.page - 页码
 * @param {number} params.page_size - 每页数量
 */
export function queryAuditLogs(params = {}) {
  return request({
    url: '/audit/logs',
    method: 'get',
    params
  })
}

/**
 * 获取单条审计日志
 * @param {string} logId - 日志ID
 */
export function getAuditLog(logId) {
  return request({
    url: `/audit/logs/${logId}`,
    method: 'get'
  })
}

// ============= Phase 13: 系统公告 API @yutiansut @quantaxis =============

/**
 * 查询公告列表
 * @param {Object} params - 查询参数
 * @param {string} params.announcement_type - 公告类型（可选）
 * @param {boolean} params.active_only - 仅有效公告（默认true）
 * @param {number} params.page - 页码
 * @param {number} params.page_size - 每页数量
 */
export function queryAnnouncements(params = {}) {
  return request({
    url: '/announcements',
    method: 'get',
    params
  })
}

/**
 * 获取单条公告
 * @param {string} announcementId - 公告ID
 */
export function getAnnouncement(announcementId) {
  return request({
    url: `/announcements/${announcementId}`,
    method: 'get'
  })
}

/**
 * 创建公告（管理员）
 * @param {Object} data - 公告信息
 * @param {string} data.admin_token - 管理员令牌
 * @param {string} data.title - 标题
 * @param {string} data.content - 内容
 * @param {string} data.announcement_type - 公告类型 System/Maintenance/Trading/Risk/Promotion
 * @param {string} data.priority - 优先级 Low/Normal/High/Urgent
 * @param {number} data.effective_from - 生效时间戳（可选）
 * @param {number} data.effective_until - 失效时间戳（可选）
 */
export function createAnnouncement(data) {
  return request({
    url: '/announcements',
    method: 'post',
    data
  })
}

/**
 * 删除公告（管理员）
 * @param {string} announcementId - 公告ID
 * @param {string} adminToken - 管理员令牌（可选，默认使用系统令牌）
 */
export function deleteAnnouncement(announcementId, adminToken = 'qaexchange_admin_2024') {
  return request({
    url: `/announcements/${announcementId}`,
    method: 'delete',
    params: { admin_token: adminToken }
  })
}
