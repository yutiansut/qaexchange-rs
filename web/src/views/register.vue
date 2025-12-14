<template>
  <div class="register-container">
    <!-- 背景动画 -->
    <div class="bg-animation">
      <div class="grid-lines"></div>
      <div class="floating-shapes">
        <div class="shape shape-1"></div>
        <div class="shape shape-2"></div>
        <div class="shape shape-3"></div>
        <div class="shape shape-4"></div>
        <div class="shape shape-5"></div>
      </div>
    </div>

    <!-- 注册卡片 -->
    <div class="register-wrapper">
      <!-- 左侧信息区 -->
      <div class="info-section">
        <div class="info-content">
          <h2>加入 QAExchange</h2>
          <p>开启专业量化交易之旅</p>

          <div class="features">
            <div class="feature-item">
              <div class="feature-icon">
                <i class="el-icon-user"></i>
              </div>
              <div class="feature-text">
                <h4>一键开户</h4>
                <p>快速注册，即刻体验</p>
              </div>
            </div>

            <div class="feature-item">
              <div class="feature-icon">
                <i class="el-icon-s-data"></i>
              </div>
              <div class="feature-text">
                <h4>多账户管理</h4>
                <p>支持绑定多个交易账户</p>
              </div>
            </div>

            <div class="feature-item">
              <div class="feature-icon">
                <i class="el-icon-trophy"></i>
              </div>
              <div class="feature-text">
                <h4>策略回测</h4>
                <p>专业级策略分析工具</p>
              </div>
            </div>

            <div class="feature-item">
              <div class="feature-icon">
                <i class="el-icon-service"></i>
              </div>
              <div class="feature-text">
                <h4>专属服务</h4>
                <p>7x24 技术支持</p>
              </div>
            </div>
          </div>

          <!-- 统计数据 -->
          <div class="stats">
            <div class="stat-item">
              <span class="stat-number">10,000+</span>
              <span class="stat-label">活跃用户</span>
            </div>
            <div class="stat-item">
              <span class="stat-number">100μs</span>
              <span class="stat-label">交易延迟</span>
            </div>
            <div class="stat-item">
              <span class="stat-number">99.9%</span>
              <span class="stat-label">系统可用性</span>
            </div>
          </div>
        </div>
      </div>

      <!-- 右侧注册表单 -->
      <div class="register-card">
        <!-- Logo 区域 -->
        <div class="logo-section">
          <div class="logo">
            <svg viewBox="0 0 40 40" fill="none" xmlns="http://www.w3.org/2000/svg">
              <rect width="40" height="40" rx="8" fill="url(#gradient-reg)"/>
              <path d="M12 20L18 26L28 14" stroke="white" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"/>
              <defs>
                <linearGradient id="gradient-reg" x1="0" y1="0" x2="40" y2="40">
                  <stop stop-color="#52c41a"/>
                  <stop offset="1" stop-color="#389e0d"/>
                </linearGradient>
              </defs>
            </svg>
          </div>
          <h1 class="title">创建账号</h1>
          <p class="subtitle">注册 QAExchange 量化交易账户</p>
        </div>

        <!-- 注册表单 -->
        <el-form
          ref="registerForm"
          :model="registerForm"
          :rules="registerRules"
          class="register-form"
          @submit.native.prevent="handleRegister"
        >
          <el-form-item prop="username">
            <el-input
              v-model="registerForm.username"
              placeholder="用户名 (3-20个字符)"
              prefix-icon="el-icon-user"
              size="large"
              clearable
            ></el-input>
          </el-form-item>

          <el-form-item prop="email">
            <el-input
              v-model="registerForm.email"
              placeholder="邮箱地址"
              prefix-icon="el-icon-message"
              size="large"
              clearable
            ></el-input>
          </el-form-item>

          <el-form-item prop="phone">
            <el-input
              v-model="registerForm.phone"
              placeholder="手机号码"
              prefix-icon="el-icon-phone"
              size="large"
              clearable
            ></el-input>
          </el-form-item>

          <div class="password-row">
            <el-form-item prop="password" class="password-item">
              <el-input
                v-model="registerForm.password"
                type="password"
                placeholder="设置密码"
                prefix-icon="el-icon-lock"
                size="large"
                show-password
                clearable
              ></el-input>
            </el-form-item>

            <el-form-item prop="confirmPassword" class="password-item">
              <el-input
                v-model="registerForm.confirmPassword"
                type="password"
                placeholder="确认密码"
                prefix-icon="el-icon-lock"
                size="large"
                show-password
                clearable
                @keyup.enter.native="handleRegister"
              ></el-input>
            </el-form-item>
          </div>

          <!-- 密码强度指示器 -->
          <div class="password-strength" v-if="registerForm.password">
            <div class="strength-bar">
              <div class="strength-fill" :class="passwordStrengthClass" :style="{ width: passwordStrength + '%' }"></div>
            </div>
            <span class="strength-text" :class="passwordStrengthClass">{{ passwordStrengthText }}</span>
          </div>

          <!-- 用户协议 -->
          <div class="agreement">
            <el-checkbox v-model="registerForm.agree">
              我已阅读并同意
              <a href="#" class="link">《服务条款》</a>
              和
              <a href="#" class="link">《隐私政策》</a>
            </el-checkbox>
          </div>

          <el-button
            type="primary"
            :loading="loading"
            :disabled="!registerForm.agree"
            class="register-btn"
            @click="handleRegister"
          >
            <span v-if="!loading">
              <i class="el-icon-check"></i>
              立即注册
            </span>
            <span v-else>注册中...</span>
          </el-button>

          <div class="divider">
            <span>或使用以下方式注册</span>
          </div>

          <div class="social-register">
            <div class="social-btn" title="企业微信">
              <i class="el-icon-chat-dot-round"></i>
            </div>
            <div class="social-btn" title="钉钉">
              <i class="el-icon-message"></i>
            </div>
            <div class="social-btn" title="微信">
              <i class="el-icon-chat-line-round"></i>
            </div>
          </div>
        </el-form>

        <!-- 登录链接 -->
        <div class="login-section">
          <span>已有账号？</span>
          <a @click="goToLogin" class="login-link">立即登录</a>
        </div>
      </div>
    </div>

    <!-- 版权信息 -->
    <div class="footer">
      <p>@yutiansut @quantaxis · QAExchange v1.0</p>
    </div>
  </div>
