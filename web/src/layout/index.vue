<template>
  <div class="layout">
    <!-- 侧边栏 -->
    <div class="sidebar" :class="{ collapsed: isCollapsed }">
      <div class="sidebar-header">
        <div class="logo">
          <svg class="logo-icon" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
            <path d="M3 13h2v-2H3v2zm0 4h2v-2H3v2zm0-8h2V7H3v2zm4 4h14v-2H7v2zm0 4h14v-2H7v2zM7 7v2h14V7H7z" fill="currentColor"/>
          </svg>
          <span v-if="!isCollapsed" class="logo-text">QAExchange</span>
        </div>
        <div v-if="!isCollapsed" class="logo-subtitle">量化交易系统</div>
      </div>

      <!-- 导航菜单 -->
      <el-menu
        :default-active="activeMenu"
        :collapse="isCollapsed"
        :collapse-transition="false"
        background-color="transparent"
        text-color="#8b949e"
        active-text-color="#ffffff"
        @select="handleMenuSelect"
      >
        <!-- 系统总览 -->
        <el-menu-item index="/dashboard">
          <i class="el-icon-data-board"></i>
          <span slot="title">系统总览</span>
        </el-menu-item>

        <!-- 交易中心 -->
        <div class="menu-group-title" v-if="!isCollapsed">交易中心</div>
        <el-menu-item index="/trade">
          <i class="el-icon-sell"></i>
          <span slot="title">交易面板</span>
        </el-menu-item>
        <el-menu-item index="/chart">
          <i class="el-icon-trend-charts"></i>
          <span slot="title">K线图表</span>
        </el-menu-item>
        <el-menu-item index="/orders">
          <i class="el-icon-document"></i>
          <span slot="title">订单管理</span>
        </el-menu-item>
        <el-menu-item index="/positions">
          <i class="el-icon-coin"></i>
          <span slot="title">持仓管理</span>
        </el-menu-item>
        <el-menu-item index="/trades">
          <i class="el-icon-finished"></i>
          <span slot="title">成交记录</span>
        </el-menu-item>
        <!-- Phase 11: 高级交易功能 @yutiansut @quantaxis -->
        <el-menu-item index="/transfer">
          <i class="el-icon-refresh"></i>
          <span slot="title">银期转账</span>
        </el-menu-item>
        <el-menu-item index="/conditional-orders">
          <i class="el-icon-aim"></i>
          <span slot="title">条件单</span>
        </el-menu-item>
        <el-menu-item index="/batch-orders">
          <i class="el-icon-copy-document"></i>
          <span slot="title">批量下单</span>
        </el-menu-item>

        <!-- 账户管理 -->
        <div class="menu-group-title" v-if="!isCollapsed">账户管理</div>
        <el-menu-item index="/accounts">
          <i class="el-icon-wallet"></i>
          <span slot="title">账户列表</span>
        </el-menu-item>
        <el-menu-item index="/my-accounts">
          <i class="el-icon-user"></i>
          <span slot="title">我的账户</span>
        </el-menu-item>
        <el-menu-item index="/account-curve">
          <i class="el-icon-data-line"></i>
          <span slot="title">资金曲线</span>
        </el-menu-item>
        <!-- Phase 12: 用户功能 @yutiansut @quantaxis -->
        <el-menu-item index="/password">
          <i class="el-icon-key"></i>
          <span slot="title">密码管理</span>
        </el-menu-item>
        <el-menu-item index="/commission">
          <i class="el-icon-money"></i>
          <span slot="title">手续费查询</span>
        </el-menu-item>
        <el-menu-item index="/margin">
          <i class="el-icon-s-finance"></i>
          <span slot="title">保证金查询</span>
        </el-menu-item>
        <!-- ✨ 系统公告入口 @yutiansut @quantaxis -->
        <el-menu-item index="/announcements">
          <i class="el-icon-bell"></i>
          <span slot="title">系统公告</span>
        </el-menu-item>

        <!-- 市场监控 -->
        <div class="menu-group-title" v-if="!isCollapsed">市场监控</div>
        <el-menu-item index="/market-overview">
          <i class="el-icon-view"></i>
          <span slot="title">市场总览</span>
        </el-menu-item>
        <el-menu-item index="/monitoring">
          <i class="el-icon-odometer"></i>
          <span slot="title">系统监控</span>
        </el-menu-item>

        <!-- 管理中心 -->
        <template v-if="isAdmin">
          <div class="menu-group-title" v-if="!isCollapsed">
            <span>管理中心</span>
            <el-tag type="danger" size="mini">Admin</el-tag>
          </div>
          <el-menu-item index="/admin-instruments">
            <i class="el-icon-tickets"></i>
            <span slot="title">合约管理</span>
          </el-menu-item>
          <el-menu-item index="/admin-risk">
            <i class="el-icon-warning-outline"></i>
            <span slot="title">风控监控</span>
          </el-menu-item>
          <el-menu-item index="/admin-settlement">
            <i class="el-icon-notebook-2"></i>
            <span slot="title">结算管理</span>
          </el-menu-item>
          <el-menu-item index="/admin-accounts">
            <i class="el-icon-user"></i>
            <span slot="title">账户管理</span>
          </el-menu-item>
          <el-menu-item index="/admin-transactions">
            <i class="el-icon-bank-card"></i>
            <span slot="title">资金流水</span>
          </el-menu-item>
          <!-- Phase 13: 管理端功能 @yutiansut @quantaxis -->
          <el-menu-item index="/admin-account-freeze">
            <i class="el-icon-lock"></i>
            <span slot="title">账户状态管理</span>
          </el-menu-item>
          <el-menu-item index="/admin-audit-logs">
            <i class="el-icon-document-checked"></i>
            <span slot="title">审计日志</span>
          </el-menu-item>
          <el-menu-item index="/admin-announcements">
            <i class="el-icon-bell"></i>
            <span slot="title">系统公告</span>
          </el-menu-item>
        </template>
      </el-menu>

      <!-- 折叠按钮 -->
      <div class="sidebar-footer">
        <div class="collapse-btn" @click="toggleCollapse">
          <i :class="isCollapsed ? 'el-icon-s-unfold' : 'el-icon-s-fold'"></i>
        </div>
      </div>
    </div>

    <!-- 右侧区域 -->
    <div class="main-container">
      <!-- ✨ 公告通知栏 @yutiansut @quantaxis -->
      <transition name="slide-down">
        <div
          v-if="showAnnouncementBar && currentAnnouncement"
          class="announcement-bar"
          :class="'announcement-' + announcementAlertType"
        >
          <div class="announcement-content" @click="goToAnnouncements">
            <i class="el-icon-bell announcement-icon"></i>
            <span class="announcement-label">
              <el-tag size="mini" :type="announcementAlertType === 'error' ? 'danger' : announcementAlertType">
                {{ currentAnnouncement.announcement_type || '公告' }}
              </el-tag>
            </span>
            <span class="announcement-title">{{ currentAnnouncement.title }}</span>
            <span v-if="announcements.length > 1" class="announcement-indicator">
              {{ announcementIndex + 1 }}/{{ announcements.length }}
            </span>
          </div>
          <div class="announcement-close" @click.stop="closeAnnouncementBar">
            <i class="el-icon-close"></i>
          </div>
        </div>
      </transition>

      <!-- 顶部栏 -->
      <div class="top-header">
        <div class="header-left">
          <div class="page-title">{{ pageTitle }}</div>
        </div>
        <div class="header-right">
          <!-- 系统状态指示 -->
          <div class="status-indicator">
            <span class="status-dot online"></span>
            <span class="status-text">系统正常</span>
          </div>

          <!-- 用户信息 -->
          <el-dropdown @command="handleUserCommand" trigger="click">
            <div class="user-info">
              <el-avatar :size="36" class="user-avatar">
                {{ avatarText }}
              </el-avatar>
              <div class="user-details" v-if="!isMobile">
                <div class="user-name">{{ displayName }}</div>
                <div class="user-role">
                  <el-tag v-if="isAdmin" type="danger" size="mini">管理员</el-tag>
                  <el-tag v-else type="info" size="mini">普通用户</el-tag>
                </div>
              </div>
              <i class="el-icon-caret-bottom"></i>
            </div>
            <el-dropdown-menu slot="dropdown" class="user-dropdown-menu">
              <el-dropdown-item disabled>
                <div class="dropdown-user-info">
                  <i class="el-icon-user"></i>
                  <span>用户ID: {{ currentUser }}</span>
                </div>
              </el-dropdown-item>
              <el-dropdown-item divided command="logout">
                <i class="el-icon-switch-button"></i>
                <span>退出登录</span>
              </el-dropdown-item>
            </el-dropdown-menu>
          </el-dropdown>
        </div>
      </div>

      <!-- 内容区域 -->
      <div class="content-wrapper">
        <transition name="fade" mode="out-in">
          <router-view :key="$route.fullPath" />
        </transition>
      </div>
    </div>
  </div>
