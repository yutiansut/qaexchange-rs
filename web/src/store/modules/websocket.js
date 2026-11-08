/**
 * WebSocket Vuex 模块
 *
 * 管理 WebSocket 连接、业务截面和实时数据
 * @yutiansut @quantaxis
 */

import WebSocketManager from '@/websocket'
import { Message } from 'element-ui'   // 与 api/request.js 同一写法
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
  unsubscribers: [],

  // ✨ 公告通知 @yutiansut @quantaxis
  processedNotifications: new Set(),  // 已处理的通知ID集合（防止重复处理）
  lastAnnouncement: null              // 最新收到的公告（用于顶部栏刷新）
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

  // ✨ 公告通知 mutations @yutiansut @quantaxis
  ADD_PROCESSED_NOTIFICATION(state, notificationKey) {
    state.processedNotifications.add(notificationKey)
  },

  SET_LAST_ANNOUNCEMENT(state, announcement) {
    state.lastAnnouncement = announcement
  },

  CLEAR_PROCESSED_NOTIFICATIONS(state) {
    state.processedNotifications.clear()
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

    // ⚠️ DIFF 推送的订阅键是 **account_id** 而不是 user_id。
    //    实测:同一端点 `ws://.../ws/diff?user_id=<X>`
    //      X = 用户 UUID   → patch 只有 quotes / notify / ins_list
    //      X = ACC_xxxxxx  → patch 还有 trades / orders / accounts
    //    用 UUID 连接会导致成交、委托、账户三类推送全部收不到,
    //    页面只能靠 HTTP 轮询兜底(纯 WS 的成交/委托区域恒空)。
    //    fetchUserAccounts 已在上一步执行,此处 currentAccountId 可用。
    //    @yutiansut @quantaxis
    const wsKey = state.currentAccountId || userId
    if (wsKey !== userId) {
      console.log('[WebSocket] 使用 account_id 作为订阅键:', wsKey)
    } else {
      // ⚠️ 这条回退**不是理论风险,是可达路径** —— 上方 `fetchUserAccounts` 失败时
      // 被 catch 掉并「继续初始化」(:158),此时 currentAccountId 仍为 null。
      // 2026-09-06 后端实测(订阅期间下真单,对比两种订阅键收到的 patch 顶层键):
      //   user_id(UUID)订阅 → ['factors','ins_list','klines','notify','quotes'] —— **无业务推送**
      //   account_id 订阅   → 上述 + ['accounts','orders','positions','trades']
      // 即回退后成交/委托/账户三类**全部收不到**,页面只能靠 HTTP 轮询兜底,
      // 而纯 WS 驱动的区域会恒空 —— 用户看到的是「行情在动,但我的委托不更新」。
      // 光打 console.warn 用户看不见,必须给一条可见提示。@yutiansut @quantaxis
      console.warn('[WebSocket] 无 currentAccountId,回退 user_id —— 将收不到成交/委托/账户推送')
      Message({
        type: 'warning',
        duration: 6000,
        message: '账户信息未就绪,实时委托/成交推送暂不可用(行情不受影响)。请刷新页面重试。'
      })
    }

    // 创建 WebSocket 管理器
    const ws = new WebSocketManager({
      ...state.config,
      userId: wsKey
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
   * @yutiansut @quantaxis
   */
  onWebSocketMessage({ state, commit, dispatch }, message) {
    // 更新业务截面
    if (state.ws) {
      const snapshot = state.ws.getSnapshot()
      commit('SET_SNAPSHOT', snapshot)

      // ✨ 处理公告通知 @yutiansut @quantaxis
      if (snapshot && snapshot.notify) {
        dispatch('handleNotifications', snapshot.notify)
      }
    }
  },

  /**
   * 处理通知消息（包括公告）
   * @yutiansut @quantaxis
   */
  handleNotifications({ state, commit }, notifications) {
    if (!notifications || typeof notifications !== 'object') return

    // 遍历所有通知
    Object.entries(notifications).forEach(([key, notify]) => {
      // 跳过已处理的通知
      if (state.processedNotifications && state.processedNotifications.has(key)) {
        return
      }

      // 处理公告通知
      if (notify.type === 'ANNOUNCEMENT') {
        console.log('[WebSocket] New announcement received:', notify.title)

        // 使用 Element UI 的 Notification 组件显示
        const { Notification } = require('element-ui')

        // 根据优先级选择类型
        const typeMap = {
          'ERROR': 'error',
          'WARNING': 'warning',
          'INFO': 'info'
        }
        const type = typeMap[notify.level] || 'info'

        Notification({
          title: `📢 ${notify.title}`,
          message: notify.content.length > 100
            ? notify.content.substring(0, 100) + '...'
            : notify.content,
          type: type,
          duration: notify.level === 'ERROR' ? 0 : 10000, // 紧急公告不自动关闭
          position: 'top-right',
          onClick: () => {
            // 点击通知跳转到公告页面
            const router = require('@/router').default
            router.push('/announcements')
          }
        })

        // 记录已处理的通知
        commit('ADD_PROCESSED_NOTIFICATION', key)

        // 触发公告刷新事件（用于顶部公告栏更新）
        commit('SET_LAST_ANNOUNCEMENT', {
          id: notify.announcement_id,
          title: notify.title,
          content: notify.content,
          priority: notify.priority,
          timestamp: notify.publish_time
        })
      }
    })
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

// 稳定身份的空兜底 —— 见下方 positions getter 的说明。@yutiansut @quantaxis
// 全部 snapshot 段的 getter 都用它:目前只有 positions 被 watch,
// 但字面量兜底是个「等着被踩」的陷阱,一并收敛。
const EMPTY_OBJ = Object.freeze({})

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
        // ⚠️ `...acc` 必须放在守卫**之前** @yutiansut @quantaxis
        //
        // 原来展开在守卫之后:字段**缺席**时守卫有效,字段**存在且为 null** 时
        // 展开会把 0 冲掉,守卫等于没写。而 `currentAccount` 直接喂
        // `OrderForm.vue:16/37` 的 `Math.round(currentAccount.available)` —— null 即崩。
        ...acc,
        balance: Number(acc.balance) || 0,
        available: Number(acc.available) || 0,
        margin: Number(acc.margin) || 0,
        risk_ratio: Number(acc.risk_ratio) || 0
      }))
    }
    return []
  },

  // 数值字段统一走 `Number(x) || 0` @yutiansut @quantaxis
  //
  // 两个坑,两个分支各中一个:
  // ① **主路径**(state.userAccounts,HTTP 原始数组)原本**完全没有守卫** ——
  //    只修下面的 CNY 兜底分支根本覆盖不到它,而这才是常走的那条。
  // ② CNY 分支的 `...cnyAccount` 展开在守卫**之后**:字段缺席时守卫有效,
  //    字段存在且为 `null` 时展开把 0 冲掉,守卫等于没写。
  //
  // 与 `userAccounts` getter 不同,本 getter **有活消费者**
  // (`AccountSelector.vue:76`,挂在 trade/batch、trade/conditional、trade/transfer 三页)。
  currentAccount: state => {
    // ⚠️ 这里**不再**把 null 兜成 0。@yutiansut @quantaxis
    //
    // 原守卫是 `Number(acc.balance) || 0` 等四行,目的是防
    // `OrderForm.vue:16` 的 `Math.round(...)` 遇到 undefined 崩溃。
    // 但它同时**抹掉了「未加载」信号** —— `AccountSelector.vue:39/43/47`
    // 拿到的永远是 0,那三行于是显示 `¥0.00`,用户分不清
    // 「还没加载」与「账户没钱」;在下单页上这个误读代价很直接。
    // (回验代理实测:改了 formatNumber 的 `--` 分支后,三行明细在
    //  null 与 0 两种场景下**逐字相同** —— `--` 分支成了死代码。)
    //
    // 去掉守卫是安全的:实测**所有数值消费者都自带兜底** ——
    //   · OrderForm.vue:16/37    `Math.round(Number(x) || 0)`
    //   · AccountSelector.vue:52 `(Number(x) || 0) * 100`
    //   · admin/risk.vue:245     `(Number(x) || 0) * 100`
    //   · my-accounts.vue 用的是**自己的局部 data**(:351),不走本 getter
    // 即守卫是**冗余**的,而它的副作用(编造 0)是实打实的。
    //
    // 原则:**兜底放在数值消费点,不放在数据源** ——
    // 放在数据源会让所有展示层都失去区分"缺失"与"零"的能力。
    const guard = acc => ({ ...acc })
    // 先从 userAccounts 查找
    if (state.currentAccountId && state.userAccounts && state.userAccounts.length > 0) {
      const found = state.userAccounts.find(acc => acc.account_id === state.currentAccountId)
      if (found) return guard(found)
    }
    // 否则从 snapshot.accounts 获取（默认 CNY）
    if (state.snapshot.accounts) {
      const cnyAccount = state.snapshot.accounts['CNY']
      if (cnyAccount) {
        return guard({
          account_id: cnyAccount.user_id || state.snapshot.user_id || 'default',
          account_name: `${cnyAccount.user_id || '交易账户'} (CNY)`,
          currency: 'CNY',
          ...cnyAccount
        })
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
  // ⚠️ 这个 getter 早就存在,但生产页面里**没有一个消费它** @yutiansut @quantaxis
  //
  // 完整链路只差最后一环:
  //   trade_gateway.rs:1662  产出 positions patch     ✓(本次会话补的,此前一条不发)
  //   SnapshotManager.js:305 getPositions() 能取      ✓
  //   websocket.js:446/450   每条 WS 消息提交整个 snapshot ✓
  //   本 getter                                       ✓
  //   ── 页面消费 ──                                   ✗ 只有 WebSocketTest.vue(测试页)
  // 于是补了后端推送仍是**空转**:持仓页 mounted 加载一次,之后永不刷新。
  // 持仓/订单/成交/我的账户四页**无轮询**,成交后一直显示陈旧数据,直到手动切页。
  // (监控页与管理页有 10s 轮询,trade 页有 5s —— 不是「全仓为 0」,是**用户数据页为 0**。)
  //
  // 我一度在此另加了一个 `snapshotPositions`,与本行逐字节相同 —— 已删。
  // **加 getter 前先读完这个 getters 块**。
  // ⚠️ `|| {}` 不能写成字面量 —— 那是每次重算都产生**新对象**。@yutiansut @quantaxis
  //
  // `SET_SNAPSHOT` 在**每条** WS 消息上整体替换 `state.snapshot`,于是本 computed
  // 每次都脏、每次都重算;若兜底是 `|| {}` 字面量,身份每次都变,
  // 任何 `watch` 都会认为「变了」。
  // 实测(回验 agent,30 秒):66 次 snapshot 提交里 **0 次带 positions、
  // 0 次内容变化**,却触发了 **15 次** `/position/user` 请求(间隔全是 2000ms,
  // 节流被打满),每次 reload 还附带 N 个 getTick —— 13 个合约约 7 req/s/标签页。
  // 用模块级冻结常量兜底,身份稳定。
  positions: state => state.snapshot.positions || EMPTY_OBJ,

  // 获取特定持仓
  position: state => (instrumentId) => {
    return state.snapshot.positions && state.snapshot.positions[instrumentId]
  },

  // 所有订单
  orders: state => state.snapshot.orders || EMPTY_OBJ,

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
  trades: state => state.snapshot.trades || EMPTY_OBJ,

  // 所有行情
  quotes: state => state.snapshot.quotes || EMPTY_OBJ,

  // 获取特定行情
  quote: state => (instrumentId) => {
    return state.snapshot.quotes && state.snapshot.quotes[instrumentId]
  },

  // 订阅的合约列表
  subscribedInstruments: state => state.subscribedInstruments,

  // 通知信息
  notifications: state => state.snapshot.notify || EMPTY_OBJ,

  // ============================================================================
  // ✨ 因子数据 Getters @yutiansut @quantaxis
  // ============================================================================

  // 所有因子数据
  factors: state => state.snapshot.factors || EMPTY_OBJ,

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
    return (factors && factors.values) || EMPTY_OBJ
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