</template>

<script>
// @yutiansut @quantaxis - 专业量化交易系统注册页面
import { register } from '@/api'

export default {
  name: 'Register',
  data() {
    const validateConfirmPassword = (rule, value, callback) => {
      if (value === '') {
        callback(new Error('请再次输入密码'))
      } else if (value !== this.registerForm.password) {
        callback(new Error('两次输入密码不一致'))
      } else {
        callback()
      }
    }

    return {
      registerForm: {
        username: '',
        email: '',
        phone: '',
        password: '',
        confirmPassword: '',
        agree: false
      },
      registerRules: {
        username: [
          { required: true, message: '请输入用户名', trigger: 'blur' },
          { min: 3, max: 20, message: '用户名长度在 3 到 20 个字符', trigger: 'blur' },
          { pattern: /^[a-zA-Z0-9_]+$/, message: '用户名只能包含字母、数字和下划线', trigger: 'blur' }
        ],
        email: [
          { required: true, message: '请输入邮箱', trigger: 'blur' },
          { type: 'email', message: '请输入正确的邮箱格式', trigger: 'blur' }
        ],
        phone: [
          { required: true, message: '请输入手机号', trigger: 'blur' },
          { pattern: /^1[3-9]\d{9}$/, message: '请输入正确的手机号', trigger: 'blur' }
        ],
        password: [
          { required: true, message: '请输入密码', trigger: 'blur' },
          { min: 6, max: 20, message: '密码长度在 6 到 20 个字符', trigger: 'blur' }
        ],
        confirmPassword: [
          { required: true, validator: validateConfirmPassword, trigger: 'blur' }
        ]
      },
      loading: false
    }
  },
  computed: {
    // 密码强度计算
    passwordStrength() {
      const password = this.registerForm.password
      if (!password) return 0

      let strength = 0

      // 长度评分
      if (password.length >= 6) strength += 20
      if (password.length >= 8) strength += 10
      if (password.length >= 12) strength += 10

      // 包含小写字母
      if (/[a-z]/.test(password)) strength += 15

      // 包含大写字母
      if (/[A-Z]/.test(password)) strength += 15

      // 包含数字
      if (/[0-9]/.test(password)) strength += 15

      // 包含特殊字符
      if (/[^a-zA-Z0-9]/.test(password)) strength += 15

      return Math.min(strength, 100)
    },
    passwordStrengthClass() {
      const strength = this.passwordStrength
      if (strength < 30) return 'weak'
      if (strength < 60) return 'medium'
      if (strength < 80) return 'strong'
      return 'very-strong'
    },
    passwordStrengthText() {
      const strength = this.passwordStrength
      if (strength < 30) return '弱'
      if (strength < 60) return '中等'
      if (strength < 80) return '强'
      return '非常强'
    }
  },
  methods: {
    handleRegister() {
      if (!this.registerForm.agree) {
        this.$message.warning('请先阅读并同意服务条款和隐私政策')
        return
      }

      this.$refs.registerForm.validate(async (valid) => {
        if (!valid) {
          return false
        }

        this.loading = true
        try {
          const { username, email, phone, password } = this.registerForm
          await register({ username, email, phone, password })

          this.$message.success('注册成功！正在跳转到登录页面...')

          // 延迟跳转，让用户看到成功消息
          setTimeout(() => {
            this.$router.push({ name: 'Login' })
          }, 1500)
        } catch (error) {
          const errorMsg = (error.response && error.response.data && error.response.data.error) || error.message || '注册失败'
          this.$message.error(errorMsg)
        } finally {
          this.loading = false
        }
      })
    },

    goToLogin() {
      this.$router.push({ name: 'Login' })
    }
  }
}
</script>