</template>

<script>
// @yutiansut @quantaxis - 专业量化交易系统布局
import { mapGetters } from 'vuex'
import { queryAnnouncements } from '@/api'

export default {
  name: 'Layout',
  data() {
    return {
      isCollapsed: false,
      isMobile: false,
      // ✨ 公告系统 @yutiansut @quantaxis
      announcements: [],
      showAnnouncementBar: false,
      announcementIndex: 0,
      announcementTimer: null
    }
  },
  computed: {
    ...mapGetters(['currentUser', 'userInfo', 'isAdmin']),
    activeMenu() {
      return this.$route.path
    },
    displayName() {
      return (this.userInfo && this.userInfo.username) || this.currentUser || '用户'
    },
    avatarText() {
      const name = this.displayName
      return name ? name.charAt(0).toUpperCase() : 'U'
    },
    // ✨ 当前显示的公告 @yutiansut @quantaxis
    currentAnnouncement() {
      if (this.announcements.length === 0) return null
      return this.announcements[this.announcementIndex % this.announcements.length]
    },
    // 公告优先级对应的样式类型
    announcementAlertType() {
      if (!this.currentAnnouncement) return 'info'
      const priorityMap = {
        'Urgent': 'error',
        'High': 'warning',
        'Normal': 'info',
        'Low': 'info'
      }
      return priorityMap[this.currentAnnouncement.priority] || 'info'
    },
    pageTitle() {
      const titles = {
        '/dashboard': '系统总览',
        '/trade': '交易面板',
        '/chart': 'K线图表',
        '/orders': '订单管理',
        '/positions': '持仓管理',
        '/trades': '成交记录',
        // Phase 11: 高级交易功能 @yutiansut @quantaxis
        '/transfer': '银期转账',
        '/conditional-orders': '条件单',
        '/batch-orders': '批量下单',
        // 账户管理
        '/accounts': '账户列表',
        '/my-accounts': '我的账户',
        '/account-curve': '资金曲线',
        // Phase 12: 用户功能 @yutiansut @quantaxis
        '/password': '密码管理',
        '/commission': '手续费查询',
        '/margin': '保证金查询',
        // 市场监控
        '/market-overview': '市场总览',
        '/monitoring': '系统监控',
        // 管理中心
        '/admin-instruments': '合约管理',
        '/admin-risk': '风控监控',
        '/admin-settlement': '结算管理',
        '/admin-accounts': '账户管理',
        '/admin-transactions': '资金流水',
        // Phase 13: 管理端功能 @yutiansut @quantaxis
        '/admin-account-freeze': '账户状态管理',
        '/admin-audit-logs': '审计日志',
        '/admin-announcements': '系统公告',
        '/announcements': '系统公告'  // ✨ 用户端公告页面 @yutiansut @quantaxis
      }
      return titles[this.$route.path] || '量化交易系统'
    }
  },
  mounted() {
    this.checkMobile()
    window.addEventListener('resize', this.checkMobile)
    // ✨ 加载公告 @yutiansut @quantaxis
    this.loadAnnouncements()
  },
  beforeDestroy() {
    window.removeEventListener('resize', this.checkMobile)
    // ✨ 清理公告轮播定时器 @yutiansut @quantaxis
    if (this.announcementTimer) {
      clearInterval(this.announcementTimer)
    }
  },
  methods: {
    handleMenuSelect(index) {
      this.$router.push(index)
    },
    handleUserCommand(command) {
      if (command === 'logout') {
        this.$confirm('确定要退出登录吗?', '提示', {
          confirmButtonText: '确定',
          cancelButtonText: '取消',
          type: 'warning'
        }).then(() => {
          this.$store.dispatch('logout')
          this.$message.success('已退出登录')
          this.$router.push('/login')
        }).catch(() => {})
      }
    },
    toggleCollapse() {
      this.isCollapsed = !this.isCollapsed
    },
    checkMobile() {
      this.isMobile = window.innerWidth < 768
      if (this.isMobile) {
        this.isCollapsed = true
      }
    },
    // ✨ 加载公告列表 @yutiansut @quantaxis
    async loadAnnouncements() {
      try {
        const res = await queryAnnouncements({ active_only: true, page_size: 20 })
        // 过滤有效期内的公告
        const now = Date.now()
        this.announcements = (res.announcements || []).filter(a => {
          const from = a.effective_from ? a.effective_from * 1000 : 0
          const until = a.effective_until ? a.effective_until * 1000 : Number.MAX_SAFE_INTEGER
          return now >= from && now <= until
        })
        // 按优先级排序：Urgent > High > Normal > Low
        const priorityOrder = { 'Urgent': 0, 'High': 1, 'Normal': 2, 'Low': 3 }
        this.announcements.sort((a, b) =>
          (priorityOrder[a.priority] || 3) - (priorityOrder[b.priority] || 3)
        )
        if (this.announcements.length > 0) {
          this.showAnnouncementBar = true
          this.startAnnouncementRotation()
        }
      } catch (error) {
        console.error('加载公告失败', error)
      }
    },
    // ✨ 开始公告轮播 @yutiansut @quantaxis
    startAnnouncementRotation() {
      if (this.announcementTimer) {
        clearInterval(this.announcementTimer)
      }
      if (this.announcements.length > 1) {
        this.announcementTimer = setInterval(() => {
          this.announcementIndex = (this.announcementIndex + 1) % this.announcements.length
        }, 8000) // 每8秒切换一条
      }
    },
    // ✨ 关闭公告栏 @yutiansut @quantaxis
    closeAnnouncementBar() {
      this.showAnnouncementBar = false
      if (this.announcementTimer) {
        clearInterval(this.announcementTimer)
      }
    },
    // ✨ 点击公告查看详情 @yutiansut @quantaxis
    goToAnnouncements() {
      this.$router.push('/announcements')
    }
  }
}
</script>

