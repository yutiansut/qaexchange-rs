<template>
  <div class="login-container">
    <!-- 背景动画 -->
    <div class="bg-animation">
      <div class="grid-lines"></div>
      <div class="floating-shapes">
        <div class="shape shape-1"></div>
        <div class="shape shape-2"></div>
        <div class="shape shape-3"></div>
        <div class="shape shape-4"></div>
      </div>
    </div>

    <!-- 登录卡片 -->
    <div class="login-wrapper">
      <div class="login-card">
        <!-- Logo 区域 -->
        <div class="logo-section">
          <div class="logo">
            <svg viewBox="0 0 40 40" fill="none" xmlns="http://www.w3.org/2000/svg">
              <rect width="40" height="40" rx="8" fill="url(#gradient)"/>
              <path d="M10 20L15 25L30 10" stroke="white" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"/>
              <defs>
                <linearGradient id="gradient" x1="0" y1="0" x2="40" y2="40">
                  <stop stop-color="#1890ff"/>
                  <stop offset="1" stop-color="#096dd9"/>
                </linearGradient>
              </defs>
            </svg>
          </div>
          <h1 class="title">QAExchange</h1>
          <p class="subtitle">专业量化交易系统</p>
        </div>

        <!-- 登录表单 -->
        <el-form
          ref="loginForm"
          :model="loginForm"
          :rules="loginRules"
          class="login-form"
          @submit.native.prevent="handleLogin"
        >
          <el-form-item prop="username">
            <el-input
              v-model="loginForm.username"
              placeholder="请输入用户名"
              prefix-icon="el-icon-user"
              size="large"
              clearable
              @keyup.enter.native="handleLogin"
            ></el-input>
          </el-form-item>

          <el-form-item prop="password">
            <el-input
              v-model="loginForm.password"
              type="password"
              placeholder="请输入密码"
              prefix-icon="el-icon-lock"
              size="large"
              show-password
              clearable
              @keyup.enter.native="handleLogin"
            ></el-input>
          </el-form-item>

          <div class="form-options">
            <el-checkbox v-model="loginForm.remember">记住登录</el-checkbox>
            <a href="#" class="forgot-link">忘记密码？</a>
          </div>

          <el-button
            type="primary"
            :loading="loading"
            class="login-btn"
            @click="handleLogin"
          >
            <span v-if="!loading">登录系统</span>
            <span v-else>登录中...</span>
          </el-button>

          <div class="divider">
            <span>其他方式</span>
          </div>

          <div class="social-login">
            <div class="social-btn" title="企业微信">
              <i class="el-icon-chat-dot-round"></i>
            </div>
            <div class="social-btn" title="钉钉">
              <i class="el-icon-message"></i>
            </div>
            <div class="social-btn" title="API Token">
              <i class="el-icon-key"></i>
            </div>
          </div>
        </el-form>

        <!-- 注册链接 -->
        <div class="register-section">
          <span>还没有账号？</span>
          <a @click="goToRegister" class="register-link">立即注册</a>
        </div>
      </div>

      <!-- 右侧信息 -->
      <div class="info-section">
        <div class="info-content">
          <h2>安全 · 高效 · 专业</h2>
          <p>为量化交易者打造的专业级交易系统</p>

          <div class="features">
            <div class="feature-item">
              <div class="feature-icon">
                <i class="el-icon-odometer"></i>
              </div>
              <div class="feature-text">
                <h4>超低延迟</h4>
                <p>P99 < 100μs 订单处理</p>
              </div>
            </div>

            <div class="feature-item">
              <div class="feature-icon">
                <i class="el-icon-lock"></i>
              </div>
              <div class="feature-text">
                <h4>安全可靠</h4>
                <p>多重风控保障资金安全</p>
              </div>
            </div>

            <div class="feature-item">
              <div class="feature-icon">
                <i class="el-icon-data-analysis"></i>
              </div>
              <div class="feature-text">
                <h4>实时数据</h4>
                <p>毫秒级行情推送</p>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- 版权信息 -->
    <div class="footer">
      <p>@yutiansut @quantaxis · QAExchange v1.0</p>
    </div>

    <!-- ✨ 登录公告弹窗 @yutiansut @quantaxis -->
    <el-dialog
      title="系统公告"
      :visible.sync="showAnnouncementDialog"
      width="600px"
      :close-on-click-modal="false"
      custom-class="announcement-dialog"
      @close="closeAnnouncementDialog"
    >
      <div class="announcement-list">
        <div
          v-for="(announcement, index) in loginAnnouncements"
          :key="announcement.id"
          class="announcement-item"
        >
          <div class="announcement-header">
            <el-tag :type="getPriorityType(announcement.priority)" size="small">
              {{ announcement.announcement_type || '公告' }}
            </el-tag>
            <span class="announcement-date">
              {{ formatDate(announcement.created_at) }}
            </span>
          </div>
          <h4 class="announcement-title">{{ announcement.title }}</h4>
          <div class="announcement-content" v-html="announcement.content"></div>
          <el-divider v-if="index < loginAnnouncements.length - 1"></el-divider>
        </div>
      </div>
      <span slot="footer" class="dialog-footer">
        <el-button type="primary" @click="closeAnnouncementDialog">
          我知道了，进入系统
        </el-button>
      </span>
    </el-dialog>
  </div>
