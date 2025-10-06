/**
 * WebSocket Vuex 模块
 *
 * 管理 WebSocket 连接、业务截面和实时数据
 */

import WebSocketManager from '@/websocket'

const state = {
  // WebSocket 管理器实例
  ws: null,

  // 连接状态
  connectionState: 'DISCONNECTED',

  // 业务截面（完整的实时数据）
  snapshot: {},

  // ✨ Phase 10: 账户管理
  currentAccountId: null,  // 当前选中的账户ID
  userAccounts: [],        // 用户的所有账户列表

  // 配置
  config: {
    url: process.env.VUE_APP_WS_URL || 'ws://localhost:8001/ws',
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
    state.snapshot = {}
    state.currentAccountId = null        // ✨ Phase 10
    state.userAccounts = []              // ✨ Phase 10
    state.subscribedInstruments = []
    state.unsubscribers.forEach(unsubscribe => unsubscribe())
    state.unsubscribers = []
  }
}

const actions = {
  /**
   * 初始化 WebSocket
   */
  async initWebSocket({ commit, rootState, dispatch }) {
    console.log('[WebSocket] Initializing...')

    // 获取当前用户 ID
    const userId = rootState.currentUser || (rootState.userInfo && rootState.userInfo.user_id)
    if (!userId) {
      console.error('[WebSocket] No user ID available')
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

    console.log('[WebSocket] Initialized')
    return ws
  },

  /**
   * 连接 WebSocket
   */
  async connectWebSocket({ state, dispatch }) {
    if (!state.ws) {
      await dispatch('initWebSocket')
    }

    if (state.ws && state.connectionState !== 'CONNECTED') {
      console.log('[WebSocket] Connecting...')
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

    // ✨ Phase 10: 自动填充 user_id 和 account_id
    const orderWithMeta = {
      user_id: rootState.currentUser || (rootState.userInfo && rootState.userInfo.user_id),
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

    const userId = rootState.currentUser || (rootState.userInfo && rootState.userInfo.user_id)
    const accountId = state.currentAccountId  // ✨ Phase 10: 传递账户ID

    // 验证 account_id
    if (!accountId) {
      console.warn('[WebSocket] No account selected, cancel may fail')
    }

    console.log('[WebSocket] Cancelling order:', orderId, 'account:', accountId)
    state.ws.cancelOrder(userId, orderId, accountId)  // ✨ Phase 10: 传递第三个参数
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
   * 获取用户账户列表
   */
  async fetchUserAccounts({ commit }, userId) {
    try {
      console.log('[WebSocket] Fetching accounts for user:', userId)

      // 调用 HTTP API 获取账户列表
      const apiUrl = process.env.VUE_APP_API_URL || 'http://localhost:8001'
      const response = await fetch(`${apiUrl}/api/user/${userId}/accounts`)

      if (!response.ok) {
        throw new Error(`HTTP ${response.status}: ${response.statusText}`)
      }

      const result = await response.json()

      if (result.success && result.data) {
        const accounts = result.data.accounts || []
        commit('SET_USER_ACCOUNTS', accounts)

        // 自动选择第一个账户（如果有）
        if (accounts.length > 0 && !state.currentAccountId) {
          commit('SET_CURRENT_ACCOUNT', accounts[0].account_id)
        }

        return accounts
      } else {
        throw new Error(result.error || 'Failed to fetch accounts')
      }
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

  userAccounts: state => state.userAccounts,

  currentAccount: state => {
    if (!state.currentAccountId) return null
    return state.userAccounts.find(acc => acc.account_id === state.currentAccountId)
  },

  hasAccounts: state => state.userAccounts.length > 0,

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
  notifications: state => state.snapshot.notify || {}
}

export default {
  namespaced: true,
  state,
  mutations,
  actions,
  getters
}