<style lang="scss" scoped>
// @yutiansut @quantaxis - 专业量化交易系统布局样式
$sidebar-width: 220px;
$sidebar-collapsed-width: 64px;
$header-height: 56px;
$primary-color: #1890ff;
$dark-bg-primary: #0d1117;
$dark-bg-secondary: #161b22;
$dark-bg-tertiary: #21262d;
$dark-border: #30363d;
$dark-text-primary: #f0f6fc;
$dark-text-secondary: #8b949e;

// ✨ 深色主题配色 @yutiansut @quantaxis
.layout {
  display: flex;
  min-height: 100vh;
  background: $dark-bg-primary;  // 深色主题背景
}

// 侧边栏
.sidebar {
  width: $sidebar-width;
  background: $dark-bg-primary;
  display: flex;
  flex-direction: column;
  transition: width 0.25s ease;
  position: fixed;
  left: 0;
  top: 0;
  height: 100vh;
  z-index: 1000;

  &.collapsed {
    width: $sidebar-collapsed-width;

    .sidebar-header {
      padding: 16px 12px;
    }

    .logo {
      justify-content: center;
    }

    .logo-subtitle {
      display: none;
    }

    .menu-group-title {
      display: none;
    }
  }
}

.sidebar-header {
  padding: 20px 16px;
  border-bottom: 1px solid $dark-border;
}