</template>

<script>
// @yutiansut @quantaxis - 专业量化交易系统登录页面
import { mapActions } from 'vuex'
import { queryAnnouncements } from '@/api'

export default {
  name: 'Login',
  data() {
    return {
      loginForm: {
        username: localStorage.getItem('rememberedUsername') || '',
        password: '',
        remember: !!localStorage.getItem('rememberedUsername')
      },
      loginRules: {
        username: [
          { required: true, message: '请输入用户名', trigger: 'blur' }
        ],
        password: [
          { required: true, message: '请输入密码', trigger: 'blur' }
        ]
      },
      loading: false,
      // ✨ 登录公告弹窗 @yutiansut @quantaxis
      showAnnouncementDialog: false,
      loginAnnouncements: []
    }
  },
  methods: {
    ...mapActions(['login']),

    handleLogin() {
      this.$refs.loginForm.validate(async (valid) => {
        if (!valid) {
          return false
        }

        this.loading = true
        try {
          const { username, password, remember } = this.loginForm

          await this.login({ username, password })

          if (remember) {
            localStorage.setItem('rememberedUsername', username)
          } else {
            localStorage.removeItem('rememberedUsername')
          }

          this.$message.success('登录成功')

          // ✨ 登录成功后检查公告 @yutiansut @quantaxis
          await this.checkLoginAnnouncements()
        } catch (error) {
          const errorMsg = (error.response && error.response.data && error.response.data.error) || error.message || '登录失败'
          this.$message.error(errorMsg)
        } finally {
          this.loading = false
        }
      })
    },

    goToRegister() {
      this.$router.push({ name: 'Register' })
    },

    // ✨ 检查登录时的公告 @yutiansut @quantaxis
    async checkLoginAnnouncements() {
      try {
        const res = await queryAnnouncements({ active_only: true, page_size: 10 })
        const now = Date.now()
        // 过滤有效期内的公告
        const validAnnouncements = (res.announcements || []).filter(a => {
          const from = a.effective_from ? a.effective_from * 1000 : 0
          const until = a.effective_until ? a.effective_until * 1000 : Number.MAX_SAFE_INTEGER
          return now >= from && now <= until
        })
        // 按优先级排序
        const priorityOrder = { 'Urgent': 0, 'High': 1, 'Normal': 2, 'Low': 3 }
        validAnnouncements.sort((a, b) =>
          (priorityOrder[a.priority] || 3) - (priorityOrder[b.priority] || 3)
        )

        if (validAnnouncements.length > 0) {
          this.loginAnnouncements = validAnnouncements
          this.showAnnouncementDialog = true
        } else {
          this.navigateToDashboard()
        }
      } catch (error) {
        console.error('加载公告失败', error)
        this.navigateToDashboard()
      }
    },

    // ✨ 关闭公告弹窗并跳转 @yutiansut @quantaxis
    closeAnnouncementDialog() {
      this.showAnnouncementDialog = false
      this.navigateToDashboard()
    },

    // ✨ 跳转到首页 @yutiansut @quantaxis
    navigateToDashboard() {
      const redirect = this.$route.query.redirect || '/dashboard'
      this.$router.push(redirect)
    },

    // ✨ 格式化日期 @yutiansut @quantaxis
    formatDate(timestamp) {
      if (!timestamp) return ''
      const date = new Date(timestamp * 1000)
      return date.toLocaleDateString('zh-CN', {
        year: 'numeric',
        month: '2-digit',
        day: '2-digit',
        hour: '2-digit',
        minute: '2-digit'
      })
    },

    // ✨ 获取公告优先级类型 @yutiansut @quantaxis
    getPriorityType(priority) {
      const map = { 'Urgent': 'danger', 'High': 'warning', 'Normal': 'info', 'Low': '' }
      return map[priority] || 'info'
    }
  }
}
</script>

