/**
 * Vuex Store @yutiansut @quantaxis
 * RBAC 权限体系集成
 */
import Vue from 'vue'
import Vuex from 'vuex'
import { getSystemMonitoring, login as apiLogin, getCurrentUser } from '@/api'

// Vuex 模块
import websocket from './modules/websocket'

Vue.use(Vuex)

// ==================== RBAC 角色和权限常量 @yutiansut @quantaxis ====================
export const UserRoles = {
  Admin: 'Admin',
  Trader: 'Trader',
  Analyst: 'Analyst',
  ReadOnly: 'ReadOnly',
  RiskManager: 'RiskManager',
  Settlement: 'Settlement'
}

export const Permissions = {
  // 交易权限
  Trade: 'Trade',
  CancelOrder: 'CancelOrder',
  ModifyOrder: 'ModifyOrder',
  BatchOrder: 'BatchOrder',
  ConditionalOrder: 'ConditionalOrder',
  Transfer: 'Transfer',
  // 账户权限
  ViewOwnAccount: 'ViewOwnAccount',
  ViewAllAccounts: 'ViewAllAccounts',
  OpenAccount: 'OpenAccount',
  CloseAccount: 'CloseAccount',
  FreezeAccount: 'FreezeAccount',
  DepositWithdraw: 'DepositWithdraw',
  // 订单/持仓/成交权限
  ViewOwnOrders: 'ViewOwnOrders',
  ViewAllOrders: 'ViewAllOrders',
  ViewOwnPositions: 'ViewOwnPositions',
  ViewAllPositions: 'ViewAllPositions',
  ViewOwnTrades: 'ViewOwnTrades',
  ViewAllTrades: 'ViewAllTrades',
  // 市场数据权限
  ViewMarketData: 'ViewMarketData',
  ViewKline: 'ViewKline',
  ViewOrderbook: 'ViewOrderbook',
  // 风控权限
  ViewRisk: 'ViewRisk',
  ForceLiquidate: 'ForceLiquidate',
  // 结算权限
  ExecuteSettlement: 'ExecuteSettlement',
  SetSettlementPrice: 'SetSettlementPrice',
  ViewSettlementHistory: 'ViewSettlementHistory',
  // 合约管理权限
  ViewInstruments: 'ViewInstruments',
  CreateInstrument: 'CreateInstrument',
  ModifyInstrument: 'ModifyInstrument',
  SuspendResumeInstrument: 'SuspendResumeInstrument',
  // 用户管理权限
  ViewUsers: 'ViewUsers',
  CreateUser: 'CreateUser',
  ModifyUserRole: 'ModifyUserRole',
  FreezeUser: 'FreezeUser',
  // 系统管理权限
  ViewMonitoring: 'ViewMonitoring',
  ViewAuditLogs: 'ViewAuditLogs',
  ManageAnnouncements: 'ManageAnnouncements',
  ViewStatistics: 'ViewStatistics',
  ExportData: 'ExportData'
}