.logo {
  display: flex;
  align-items: center;
  gap: 10px;

  .logo-icon {
    width: 32px;
    height: 32px;
    color: $primary-color;
    flex-shrink: 0;
  }

  .logo-text {
    font-size: 18px;
    font-weight: 700;
    color: $dark-text-primary;
    white-space: nowrap;
  }
}

.logo-subtitle {
  font-size: 12px;
  color: $dark-text-secondary;
  margin-top: 4px;
  padding-left: 42px;
}

// 菜单样式
.menu-group-title {
  padding: 20px 16px 8px;
  font-size: 11px;
  font-weight: 600;
  color: $dark-text-secondary;
  text-transform: uppercase;
  letter-spacing: 0.5px;
  display: flex;
  align-items: center;
  gap: 8px;
}

::v-deep .el-menu {
  border: none;
  flex: 1;
  overflow-y: auto;
  padding: 8px;

  .el-menu-item {
    height: 44px;
    line-height: 44px;
    margin: 2px 0;
    border-radius: 8px;
    transition: all 0.2s ease;

    i {
      color: $dark-text-secondary;
      font-size: 18px;
      margin-right: 12px;
    }

    &:hover {
      background: $dark-bg-tertiary !important;
    }

    &.is-active {
      background: linear-gradient(135deg, $primary-color 0%, #096dd9 100%) !important;
      color: white !important;

      i {
        color: white;
      }
    }
  }

  &.el-menu--collapse {
    .el-menu-item {
      padding: 0 20px;

      i {
        margin-right: 0;
      }
    }
  }
}

.sidebar-footer {
  padding: 12px;
  border-top: 1px solid $dark-border;
}

.collapse-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 40px;
  border-radius: 8px;
  cursor: pointer;
  color: $dark-text-secondary;
  transition: all 0.2s ease;

  i {
    font-size: 20px;
  }

  &:hover {
    background: $dark-bg-tertiary;
    color: $dark-text-primary;
  }
}