<style lang="scss" scoped>
// @yutiansut @quantaxis - 专业量化交易系统注册样式
$primary-color: #52c41a;
$primary-dark: #389e0d;
$blue-color: #1890ff;
$dark-bg: #0d1117;
$dark-bg-secondary: #161b22;

.register-container {
  min-height: 100vh;
  display: flex;
  flex-direction: column;
  justify-content: center;
  align-items: center;
  background: $dark-bg;
  position: relative;
  overflow: hidden;
  padding: 20px;
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
    linear-gradient(rgba(82, 196, 26, 0.03) 1px, transparent 1px),
    linear-gradient(90deg, rgba(82, 196, 26, 0.03) 1px, transparent 1px);
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
    background: linear-gradient(135deg, rgba(82, 196, 26, 0.1) 0%, rgba(56, 158, 13, 0.1) 100%);
    animation: float 20s ease-in-out infinite;
  }

  .shape-1 {
    width: 400px;
    height: 400px;
    top: -150px;
    left: -100px;
    animation-delay: 0s;
  }

  .shape-2 {
    width: 250px;
    height: 250px;
    bottom: -80px;
    right: -50px;
    animation-delay: -5s;
  }

  .shape-3 {
    width: 180px;
    height: 180px;
    top: 40%;
    right: 10%;
    animation-delay: -10s;
  }

  .shape-4 {
    width: 120px;
    height: 120px;
    bottom: 30%;
    left: 5%;
    animation-delay: -15s;
  }

  .shape-5 {
    width: 80px;
    height: 80px;
    top: 20%;
    left: 30%;
    animation-delay: -8s;
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

// 注册包装
.register-wrapper {
  display: flex;
  z-index: 1;
  max-width: 1000px;
  width: 95%;
  background: white;
  border-radius: 20px;
  box-shadow: 0 25px 60px -12px rgba(0, 0, 0, 0.35);
  overflow: hidden;
}

// 左侧信息区
.info-section {
  width: 420px;
  background: linear-gradient(135deg, $dark-bg 0%, $dark-bg-secondary 100%);
  padding: 48px 40px;
  display: flex;
  align-items: center;
  position: relative;
  overflow: hidden;

  &::before {
    content: '';
    position: absolute;
    top: -50%;
    left: -50%;
    width: 100%;
    height: 100%;
    background: radial-gradient(circle, rgba(82, 196, 26, 0.1) 0%, transparent 70%);
  }

  &::after {
    content: '';
    position: absolute;
    bottom: -30%;
    right: -30%;
    width: 80%;
    height: 80%;
    background: radial-gradient(circle, rgba(24, 144, 255, 0.08) 0%, transparent 70%);
  }
}

.info-content {
  position: relative;
  z-index: 1;
  width: 100%;

  h2 {
    font-size: 28px;
    font-weight: 700;
    color: white;
    margin: 0 0 12px;
  }

  > p {
    font-size: 15px;
    color: #8b949e;
    margin: 0 0 40px;
  }
}

.features {
  display: flex;
  flex-direction: column;
  gap: 24px;
  margin-bottom: 40px;
}

.feature-item {
  display: flex;
  align-items: flex-start;
  gap: 16px;

  .feature-icon {
    width: 44px;
    height: 44px;
    border-radius: 12px;
    background: rgba(82, 196, 26, 0.15);
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

// 统计数据
.stats {
  display: flex;
  justify-content: space-between;
  padding-top: 24px;
  border-top: 1px solid rgba(255, 255, 255, 0.1);
}

.stat-item {
  text-align: center;

  .stat-number {
    display: block;
    font-size: 20px;
    font-weight: 700;
    color: $primary-color;
    margin-bottom: 4px;
  }

  .stat-label {
    font-size: 12px;
    color: #8b949e;
  }
}

// 右侧注册卡片
.register-card {
  flex: 1;
  padding: 40px 48px;
  display: flex;
  flex-direction: column;
}

.logo-section {
  text-align: center;
  margin-bottom: 32px;

  .logo {
    width: 52px;
    height: 52px;
    margin: 0 auto 14px;

    svg {
      width: 100%;
      height: 100%;
    }
  }

  .title {
    font-size: 26px;
    font-weight: 700;
    color: #303133;
    margin: 0 0 8px;
  }

  .subtitle {
    font-size: 14px;
    color: #909399;
    margin: 0;
  }
}

// 表单样式
.register-form {
  flex: 1;

  ::v-deep .el-form-item {
    margin-bottom: 20px;
  }

  ::v-deep .el-input__inner {
    height: 46px;
    line-height: 46px;
    border-radius: 10px;
    border: 1px solid #e4e7ed;
    font-size: 14px;
    transition: all 0.2s ease;

    &:hover {
      border-color: $primary-color;
    }

    &:focus {
      border-color: $primary-color;
      box-shadow: 0 0 0 2px rgba(82, 196, 26, 0.1);
    }
  }

  ::v-deep .el-input__prefix {
    left: 12px;
    font-size: 17px;
    color: #909399;
  }

  ::v-deep .el-input--prefix .el-input__inner {
    padding-left: 40px;
  }
}

.password-row {
  display: flex;
  gap: 16px;

  .password-item {
    flex: 1;
  }
}

// 密码强度指示器
.password-strength {
  display: flex;
  align-items: center;
  gap: 12px;
  margin: -8px 0 16px;

  .strength-bar {
    flex: 1;
    height: 4px;
    background: #e4e7ed;
    border-radius: 2px;
    overflow: hidden;
  }

  .strength-fill {
    height: 100%;
    border-radius: 2px;
    transition: all 0.3s ease;

    &.weak {
      background: #f56c6c;
    }

    &.medium {
      background: #e6a23c;
    }

    &.strong {
      background: #67c23a;
    }

    &.very-strong {
      background: #52c41a;
    }
  }

  .strength-text {
    font-size: 12px;
    min-width: 50px;

    &.weak {
      color: #f56c6c;
    }

    &.medium {
      color: #e6a23c;
    }

    &.strong {
      color: #67c23a;
    }

    &.very-strong {
      color: #52c41a;
    }
  }
}

// 用户协议
.agreement {
  margin-bottom: 20px;

  ::v-deep .el-checkbox__label {
    font-size: 13px;
    color: #606266;
  }

  .link {
    color: $blue-color;
    text-decoration: none;

    &:hover {
      text-decoration: underline;
    }
  }
}

.register-btn {
  width: 100%;
  height: 48px;
  border-radius: 10px;
  font-size: 16px;
  font-weight: 600;
  background: linear-gradient(135deg, $primary-color 0%, $primary-dark 100%);
  border: none;
  transition: all 0.3s ease;

  i {
    margin-right: 6px;
  }

  &:hover:not(:disabled) {
    transform: translateY(-2px);
    box-shadow: 0 8px 20px rgba(82, 196, 26, 0.35);
  }

  &:active:not(:disabled) {
    transform: translateY(0);
  }

  &:disabled {
    background: #c0c4cc;
    cursor: not-allowed;
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
    font-size: 12px;
  }
}

.social-register {
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
      background: rgba(82, 196, 26, 0.05);
    }
  }
}

.login-section {
  text-align: center;
  margin-top: 24px;
  padding-top: 24px;
  border-top: 1px solid #e4e7ed;
  color: #909399;
  font-size: 14px;

  .login-link {
    color: $blue-color;
    font-weight: 500;
    cursor: pointer;

    &:hover {
      text-decoration: underline;
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
@media (max-width: 900px) {
  .register-wrapper {
    flex-direction: column;
    max-width: 480px;
  }

  .info-section {
    width: 100%;
    padding: 32px 24px;

    .features {
      display: none;
    }

    .stats {
      margin-top: 16px;
      padding-top: 16px;
    }
  }

  .info-content {
    h2 {
      font-size: 22px;
      margin-bottom: 8px;
    }

    > p {
      margin-bottom: 0;
    }
  }

  .register-card {
    padding: 32px 24px;
  }

  .password-row {
    flex-direction: column;
    gap: 0;
  }
}

@media (max-width: 480px) {
  .info-section {
    .stats {
      flex-direction: column;
      gap: 12px;
    }

    .stat-item {
      display: flex;
      justify-content: space-between;
      text-align: left;
    }
  }
}
</style>