<style lang="scss" scoped>
// @yutiansut @quantaxis - 专业量化交易系统登录样式（深色主题）
$primary-color: #1890ff;
$primary-dark: #096dd9;
$dark-bg: #0d1117;
$dark-bg-secondary: #161b22;
$dark-bg-tertiary: #21262d;
$dark-border: #30363d;
$dark-text-primary: #f0f6fc;
$dark-text-secondary: #8b949e;
$dark-text-muted: #6e7681;

.login-container {
  min-height: 100vh;
  display: flex;
  flex-direction: column;
  justify-content: center;
  align-items: center;
  background: $dark-bg;
  position: relative;
  overflow: hidden;
}

// 背景动画
.bg-animation {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  z-index: 0;
}

.grid-lines {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background-image:
    linear-gradient(rgba(24, 144, 255, 0.03) 1px, transparent 1px),
    linear-gradient(90deg, rgba(24, 144, 255, 0.03) 1px, transparent 1px);
  background-size: 50px 50px;
}

.floating-shapes {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;

  .shape {
    position: absolute;
    border-radius: 50%;
    background: linear-gradient(135deg, rgba(24, 144, 255, 0.1) 0%, rgba(9, 109, 217, 0.1) 100%);
    animation: float 20s ease-in-out infinite;
  }

  .shape-1 {
    width: 300px;
    height: 300px;
    top: -100px;
    right: -50px;
    animation-delay: 0s;
  }

  .shape-2 {
    width: 200px;
    height: 200px;
    bottom: -50px;
    left: -50px;
    animation-delay: -5s;
  }

  .shape-3 {
    width: 150px;
    height: 150px;
    top: 50%;
    left: 10%;
    animation-delay: -10s;
  }

  .shape-4 {
    width: 100px;
    height: 100px;
    bottom: 20%;
    right: 15%;
    animation-delay: -15s;
  }
}

@keyframes float {
  0%, 100% {
    transform: translateY(0) rotate(0deg);
  }
  50% {
    transform: translateY(-30px) rotate(180deg);
  }
}

// ✨ 登录包装 - 深色主题 @yutiansut @quantaxis
.login-wrapper {
  display: flex;
  z-index: 1;
  max-width: 900px;
  width: 90%;
  background: $dark-bg-secondary;
  border: 1px solid $dark-border;
  border-radius: 16px;
  box-shadow: 0 25px 50px -12px rgba(0, 0, 0, 0.5);
  overflow: hidden;
}

// 登录卡片
.login-card {
  flex: 1;
  padding: 48px;
  display: flex;
  flex-direction: column;
}

// ✨ Logo 区域 - 深色主题 @yutiansut @quantaxis
.logo-section {
  text-align: center;
  margin-bottom: 36px;

  .logo {
    width: 56px;
    height: 56px;
    margin: 0 auto 16px;

    svg {
      width: 100%;
      height: 100%;
    }
  }

  .title {
    font-size: 28px;
    font-weight: 700;
    color: $dark-text-primary;
    margin: 0 0 8px;
  }

  .subtitle {
    font-size: 14px;
    color: $dark-text-secondary;
    margin: 0;
  }
}

// ✨ 表单样式 - 深色主题 @yutiansut @quantaxis
.login-form {
  flex: 1;

  ::v-deep .el-form-item {
    margin-bottom: 24px;
  }

  ::v-deep .el-input__inner {
    height: 48px;
    line-height: 48px;
    border-radius: 8px;
    border: 1px solid $dark-border;
    background-color: $dark-bg-tertiary;
    color: $dark-text-primary;
    font-size: 15px;
    transition: all 0.2s ease;

    &::placeholder {
      color: $dark-text-muted;
    }

    &:hover {
      border-color: $primary-color;
    }

    &:focus {
      border-color: $primary-color;
      box-shadow: 0 0 0 2px rgba(24, 144, 255, 0.2);
    }
  }

  ::v-deep .el-input__prefix {
    left: 12px;
    font-size: 18px;
    color: $dark-text-secondary;
  }

  ::v-deep .el-input--prefix .el-input__inner {
    padding-left: 40px;
  }
}

.form-options {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 24px;

  .forgot-link {
    color: $primary-color;
    font-size: 13px;
    text-decoration: none;

    &:hover {
      text-decoration: underline;
    }
  }
}

