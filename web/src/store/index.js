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
    logout({ commit }) {
      commit('SET_TOKEN', '')
      commit('SET_USER_INFO', null)
      commit('SET_CURRENT_USER', '')
      // 清除 RBAC 信息
      commit('SET_ROLES', [])
      commit('SET_PERMISSIONS', [])
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
      dispatch('fetchMonitoring')

      // 每10秒刷新一次
      const timer = setInterval(() => {
        dispatch('fetchMonitoring')
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
    currentUser: state => state.currentUser,
    userInfo: state => state.userInfo,
    token: state => state.token,
    isLoggedIn: state => state.isLoggedIn,
    isAdmin: state => (state.userInfo && state.userInfo.is_admin) || false,
    monitoring: state => state.monitoring,

    // 保证金占用率
    marginUtilization: state => {
      const balance = state.monitoring.accounts.total_balance
      const margin = state.monitoring.accounts.total_margin_used
      return balance > 0 ? ((margin / balance) * 100).toFixed(1) : '0.0'
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