// 主内容区
.main-container {
  flex: 1;
  margin-left: $sidebar-width;
  display: flex;
  flex-direction: column;
  min-height: 100vh;
  transition: margin-left 0.25s ease;

  .sidebar.collapsed ~ & {
    margin-left: $sidebar-collapsed-width;
  }
}

.collapsed ~ .main-container {
  margin-left: $sidebar-collapsed-width;
}

// ✨ 顶部栏 - 深色主题 @yutiansut @quantaxis
.top-header {
  height: $header-height;
  background: $dark-bg-secondary;
  border-bottom: 1px solid $dark-border;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 24px;
  position: sticky;
  top: 0;
  z-index: 100;
}

.header-left {
  .page-title {
    font-size: 18px;
    font-weight: 600;
    color: $dark-text-primary;
  }
}

.header-right {
  display: flex;
  align-items: center;
  gap: 20px;
}

.status-indicator {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 12px;
  background: rgba(82, 196, 26, 0.15);
  border: 1px solid rgba(82, 196, 26, 0.3);
  border-radius: 20px;

  .status-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;

    &.online {
      background: #52c41a;
      box-shadow: 0 0 8px rgba(82, 196, 26, 0.6);
      animation: pulse 2s infinite;
    }
  }

  .status-text {
    font-size: 13px;
    color: #52c41a;
    font-weight: 500;
  }
}