.login-btn {
  width: 100%;
  height: 48px;
  border-radius: 8px;
  font-size: 16px;
  font-weight: 600;
  background: linear-gradient(135deg, $primary-color 0%, $primary-dark 100%);
  border: none;
  transition: all 0.3s ease;

  &:hover {
    transform: translateY(-2px);
    box-shadow: 0 8px 20px rgba(24, 144, 255, 0.3);
  }

  &:active {
    transform: translateY(0);
  }
}

.divider {
  display: flex;
  align-items: center;
  margin: 24px 0;

  &::before, &::after {
    content: '';
    flex: 1;
    height: 1px;
    background: #e4e7ed;
  }

  span {
    padding: 0 16px;
    color: #909399;
    font-size: 13px;
  }
}

.social-login {
  display: flex;
  justify-content: center;
  gap: 16px;

  .social-btn {
    width: 44px;
    height: 44px;
    border-radius: 50%;
    border: 1px solid #e4e7ed;
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    transition: all 0.2s ease;
    color: #909399;

    i {
      font-size: 20px;
    }

    &:hover {
      border-color: $primary-color;
      color: $primary-color;
      background: rgba(24, 144, 255, 0.05);
    }
  }
}

.register-section {
  text-align: center;
  margin-top: 24px;
  padding-top: 24px;
  border-top: 1px solid #e4e7ed;
  color: #909399;
  font-size: 14px;

  .register-link {
    color: $primary-color;
    font-weight: 500;
    cursor: pointer;

    &:hover {
      text-decoration: underline;
    }
  }
}

// 右侧信息区
.info-section {
  width: 380px;
  background: linear-gradient(135deg, $dark-bg 0%, $dark-bg-secondary 100%);
  padding: 48px;
  display: flex;
  align-items: center;
  position: relative;
  overflow: hidden;

  &::before {
    content: '';
    position: absolute;
    top: -50%;
    right: -50%;
    width: 100%;
    height: 100%;
    background: radial-gradient(circle, rgba(24, 144, 255, 0.1) 0%, transparent 70%);
  }
}

.info-content {
  position: relative;
  z-index: 1;

  h2 {
    font-size: 24px;
    font-weight: 700;
    color: white;
    margin: 0 0 12px;
  }

  > p {
    font-size: 14px;
    color: #8b949e;
    margin: 0 0 36px;
  }
}

.features {
  display: flex;
  flex-direction: column;
  gap: 24px;
}

.feature-item {
  display: flex;
  align-items: flex-start;
  gap: 16px;

  .feature-icon {
    width: 44px;
    height: 44px;
    border-radius: 10px;
    background: rgba(24, 144, 255, 0.15);
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;

    i {
      font-size: 22px;
      color: $primary-color;
    }
  }

  .feature-text {
    h4 {
      font-size: 15px;
      font-weight: 600;
      color: white;
      margin: 0 0 4px;
    }

    p {
      font-size: 13px;
      color: #8b949e;
      margin: 0;
    }
  }
}

// 底部版权
.footer {
  position: absolute;
  bottom: 24px;
  color: #6e7681;
  font-size: 12px;
  z-index: 1;
}

// 响应式
@media (max-width: 768px) {
  .login-wrapper {
    flex-direction: column;
    max-width: 400px;
  }

  .info-section {
    display: none;
  }

  .login-card {
    padding: 32px 24px;
  }
}

// ✨ 公告弹窗样式 @yutiansut @quantaxis
::v-deep .announcement-dialog {
  background: $dark-bg-secondary !important;
  border: 1px solid $dark-border;
  border-radius: 12px;

  .el-dialog__header {
    border-bottom: 1px solid $dark-border;
    padding: 16px 20px;

    .el-dialog__title {
      color: $dark-text-primary;
      font-size: 16px;
      font-weight: 600;
    }

    .el-dialog__headerbtn .el-dialog__close {
      color: $dark-text-secondary;
    }
  }

  .el-dialog__body {
    padding: 20px;
    max-height: 400px;
    overflow-y: auto;
  }

  .el-dialog__footer {
    border-top: 1px solid $dark-border;
    padding: 16px 20px;
  }
}

.announcement-list {
  .announcement-item {
    .announcement-header {
      display: flex;
      align-items: center;
      gap: 12px;
      margin-bottom: 8px;
    }

    .announcement-date {
      color: $dark-text-muted;
      font-size: 12px;
    }

    .announcement-title {
      color: $dark-text-primary;
      font-size: 15px;
      font-weight: 600;
      margin: 0 0 10px;
    }

    .announcement-content {
      color: $dark-text-secondary;
      font-size: 14px;
      line-height: 1.6;
    }

    ::v-deep .el-divider {
      background-color: $dark-border;
      margin: 16px 0;
    }
  }
}
</style>
