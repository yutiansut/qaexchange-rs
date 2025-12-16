/**
 * WebSocket Vuex 模块
 *
 * 管理 WebSocket 连接、业务截面和实时数据
 * @yutiansut @quantaxis
 */

import WebSocketManager from '@/websocket'
import request from '@/api/request'

const state = {
  // WebSocket 管理器实例
  ws: null,

  // 连接状态
  connectionState: 'DISCONNECTED',

  // ✨ 防止并发初始化的标志 @yutiansut @quantaxis
  isInitializing: false,

  // 业务截面（完整的实时数据）
  snapshot: {},

  // ✨ Phase 10: 账户管理
  currentAccountId: null,  // 当前选中的账户ID
  userAccounts: [],        // 用户的所有账户列表

  // 配置
  config: {
    url: process.env.VUE_APP_WS_URL || 'ws://localhost:8095/ws/diff',
    autoConnect: true,
    autoReconnect: true,
    reconnectInterval: 3000,
    reconnectMaxAttempts: 10,
    heartbeatInterval: 5000,
    heartbeatTimeout: 10000,
    logLevel: process.env.NODE_ENV === 'development' ? 'DEBUG' : 'INFO'
  },

  // 订阅的合约列表
  subscribedInstruments: [],

  // 事件监听器取消函数
  unsubscribers: []
}

const mutations = {
  SET_WS(state, ws) {
    state.ws = ws
  },

  SET_CONNECTION_STATE(state, connectionState) {
    state.connectionState = connectionState
  },

  SET_SNAPSHOT(state, snapshot) {
    state.snapshot = snapshot
  },

  UPDATE_SNAPSHOT(state, updates) {
    state.snapshot = { ...state.snapshot, ...updates }
  },

  // ✨ Phase 10: 账户管理 mutations
  SET_CURRENT_ACCOUNT(state, accountId) {
    state.currentAccountId = accountId
    console.log('[WebSocket] Current account set to:', accountId)
  },

  SET_USER_ACCOUNTS(state, accounts) {
    state.userAccounts = accounts
    console.log('[WebSocket] User accounts loaded:', accounts.length)
  },

  SET_SUBSCRIBED_INSTRUMENTS(state, instruments) {
    state.subscribedInstruments = instruments
  },

  ADD_UNSUBSCRIBER(state, unsubscriber) {
    state.unsubscribers.push(unsubscriber)
  },

  CLEAR_UNSUBSCRIBERS(state) {
    state.unsubscribers.forEach(unsubscribe => unsubscribe())
    state.unsubscribers = []
  },

  RESET_STATE(state) {
    state.ws = null
    state.connectionState = 'DISCONNECTED'
    state.isInitializing = false         // ✨ 重置初始化标志 @yutiansut @quantaxis
    state.snapshot = {}
    state.currentAccountId = null        // ✨ Phase 10
    state.userAccounts = []              // ✨ Phase 10
    state.subscribedInstruments = []
    state.unsubscribers.forEach(unsubscribe => unsubscribe())
    state.unsubscribers = []
  },

  // ✨ 设置初始化标志 @yutiansut @quantaxis
  SET_INITIALIZING(state, value) {
    state.isInitializing = value
  }
}