export default new Vuex.Store({
  modules: {
    websocket
  },

  state: {
    // 系统状态(/api/monitoring/status 的完整响应)@yutiansut @quantaxis
    // 由 layout 的 15 秒健康检查写入,dashboard 直接读 ——
    // 此前两处**各自请求同一接口**:layout 拿到 payload 只留一个布尔就丢弃,
    // dashboard 又取一遍同样的东西(回验实测整页加载 2 次,相隔 1ms)。
    sysStatus: null,
    // 当前用户 ID
    currentUser: localStorage.getItem('currentUser') || '',

    // 用户信息
    userInfo: JSON.parse(localStorage.getItem('userInfo') || 'null'),

    // 认证 Token
    token: localStorage.getItem('token') || '',

    // 是否已登录
    isLoggedIn: !!localStorage.getItem('token'),

    // RBAC: 用户角色列表 @yutiansut @quantaxis
    roles: JSON.parse(localStorage.getItem('roles') || '[]'),

    // RBAC: 用户权限列表 @yutiansut @quantaxis
    permissions: JSON.parse(localStorage.getItem('permissions') || '[]'),

    // 监控数据是否已从后端取回一次。@yutiansut @quantaxis
    //
    // ⚠️ 存在的理由:`monitoring.accounts` 的初值是三个 `0`,而「尚未加载」
    // 与「真实的零」在数值上**无法区分**。子代理实测的后果:
    // dashboard 饼图在首屏画出一张 **33% / 33% / 33% 的等分资金分布** ——
    // 信心十足但完全虚构。图表原本只有「可显示 / 超出范围」两态,
    // 缺的正是这第三态。
    // (为什么不把初值改成 `null`:`monitoring` 下还有 orders/trades/storage
    //  十几个字段被多处直接参与算术,统一改 null 的波及面远大于加一个标志。)
    monitoringLoaded: false,

    // 系统监控数据
    monitoring: {
      accounts: {
        total_count: 0,
        active_count: 0,
        total_balance: 0,
        total_available: 0,
        total_margin_used: 0
      },
      orders: {
        total_count: 0,
        pending_count: 0,
        filled_count: 0,
        cancelled_count: 0
      },
      trades: {
        total_count: 0,
        total_amount: 0,
        total_volume: 0
      },
      storage: {
        oltp: {
          total_records: 0,
          total_batches: 0,
          total_errors: 0
        },
        olap: {
          total_tasks: 0,
          pending_tasks: 0,
          converting_tasks: 0,
          success_tasks: 0,
          failed_tasks: 0,
          avg_duration_secs: 0
        }
      }
    },

    // 自动刷新定时器
    refreshTimer: null
  },

  mutations: {
    SET_SYS_STATUS(state, v) {
      state.sysStatus = v
    },

    SET_CURRENT_USER(state, userId) {
      state.currentUser = userId
      localStorage.setItem('currentUser', userId)
    },

    SET_USER_INFO(state, userInfo) {
      state.userInfo = userInfo
      if (userInfo) {
        localStorage.setItem('userInfo', JSON.stringify(userInfo))
      } else {
        localStorage.removeItem('userInfo')
      }
    },

    SET_TOKEN(state, token) {
      state.token = token
      state.isLoggedIn = !!token
      if (token) {
        localStorage.setItem('token', token)
      } else {
        localStorage.removeItem('token')
      }
    },

    SET_MONITORING(state, data) {
      state.monitoringLoaded = true
      state.monitoring = data
    },

    SET_REFRESH_TIMER(state, timer) {
      state.refreshTimer = timer
    },

    // RBAC mutations @yutiansut @quantaxis
    SET_ROLES(state, roles) {
      state.roles = roles || []
      localStorage.setItem('roles', JSON.stringify(state.roles))
    },

    SET_PERMISSIONS(state, permissions) {
      state.permissions = permissions || []
      localStorage.setItem('permissions', JSON.stringify(state.permissions))
    }
  },

  actions: {
    // 设置当前用户
    setCurrentUser({ commit }, userId) {
      commit('SET_CURRENT_USER', userId)
    },

    // 用户登录 @yutiansut @quantaxis
    // 支持 RBAC 角色和权限
    async login({ commit }, loginData) {
      try {
        const data = await apiLogin(loginData)

        // ✨ 检查内层登录结果 @yutiansut @quantaxis
        // 后端返回 { success: true, data: { success: false, message: "..." } }
        // 外层 success 表示 HTTP 请求成功，内层 success 表示登录业务成功
        if (!data.success) {
          throw new Error(data.message || '登录失败')
        }

        const { token, user_id, username, is_admin, roles, permissions } = data

        // ✨ 重要：先保存用户信息，再保存 token @yutiansut @quantaxis
        // 因为 SET_TOKEN 会触发 isLoggedIn 变化，进而触发 WebSocket 初始化
        // 必须确保 userInfo.user_id (UUID) 在 WebSocket 初始化前已设置

        // 1. 保存用户信息 (包含 user_id UUID)
        const userInfo = { user_id, username, is_admin, roles }
        commit('SET_USER_INFO', userInfo)
        commit('SET_CURRENT_USER', user_id)

        // 2. 保存 RBAC 信息
        commit('SET_ROLES', roles || [])
        commit('SET_PERMISSIONS', permissions || [])

        // 3. 最后保存 token（触发 isLoggedIn 变化和 WebSocket 初始化）
        commit('SET_TOKEN', token)

        return data
      } catch (error) {
        console.error('Login failed:', error)
        throw error
      }
    },

    // 用户登出 @yutiansut @quantaxis
    logout({ commit, dispatch }) {
      commit('SET_TOKEN', '')
      commit('SET_USER_INFO', null)
      commit('SET_CURRENT_USER', '')
      // 清除 RBAC 信息
      commit('SET_ROLES', [])
      commit('SET_PERMISSIONS', [])
      // ✨ 断开 WebSocket —— 此前登出**不断连接**。@yutiansut @quantaxis
      //
      // `websocket/destroyWebSocket`(store/modules/websocket.js:297)一直存在,
      // 但全仓只有 App.vue 的 `cleanupWebSocket` 会调,而那是组件销毁钩子 ——
      // 登出走的是 SPA 路由跳转,组件没销毁,钩子不触发。
      // 后果:登出后连接仍在推 accounts/orders/positions/trades,
      // 子代理实测另一标签 87 秒内继续收到 196~208 条推送。
      // 用已有 action,不新增销毁逻辑。
      dispatch('websocket/destroyWebSocket')
      localStorage.clear()
    },

    // 获取用户信息
    async fetchUserInfo({ commit, state }) {
      if (!state.currentUser) {
        throw new Error('No user ID available')
      }
      try {
        const data = await getCurrentUser(state.currentUser)
        commit('SET_USER_INFO', data)
        return data
      } catch (error) {
        console.error('Failed to fetch user info:', error)
        throw error
      }
    },

    // 获取系统监控数据
    async fetchMonitoring({ commit }) {
      try {
        const data = await getSystemMonitoring()
        commit('SET_MONITORING', data)
        return data
      } catch (error) {
        console.error('Failed to fetch monitoring data:', error)
        throw error
      }
    },

    // 启动自动刷新
    startAutoRefresh({ dispatch, commit, state }) {
      // 清除现有定时器
      if (state.refreshTimer) {
        clearInterval(state.refreshTimer)
      }

      // 立即执行一次
      // ⚠️ **必须带 `.catch`** —— `fetchMonitoring` 的 catch 里是 `throw error`
      // (store/index.js:292),所以它**会 reject**。裸 dispatch 丢弃返回值 ⇒
      // 未捕获的 promise rejection。
      // 实测(2026-09-06,全接口 500):`/dashboard` 与 `/market-overview` 各抛 1 条
      // `pageerror: Request failed with status code 500`,栈里全是 axios 内部帧、
      // 没有应用帧,所以从栈上看不出是这里发起的。
      // 用户影响为零(拦截器已弹过服务端消息,main.js:162 的全局去重也正确跳过),
      // 但它**污染错误通道** —— 盯 `pageerror` 的监控从此长期假阳性,
      // 真正的未捕获错误反而更难被发现。
      // 这里空 catch 是**有意**的:错误已经被拦截器提示过,这条轮询失败不需要额外动作。
      // @yutiansut @quantaxis
      dispatch('fetchMonitoring').catch(() => {})

      // 每10秒刷新一次
      const timer = setInterval(() => {
        dispatch('fetchMonitoring').catch(() => {})
      }, 10000)

      commit('SET_REFRESH_TIMER', timer)
    },

    // 停止自动刷新
    stopAutoRefresh({ commit, state }) {
      if (state.refreshTimer) {
        clearInterval(state.refreshTimer)
        commit('SET_REFRESH_TIMER', null)
      }
    }
  },

  getters: {
    sysStatus: state => state.sysStatus,
    currentUser: state => state.currentUser,
    userInfo: state => state.userInfo,
    token: state => state.token,
    isLoggedIn: state => state.isLoggedIn,
    isAdmin: state => (state.userInfo && state.userInfo.is_admin) || false,
    monitoring: state => state.monitoring,

    // 保证金占用率 —— 这是**第二实现**(第一处在 `monitoring/index.vue:237`)
    // @yutiansut @quantaxis
    //
    // 缺字段时原写法给 `NaN%`,而它同时喂 `dashboard/index.vue:51` 的文案
    // 和 `:60` 的进度条宽度。`width: 'NaN%'` 是**非法 CSS,浏览器直接丢弃** ——
    // 进度条不是归零,而是**静默冻结在上一个合法值**,比显示 0 更误导。
    // ✨ 拆成三个:数值 / 显示 / 宽度 —— 三个消费点的正确表示各不相同。
    // @yutiansut @quantaxis
    //
    // 原实现 `Number(x) || 0` 无任何闸,子代理实测两种错法:
    //   · 真实数据(balance = 1e300)→ 徽标显示 **`0.0%`** —— 把「未知」说成「零」
    //   · 注入 margin = 1e300 → `.toFixed(1)` 返回 `"1e+300"` → 渲染 **`1e+300%`**
    // 本 getter 在 store 里(不在 .vue 中),所以那轮 80 处模板扫描没覆盖到。
    //
    // 为什么不能统一返回 `--`:`:60` 把它拼进 `width: X%`,
    // 而 `width:'--%'` 与 `width:'NaN%'` 一样是**非法 CSS,浏览器直接丢弃**,
    // 进度条不是归零而是**冻结在上一个合法值**——比显示 0 更误导(见上方原注释)。
    // 所以宽度单独走一条:未知时给 `0%`(零宽是诚实的「画不出来」)。

    /// 数值版:未知返回 `null`。用于阈值比较(`null > 50` 为 false,落到 normal)。
    marginUtilization: state => {
      if (!state.monitoringLoaded) return null   // 未加载 ≠ 0%
      const acc = (state.monitoring && state.monitoring.accounts) || {}
      const ok = v => Number.isFinite(v) && Math.abs(v) < 1e15
      const b = Number(acc.total_balance)
      const m = Number(acc.total_margin_used)
      if (!ok(b) || !ok(m) || b <= 0) return null
      return (m / b) * 100
    },

    /// 显示版:自带百分号;未知时是 `--`(**不是** `--%`)。
    marginUtilizationText: (state, getters) => {
      const v = getters.marginUtilization
      return v === null ? '--' : v.toFixed(1) + '%'
    },

    /// 宽度版:未知给 `0%`,并夹到 [0,100] —— 超过 100% 的条会溢出容器。
    marginUtilizationWidth: (state, getters) => {
      const v = getters.marginUtilization
      if (v === null) return '0%'
      return Math.min(100, Math.max(0, v)).toFixed(1) + '%'
    },

    // ==================== RBAC Getters @yutiansut @quantaxis ====================

    // 获取用户角色列表
    roles: state => state.roles,

    // 获取用户权限列表
    permissions: state => state.permissions,

    // 检查用户是否拥有指定角色
    hasRole: state => role => {
      // Admin 拥有所有角色
      if (state.roles.includes('Admin')) return true
      return state.roles.includes(role)
    },

    // 检查用户是否拥有指定权限
    hasPermission: state => permission => {
      // Admin 拥有所有权限
      if (state.roles.includes('Admin')) return true
      return state.permissions.includes(permission)
    },

    // 检查用户是否拥有任一指定权限
    hasAnyPermission: state => permissions => {
      if (state.roles.includes('Admin')) return true
      return permissions.some(p => state.permissions.includes(p))
    },

    // 检查用户是否拥有所有指定权限
    hasAllPermissions: state => permissions => {
      if (state.roles.includes('Admin')) return true
      return permissions.every(p => state.permissions.includes(p))
    },

    // 获取用户主要角色 (优先级最高)
    primaryRole: state => {
      const rolePriority = {
        Admin: 100,
        RiskManager: 80,
        Settlement: 70,
        Trader: 50,
        Analyst: 30,
        ReadOnly: 10
      }
      if (state.roles.length === 0) return 'ReadOnly'
      return state.roles.reduce((a, b) =>
        (rolePriority[a] || 0) >= (rolePriority[b] || 0) ? a : b
      )
    },

    // 是否是风控员
    isRiskManager: state => state.roles.includes('RiskManager') || state.roles.includes('Admin'),

    // 是否是结算员
    isSettlement: state => state.roles.includes('Settlement') || state.roles.includes('Admin'),

    // 是否有交易权限
    canTrade: state => {
      if (state.roles.includes('Admin')) return true
      return state.permissions.includes('Trade')
    },

    // 是否有管理员权限 (ViewAllAccounts, etc.)
    canManage: state => {
      if (state.roles.includes('Admin')) return true
      return state.permissions.includes('ViewAllAccounts')
    }
  }
})
