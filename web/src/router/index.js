import Vue from 'vue'
import VueRouter from 'vue-router'
import store from '@/store'

Vue.use(VueRouter)

// 布局组件
const Layout = () => import('@/layout/index.vue')

const routes = [
  // 登录注册页面（无布局）
  {
    path: '/login',
    name: 'Login',
    component: () => import('@/views/login.vue'),
    meta: { title: '登录', requireAuth: false }
  },
  {
    path: '/register',
    name: 'Register',
    component: () => import('@/views/register.vue'),
    meta: { title: '注册', requireAuth: false }
  },
  // 主应用（带布局）
  {
    path: '/',
    component: Layout,
    redirect: '/dashboard',
    children: [
      {
        path: 'dashboard',
        name: 'Dashboard',
        component: () => import('@/views/dashboard/index.vue'),
        meta: { title: '监控仪表盘', icon: 'el-icon-monitor', group: 'system' }
      },
      {
        path: 'trade',
        name: 'Trade',
        component: () => import('@/views/trade/index.vue'),
        meta: { title: '交易面板', icon: 'el-icon-sell', group: 'trading' }
      },
      {
        path: 'chart',
        name: 'Chart',
        component: () => import('@/views/chart/index.vue'),
        meta: { title: 'K线图表', icon: 'el-icon-data-line', group: 'trading' }
      },
      {
        path: 'accounts',
        name: 'Accounts',
        component: () => import('@/views/accounts/index.vue'),
        meta: { title: '账户管理', icon: 'el-icon-user', group: 'trading' }
      },
      {
        path: 'orders',
        name: 'Orders',
        component: () => import('@/views/orders/index.vue'),
        meta: { title: '订单管理', icon: 'el-icon-document', group: 'trading' }
      },
      {
        path: 'positions',
        name: 'Positions',
        component: () => import('@/views/positions/index.vue'),
        meta: { title: '持仓管理', icon: 'el-icon-s-data', group: 'trading' }
      },
      {
        path: 'trades',
        name: 'Trades',
        component: () => import('@/views/trades/index.vue'),
        meta: { title: '成交记录', icon: 'el-icon-tickets', group: 'trading' }
      },
      // Phase 11: 银期转账 @yutiansut @quantaxis
      {
        path: 'transfer',
        name: 'Transfer',
        component: () => import('@/views/trade/transfer.vue'),
        meta: { title: '银期转账', icon: 'el-icon-refresh', group: 'trading' }
      },
      // Phase 11: 条件单 @yutiansut @quantaxis
      {
        path: 'conditional-orders',
        name: 'ConditionalOrders',
        component: () => import('@/views/trade/conditional.vue'),
        meta: { title: '条件单', icon: 'el-icon-aim', group: 'trading' }
      },
      // Phase 11: 批量下单 @yutiansut @quantaxis
      {
        path: 'batch-orders',
        name: 'BatchOrders',
        component: () => import('@/views/trade/batch.vue'),
        meta: { title: '批量下单', icon: 'el-icon-copy-document', group: 'trading' }
      },
      {
        path: 'account-curve',
        name: 'AccountCurve',
        component: () => import('@/views/user/account-curve.vue'),
        meta: { title: '资金曲线', icon: 'el-icon-data-analysis', group: 'analysis' }
      },
      {
        path: 'my-accounts',
        name: 'MyAccounts',
        component: () => import('@/views/user/my-accounts.vue'),
        meta: { title: '我的账户', icon: 'el-icon-wallet', group: 'user' }
      },
      {
        path: 'account/:accountId',
        name: 'AccountDetail',
        component: () => import('@/views/user/account-detail.vue'),
        meta: { title: '账户详情', group: 'user', hidden: true }
      },
      {
        path: 'monitoring',
        name: 'Monitoring',
        component: () => import('@/views/monitoring/index.vue'),
        meta: { title: '系统监控', icon: 'el-icon-odometer', group: 'system' }
      },
      // 管理端功能
      {
        path: 'market-overview',
        name: 'MarketOverview',
        component: () => import('@/views/admin/market-overview.vue'),
        meta: { title: '市场总览', icon: 'el-icon-view', group: 'system' }
      },
      {
        path: 'admin-instruments',
        name: 'AdminInstruments',
        component: () => import('@/views/admin/instruments.vue'),
        meta: { title: '合约管理', icon: 'el-icon-s-management', group: 'admin', requireAdmin: true }
      },
      {
        path: 'admin-risk',
        name: 'AdminRisk',
        component: () => import('@/views/admin/risk.vue'),
        meta: { title: '风控监控', icon: 'el-icon-warning', group: 'admin', requireAdmin: true }
      },
      {
        path: 'admin-settlement',
        name: 'AdminSettlement',
        component: () => import('@/views/admin/settlement.vue'),
        meta: { title: '结算管理', icon: 'el-icon-s-finance', group: 'admin', requireAdmin: true }
      },
      {
        path: 'admin-accounts',
        name: 'AdminAccounts',
        component: () => import('@/views/admin/accounts.vue'),
        meta: { title: '账户管理', icon: 'el-icon-user-solid', group: 'admin', requireAdmin: true }
      },
      {
        path: 'admin-transactions',
        name: 'AdminTransactions',
        component: () => import('@/views/admin/transactions.vue'),
        meta: { title: '资金流水', icon: 'el-icon-notebook-2', group: 'admin', requireAdmin: true }
      },
      // Phase 12-13: 用户功能 @yutiansut @quantaxis
      {
        path: 'password',
        name: 'Password',
        component: () => import('@/views/user/password.vue'),
        meta: { title: '密码管理', icon: 'el-icon-key', group: 'user' }
      },
      {
        path: 'commission',
        name: 'Commission',
        component: () => import('@/views/user/commission.vue'),
        meta: { title: '手续费查询', icon: 'el-icon-money', group: 'user' }
      },
      {
        path: 'margin',
        name: 'Margin',
        component: () => import('@/views/user/margin.vue'),
        meta: { title: '保证金查询', icon: 'el-icon-s-finance', group: 'user' }
      },
      // Phase 13: 管理端功能 @yutiansut @quantaxis
      {
        path: 'admin-account-freeze',
        name: 'AdminAccountFreeze',
        component: () => import('@/views/admin/account-freeze.vue'),
        meta: { title: '账户状态管理', icon: 'el-icon-lock', group: 'admin', requireAdmin: true }
      },
      {
        path: 'admin-audit-logs',
        name: 'AdminAuditLogs',
        component: () => import('@/views/admin/audit-logs.vue'),
        meta: { title: '审计日志', icon: 'el-icon-document-checked', group: 'admin', requireAdmin: true }
      },
      {
        path: 'admin-announcements',
        name: 'AdminAnnouncements',
        component: () => import('@/views/admin/announcements.vue'),
        meta: { title: '系统公告', icon: 'el-icon-bell', group: 'admin', requireAdmin: true }
      },
      // WebSocket 测试页面
      {
        path: 'websocket-test',
        name: 'WebSocketTest',
        component: () => import('@/views/WebSocketTest.vue'),
        meta: { title: 'WebSocket 测试', icon: 'el-icon-connection', group: 'system' }
      }
    ]
  }
]

const router = new VueRouter({
  mode: 'hash',
  base: process.env.BASE_URL,
  routes
})

// 路由守卫
router.beforeEach((to, from, next) => {
  const isLoggedIn = store.getters.isLoggedIn
  const isAdmin = store.getters.isAdmin

  // 设置页面标题
  document.title = to.meta.title ? `${to.meta.title} - QAExchange` : 'QAExchange'

  // 如果访问登录/注册页面且已登录，跳转到首页
  if ((to.path === '/login' || to.path === '/register') && isLoggedIn) {
    next('/dashboard')
    return
  }

  // 如果访问需要登录的页面但未登录，跳转到登录页
  if (to.meta.requireAuth !== false && !isLoggedIn) {
    next({
      path: '/login',
      query: { redirect: to.fullPath }
    })
    return
  }

  // 如果访问管理员页面但不是管理员，拒绝访问
  if (to.meta.requireAdmin && !isAdmin) {
    next('/dashboard')
    return
  }

  next()
})

export default router