@keyframes pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.6; }
}

.user-info {
  display: flex;
  align-items: center;
  gap: 12px;
  cursor: pointer;
  padding: 6px 12px;
  border-radius: 8px;
  transition: background 0.2s ease;

  &:hover {
    background: $dark-bg-tertiary;
  }

  .user-avatar {
    background: linear-gradient(135deg, $primary-color 0%, #096dd9 100%);
    color: white;
    font-weight: 600;
  }

  .user-details {
    .user-name {
      font-size: 14px;
      font-weight: 500;
      color: $dark-text-primary;
      line-height: 1.2;
    }

    .user-role {
      margin-top: 2px;
    }
  }

  .el-icon-caret-bottom {
    color: $dark-text-secondary;
    font-size: 12px;
  }
}

// ✨ 用户下拉菜单 - 深色主题 @yutiansut @quantaxis
.user-dropdown-menu {
  background: $dark-bg-secondary !important;
  border: 1px solid $dark-border !important;

  .dropdown-user-info {
    display: flex;
    align-items: center;
    gap: 8px;
    color: $dark-text-secondary;
  }
}

// 内容区域
.content-wrapper {
  flex: 1;
  padding: 20px;
  overflow: auto;
}

// 页面过渡
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.2s ease;
}

.fade-enter,
.fade-leave-to {
  opacity: 0;
}

// ✨ 公告通知栏样式 @yutiansut @quantaxis
.announcement-bar {
  height: 40px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 20px;
  background: rgba($primary-color, 0.15);
  border-bottom: 1px solid rgba($primary-color, 0.3);
  transition: all 0.3s ease;

  &.announcement-warning {
    background: rgba(250, 173, 20, 0.15);
    border-bottom-color: rgba(250, 173, 20, 0.3);
  }

  &.announcement-error {
    background: rgba(245, 108, 108, 0.15);
    border-bottom-color: rgba(245, 108, 108, 0.3);
  }

  .announcement-content {
    display: flex;
    align-items: center;
    gap: 12px;
    cursor: pointer;
    flex: 1;
    overflow: hidden;

    &:hover .announcement-title {
      color: $primary-color;
    }
  }

  .announcement-icon {
    color: $primary-color;
    font-size: 16px;
    animation: bell-ring 2s infinite;
  }

  .announcement-label {
    flex-shrink: 0;
  }

  .announcement-title {
    color: $dark-text-primary;
    font-size: 14px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    transition: color 0.2s ease;
  }

  .announcement-indicator {
    color: $dark-text-secondary;
    font-size: 12px;
    flex-shrink: 0;
    padding: 2px 8px;
    background: rgba(255, 255, 255, 0.1);
    border-radius: 10px;
  }

  .announcement-close {
    cursor: pointer;
    padding: 4px;
    color: $dark-text-secondary;
    transition: color 0.2s ease;
    flex-shrink: 0;

    &:hover {
      color: $dark-text-primary;
    }

    i {
      font-size: 16px;
    }
  }
}

@keyframes bell-ring {
  0%, 50%, 100% { transform: rotate(0); }
  10%, 30% { transform: rotate(10deg); }
  20%, 40% { transform: rotate(-10deg); }
}

// 公告栏滑入动画
.slide-down-enter-active,
.slide-down-leave-active {
  transition: all 0.3s ease;
}

.slide-down-enter,
.slide-down-leave-to {
  transform: translateY(-100%);
  opacity: 0;
}

// 响应式
@media (max-width: 768px) {
  .sidebar {
    width: $sidebar-collapsed-width;
  }

  .main-container {
    margin-left: $sidebar-collapsed-width;
  }

  .content-wrapper {
    padding: 12px;
  }

  .status-indicator {
    display: none;
  }
}
</style>
