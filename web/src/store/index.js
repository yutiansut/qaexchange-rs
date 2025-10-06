import Vue from 'vue'
import Vuex from 'vuex'
import { getSystemMonitoring, login as apiLogin, getCurrentUser } from '@/api'

Vue.use(Vuex)

export default new Vuex.Store({
  state: {
    // 当前用户 ID
    currentUser: localStorage.getItem('currentUser') || '',

    // 用户信息
    userInfo: JSON.parse(localStorage.getItem('userInfo') || 'null'),

    // 认证 Token
    token: localStorage.getItem('token') || '',

    // 是否已登录
    isLoggedIn: !!localStorage.getItem('token'),

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
    }
  },

  actions: {
    // 设置当前用户
    setCurrentUser({ commit }, userId) {
      commit('SET_CURRENT_USER', userId)
    },

    // 用户登录
    async login({ commit }, loginData) {
      try {
        const data = await apiLogin(loginData)
        const { token, user_id, username, is_admin } = data

        // 保存 token
        commit('SET_TOKEN', token)

        // 保存用户信息
        const userInfo = { user_id, username, is_admin }
        commit('SET_USER_INFO', userInfo)
        commit('SET_CURRENT_USER', user_id)

        return data
      } catch (error) {
        console.error('Login failed:', error)
        throw error
      }
    },

    // 用户登出
    logout({ commit }) {
      commit('SET_TOKEN', '')
      commit('SET_USER_INFO', null)
      commit('SET_CURRENT_USER', '')
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
    }
  }
})
