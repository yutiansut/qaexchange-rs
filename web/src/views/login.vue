<template>
  <div class="login-container">
    <el-card class="login-card">
      <div slot="header" class="card-header">
        <h2>用户登录</h2>
        <p class="subtitle">QAExchange 量化交易系统</p>
      </div>

      <el-form
        ref="loginForm"
        :model="loginForm"
        :rules="loginRules"
        label-width="80px"
        @submit.native.prevent="handleLogin"
      >
        <el-form-item label="用户名" prop="username">
          <el-input
            v-model="loginForm.username"
            placeholder="请输入用户名"
            prefix-icon="el-icon-user"
            clearable
            @keyup.enter.native="handleLogin"
          ></el-input>
        </el-form-item>

        <el-form-item label="密码" prop="password">
          <el-input
            v-model="loginForm.password"
            type="password"
            placeholder="请输入密码"
            prefix-icon="el-icon-lock"
            show-password
            clearable
            @keyup.enter.native="handleLogin"
          ></el-input>
        </el-form-item>

        <el-form-item>
          <el-checkbox v-model="loginForm.remember">记住我</el-checkbox>
        </el-form-item>

        <el-form-item>
          <el-button
            type="primary"
            :loading="loading"
            style="width: 100%"
            @click="handleLogin"
          >
            登录
          </el-button>
        </el-form-item>

        <div class="register-link">
          <span>还没有账号？</span>
          <el-button type="text" @click="goToRegister">立即注册</el-button>
        </div>
      </el-form>
    </el-card>
  </div>
</template>

<script>
import { mapActions } from 'vuex'

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
      loading: false
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

          // 调用 Vuex login action
          await this.login({ username, password })

          // 记住用户名
          if (remember) {
            localStorage.setItem('rememberedUsername', username)
          } else {
            localStorage.removeItem('rememberedUsername')
          }

          this.$message.success('登录成功')

          // 跳转到首页或之前访问的页面
          const redirect = this.$route.query.redirect || '/dashboard'
          this.$router.push(redirect)
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
    }
  }
}
</script>

<style scoped>
.login-container {
  display: flex;
  justify-content: center;
  align-items: center;
  min-height: 100vh;
  background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
  padding: 20px;
}

.login-card {
  width: 100%;
  max-width: 400px;
  box-shadow: 0 10px 40px rgba(0, 0, 0, 0.1);
}

.card-header {
  text-align: center;
}

.card-header h2 {
  margin: 0 0 8px 0;
  color: #303133;
  font-size: 28px;
  font-weight: 600;
}

.subtitle {
  margin: 0;
  color: #909399;
  font-size: 14px;
}

.register-link {
  text-align: center;
  margin-top: 20px;
  color: #606266;
}

.register-link span {
  margin-right: 5px;
}
</style>