const actions = {
  /**
   * 初始化 WebSocket
   * ✨ 增加 isInitializing 标志防止并发初始化 @yutiansut @quantaxis
   */
  async initWebSocket({ state, commit, rootState, dispatch }) {
    console.log('[WebSocket] Initializing...')

    // ✨ 防止并发初始化
    if (state.isInitializing) {
      console.warn('[WebSocket] Already initializing, skipping duplicate call')
      return state.ws
    }

    // ✅ 防止重复创建：如果已有连接，先销毁
    if (state.ws) {
      console.warn('[WebSocket] Already initialized, destroying existing instance...')
      await dispatch('destroyWebSocket')
    }

    // ✨ 设置初始化标志
    commit('SET_INITIALIZING', true)

    // 获取当前用户 ID @yutiansut @quantaxis
    // 优先使用 userInfo.user_id (UUID)，避免使用 username
    const userId = (rootState.userInfo && rootState.userInfo.user_id) || rootState.currentUser
    if (!userId) {
      console.error('[WebSocket] No user ID available')
      commit('SET_INITIALIZING', false)  // ✨ 清除标志
      throw new Error('No user ID available')
    }

    // ✨ Phase 10: 获取用户账户列表
    try {
      await dispatch('fetchUserAccounts', userId)
    } catch (error) {
      console.error('[WebSocket] Failed to fetch user accounts:', error)
      // 继续初始化，即使获取账户失败
    }

    // 创建 WebSocket 管理器
    const ws = new WebSocketManager({
      ...state.config,
      userId
    })

    // 监听连接成功事件
    const onOpen = () => {
      console.log('[WebSocket] Connected')
      dispatch('onWebSocketOpen')
    }

    // 监听连接关闭事件
    const onClose = (event) => {
      console.log('[WebSocket] Closed', event.code, event.reason)
      dispatch('onWebSocketClose', event)
    }

    // 监听错误事件
    const onError = (error) => {
      console.error('[WebSocket] Error', error)
      dispatch('onWebSocketError', error)
    }

    // 监听消息事件
    const onMessage = (message) => {
      dispatch('onWebSocketMessage', message)
    }

    // 监听状态变化事件
    const onStateChange = ({ oldState, newState }) => {
      console.log(`[WebSocket] State changed: ${oldState} -> ${newState}`)
      commit('SET_CONNECTION_STATE', newState)
      dispatch('onWebSocketStateChange', { oldState, newState })
    }

    // 注册事件监听器
    commit('ADD_UNSUBSCRIBER', ws.on('open', onOpen))
    commit('ADD_UNSUBSCRIBER', ws.on('close', onClose))
    commit('ADD_UNSUBSCRIBER', ws.on('error', onError))
    commit('ADD_UNSUBSCRIBER', ws.on('message', onMessage))
    commit('ADD_UNSUBSCRIBER', ws.on('stateChange', onStateChange))

    // 保存 WebSocket 实例
    commit('SET_WS', ws)

    // ✨ 清除初始化标志 @yutiansut @quantaxis
    commit('SET_INITIALIZING', false)

    console.log('[WebSocket] Initialized')
    return ws
  },

  /**
   * 连接 WebSocket
   * ✨ 增加 CONNECTING 状态检查，防止重复连接 @yutiansut @quantaxis
   */
  async connectWebSocket({ state, dispatch }) {
    console.log('[WebSocket] connectWebSocket called, current state:', {
      hasWs: !!state.ws,
      connectionState: state.connectionState
    })

    // ✨ 如果正在连接或已连接，直接返回
    if (state.connectionState === 'CONNECTED') {
      console.log('[WebSocket] Already connected, skipping connection')
      return
    }

    if (state.connectionState === 'CONNECTING') {
      console.log('[WebSocket] Connection in progress, skipping duplicate connect call')
      return
    }

    if (!state.ws) {
      console.log('[WebSocket] No ws instance, initializing...')
      await dispatch('initWebSocket')
    }

    if (state.ws && state.connectionState !== 'CONNECTED') {
      console.log('[WebSocket] Connecting... (state:', state.connectionState, ')')
      try {
        await state.ws.connect()
        console.log('[WebSocket] Connected successfully')
      } catch (error) {
        console.error('[WebSocket] Connection failed:', error)
        throw error
      }
    }
  },

  /**
   * 断开 WebSocket
   */
  disconnectWebSocket({ state, commit }) {
    if (state.ws) {
      console.log('[WebSocket] Disconnecting...')
      state.ws.disconnect()
      commit('SET_CONNECTION_STATE', 'DISCONNECTED')
    }
  },

  /**
   * 销毁 WebSocket
   */
  destroyWebSocket({ state, commit }) {
    console.log('[WebSocket] Destroying...')

    // 清除所有事件监听器
    commit('CLEAR_UNSUBSCRIBERS')

    // 销毁 WebSocket 实例
    if (state.ws) {
      state.ws.destroy()
    }

    // 重置状态
    commit('RESET_STATE')

    console.log('[WebSocket] Destroyed')
  },

  /**
   * 订阅行情
   */
  subscribeQuote({ state, commit }, instruments) {
    if (!state.ws || state.connectionState !== 'CONNECTED') {
      console.warn('[WebSocket] Not connected, cannot subscribe')
      return
    }

    const instrumentList = Array.isArray(instruments) ? instruments : [instruments]
    console.log('[WebSocket] Subscribing to quotes:', instrumentList)

    state.ws.subscribeQuote(instrumentList)
    commit('SET_SUBSCRIBED_INSTRUMENTS', instrumentList)
  },

  /**
   * 下单
   */
  insertOrder({ state, rootState }, order) {
    if (!state.ws || state.connectionState !== 'CONNECTED') {
      console.warn('[WebSocket] Not connected, cannot insert order')
      throw new Error('WebSocket not connected')
    }

    // ✨ Phase 10: 自动填充 user_id 和 account_id @yutiansut @quantaxis
    // 优先使用 userInfo.user_id (UUID)，避免使用 username
    const orderWithMeta = {
      user_id: (rootState.userInfo && rootState.userInfo.user_id) || rootState.currentUser,
      account_id: state.currentAccountId,  // ✨ 明确传递账户ID
      order_id: `order_${Date.now()}`,
      ...order
    }

    // 验证 account_id
    if (!orderWithMeta.account_id) {
      console.warn('[WebSocket] No account selected, order may fail')
      // 可以选择抛出错误或继续（向后兼容模式）
    }

    console.log('[WebSocket] Inserting order:', orderWithMeta)
    state.ws.insertOrder(orderWithMeta)

    return orderWithMeta.order_id
  },

  /**
   * 撤单
   */
  cancelOrder({ state, rootState }, orderId) {
    if (!state.ws || state.connectionState !== 'CONNECTED') {
      console.warn('[WebSocket] Not connected, cannot cancel order')
      throw new Error('WebSocket not connected')
    }

    // 优先使用 userInfo.user_id (UUID)，避免使用 username @yutiansut @quantaxis
    const userId = (rootState.userInfo && rootState.userInfo.user_id) || rootState.currentUser
    const accountId = state.currentAccountId  // ✨ Phase 10: 传递账户ID

    // 验证 account_id
    if (!accountId) {
      console.warn('[WebSocket] No account selected, cancel may fail')
    }

    console.log('[WebSocket] Cancelling order:', orderId, 'account:', accountId)
    state.ws.cancelOrder(userId, orderId, accountId)  // ✨ Phase 10: 传递第三个参数
  },

  /**
   * 订阅K线图表数据
   *
   * @param {Object} chart - 图表配置
   * @param {string} chart.chart_id - 图表ID
   * @param {string} chart.instrument_id - 合约代码
   * @param {number} chart.period - K线周期 (0=日线, 4=1分钟, 5=5分钟等)
   * @param {number} [chart.count=500] - 数据条数
   */
  setChart({ state }, chart) {
    if (!state.ws || state.connectionState !== 'CONNECTED') {
      console.warn('[WebSocket] Not connected, cannot set chart')
      return
    }

    // 转换周期为纳秒（DIFF协议要求）
    const periodToNs = (period) => {
      switch (period) {
        case 0: return 86400000000000  // 日线
        case 3: return 3000000000      // 3秒
        case 4: return 60000000000     // 1分钟
        case 5: return 300000000000    // 5分钟
        case 6: return 900000000000    // 15分钟
        case 7: return 1800000000000   // 30分钟
        case 8: return 3600000000000   // 60分钟
        default: return 300000000000   // 默认5分钟
      }
    }

    const chartConfig = {
      chart_id: chart.chart_id || `chart_${Date.now()}`,
      ins_list: chart.instrument_id || chart.ins_list,
      duration: periodToNs(chart.period || 5),
      view_width: chart.count || 500
    }

    console.log('[WebSocket] Setting chart:', chartConfig)
    state.ws.setChart(chartConfig)
  },

  /**
   * WebSocket 连接成功处理
   */
  onWebSocketOpen({ commit, state }) {
    // 自动订阅默认合约（可选）
    const defaultInstruments = process.env.VUE_APP_DEFAULT_INSTRUMENTS
    if (defaultInstruments) {
      const instruments = defaultInstruments.split(',').map(s => s.trim())
      if (instruments.length > 0) {
        console.log('[WebSocket] Auto-subscribing to default instruments:', instruments)
        state.ws.subscribeQuote(instruments)
        commit('SET_SUBSCRIBED_INSTRUMENTS', instruments)
      }
    }
  },

  /**
   * WebSocket 连接关闭处理
   */
  onWebSocketClose({ commit }, event) {
    if (!event.wasClean) {
      console.warn('[WebSocket] Connection closed unexpectedly')
      // 可以在这里触发 UI 提示
    }
  },

  /**
   * WebSocket 错误处理
   */
  onWebSocketError({ commit }, error) {
    console.error('[WebSocket] Error occurred:', error)
    // 可以在这里触发 UI 提示
  },

  /**
   * WebSocket 消息处理
   */
  onWebSocketMessage({ state, commit }, message) {
    // 更新业务截面
    if (state.ws) {
      const snapshot = state.ws.getSnapshot()
      commit('SET_SNAPSHOT', snapshot)
    }
  },

  /**
   * WebSocket 状态变化处理
   */
  onWebSocketStateChange({ commit }, { oldState, newState }) {
    // 可以在这里根据状态变化触发不同的操作
    if (newState === 'CONNECTED') {
      console.log('[WebSocket] Connection established')
    } else if (newState === 'RECONNECTING') {
      console.log('[WebSocket] Reconnecting...')
    } else if (newState === 'DISCONNECTED') {
      console.log('[WebSocket] Disconnected')
    }
  },

  // ✨ Phase 10: 账户管理 actions

  /**
   * 获取用户账户列表 @yutiansut @quantaxis
   * 使用 axios request 实例，通过代理访问 API
   */
  async fetchUserAccounts({ commit, state }, userId) {
    try {
      console.log('[WebSocket] Fetching accounts for user:', userId)

      // 使用 axios request 实例，URL 会通过 vue.config.js 代理
      const result = await request({
        url: `/user/${userId}/accounts`,
        method: 'get'
      })

      // request 拦截器已处理 success 检查，直接返回 data
      const accounts = result.accounts || []
      commit('SET_USER_ACCOUNTS', accounts)

      // 自动选择第一个账户（如果有）
      if (accounts.length > 0 && !state.currentAccountId) {
        commit('SET_CURRENT_ACCOUNT', accounts[0].account_id)
      }

      return accounts
    } catch (error) {
      console.error('[WebSocket] Error fetching accounts:', error)
      throw error
    }
  },

  /**
   * 切换当前账户
   */
  switchAccount({ commit, state }, accountId) {
    // 验证账户ID是否在账户列表中
    const account = state.userAccounts.find(acc => acc.account_id === accountId)
    if (!account) {
      console.error('[WebSocket] Invalid account ID:', accountId)
      throw new Error(`Account not found: ${accountId}`)
    }

    console.log('[WebSocket] Switching to account:', accountId)
    commit('SET_CURRENT_ACCOUNT', accountId)
  }
}

const getters = {
  // WebSocket 实例
  ws: state => state.ws,

  // 连接状态
  connectionState: state => state.connectionState,

  // 是否已连接
  isConnected: state => state.connectionState === 'CONNECTED',

  // 业务截面
  snapshot: state => state.snapshot,

  // ✨ Phase 10: 账户管理 getters
  currentAccountId: state => state.currentAccountId,

  // ✨ 修复：合并 userAccounts 和 snapshot.accounts @yutiansut @quantaxis
  userAccounts: state => {
    // 如果有 HTTP API 获取的账户列表，直接返回
    if (state.userAccounts && state.userAccounts.length > 0) {
      return state.userAccounts
    }
    // 否则从 snapshot.accounts 构建账户列表
    if (state.snapshot.accounts) {
      return Object.entries(state.snapshot.accounts).map(([currency, acc]) => ({
        account_id: acc.user_id || state.snapshot.user_id || 'default',
        account_name: `${acc.user_id || '交易账户'} (${currency})`,
        currency: currency,
        balance: acc.balance || 0,
        available: acc.available || 0,
        margin: acc.margin || 0,
        risk_ratio: acc.risk_ratio || 0,
        ...acc
      }))
    }
    return []
  },

  currentAccount: state => {
    // 先从 userAccounts 查找
    if (state.currentAccountId && state.userAccounts && state.userAccounts.length > 0) {
      const found = state.userAccounts.find(acc => acc.account_id === state.currentAccountId)
      if (found) return found
    }
    // 否则从 snapshot.accounts 获取（默认 CNY）
    if (state.snapshot.accounts) {
      const cnyAccount = state.snapshot.accounts['CNY']
      if (cnyAccount) {
        return {
          account_id: cnyAccount.user_id || state.snapshot.user_id || 'default',
          account_name: `${cnyAccount.user_id || '交易账户'} (CNY)`,
          currency: 'CNY',
          balance: cnyAccount.balance || 0,
          available: cnyAccount.available || 0,
          margin: cnyAccount.margin || 0,
          risk_ratio: cnyAccount.risk_ratio || 0,
          ...cnyAccount
        }
      }
    }
    return null
  },

  // ✨ 修复：同时检查 userAccounts 和 snapshot.accounts @yutiansut @quantaxis
  // userAccounts 来自 HTTP API，snapshot.accounts 来自 WebSocket QIFI
  hasAccounts: state => {
    // 优先检查 userAccounts（HTTP API 获取的账户列表）
    if (state.userAccounts && state.userAccounts.length > 0) {
      return true
    }
    // 其次检查 snapshot.accounts（WebSocket QIFI 数据）
    if (state.snapshot.accounts && Object.keys(state.snapshot.accounts).length > 0) {
      return true
    }
    return false
  },

  // 账户信息
  account: state => (currency = 'CNY') => {
    return state.snapshot.accounts && state.snapshot.accounts[currency]
  },

  // 所有持仓
  positions: state => state.snapshot.positions || {},

  // 获取特定持仓
  position: state => (instrumentId) => {
    return state.snapshot.positions && state.snapshot.positions[instrumentId]
  },

  // 所有订单
  orders: state => state.snapshot.orders || {},

  // 获取特定订单
  order: state => (orderId) => {
    return state.snapshot.orders && state.snapshot.orders[orderId]
  },

  // 活跃订单（未完成的订单）
  activeOrders: state => {
    const orders = state.snapshot.orders || {}
    return Object.values(orders).filter(order =>
      order.status !== 'FILLED' &&
      order.status !== 'CANCELLED' &&
      order.status !== 'REJECTED'
    )
  },

  // 所有成交记录
  trades: state => state.snapshot.trades || {},

  // 所有行情
  quotes: state => state.snapshot.quotes || {},

  // 获取特定行情
  quote: state => (instrumentId) => {
    return state.snapshot.quotes && state.snapshot.quotes[instrumentId]
  },

  // 订阅的合约列表
  subscribedInstruments: state => state.subscribedInstruments,

  // 通知信息
  notifications: state => state.snapshot.notify || {},

  // ============================================================================
  // ✨ 因子数据 Getters @yutiansut @quantaxis
  // ============================================================================

  // 所有因子数据
  factors: state => state.snapshot.factors || {},

  // 获取特定合约的因子数据
  factor: state => (instrumentId) => {
    return state.snapshot.factors && state.snapshot.factors[instrumentId]
  },

  // 获取特定因子值
  factorValue: state => (instrumentId, factorId) => {
    const factors = state.snapshot.factors && state.snapshot.factors[instrumentId]
    if (factors && factors.values) {
      return factors.values[factorId]
    }
    return null
  },

  // 获取合约的所有因子值（便捷访问）
  factorValues: state => (instrumentId) => {
    const factors = state.snapshot.factors && state.snapshot.factors[instrumentId]
    return (factors && factors.values) || {}
  },

  // 获取因子更新时间戳
  factorTimestamp: state => (instrumentId) => {
    const factors = state.snapshot.factors && state.snapshot.factors[instrumentId]
    return factors && factors.timestamp
  },

  // 检查是否有因子数据
  hasFactors: state => {
    return state.snapshot.factors && Object.keys(state.snapshot.factors).length > 0
  }
}

export default {
  namespaced: true,
  state,
  mutations,
  actions,
  getters
}
