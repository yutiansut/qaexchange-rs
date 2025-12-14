<template>
  <div class="password-container">
    <!-- 页面标题 -->
    <div class="page-header">
      <h2>密码管理</h2>
    </div>

    <el-row :gutter="20">
      <!-- 修改交易密码 -->
      <el-col :span="12">
        <el-card class="password-card">
          <div slot="header">
            <span>修改交易密码</span>
          </div>
          <el-form :model="tradingPasswordForm" :rules="passwordRules" ref="tradingPasswordForm" label-width="100px">
            <el-form-item label="账户ID" prop="account_id">
              <el-select v-model="tradingPasswordForm.account_id" placeholder="请选择账户" style="width: 100%">
                <el-option
                  v-for="account in accounts"
                  :key="account.account_id"
                  :label="account.account_id"
                  :value="account.account_id"
                ></el-option>
              </el-select>
            </el-form-item>
            <el-form-item label="旧密码" prop="old_password">
              <el-input
                type="password"
                v-model="tradingPasswordForm.old_password"
                placeholder="请输入旧密码"
                show-password
              ></el-input>
            </el-form-item>
            <el-form-item label="新密码" prop="new_password">
              <el-input
                type="password"
                v-model="tradingPasswordForm.new_password"
                placeholder="请输入新密码"
                show-password
              ></el-input>
            </el-form-item>
            <el-form-item label="确认密码" prop="confirm_password">
              <el-input
                type="password"
                v-model="tradingPasswordForm.confirm_password"
                placeholder="请再次输入新密码"
                show-password
              ></el-input>
            </el-form-item>
            <el-form-item>
              <el-button type="primary" @click="submitTradingPassword" :loading="submitting">修改交易密码</el-button>
            </el-form-item>
          </el-form>
        </el-card>
      </el-col>

      <!-- 修改资金密码 -->
      <el-col :span="12">
        <el-card class="password-card">
          <div slot="header">
            <span>修改资金密码</span>
          </div>
          <el-form :model="fundPasswordForm" :rules="passwordRules" ref="fundPasswordForm" label-width="100px">
            <el-form-item label="账户ID" prop="account_id">
              <el-select v-model="fundPasswordForm.account_id" placeholder="请选择账户" style="width: 100%">
                <el-option
                  v-for="account in accounts"
                  :key="account.account_id"
                  :label="account.account_id"
                  :value="account.account_id"
                ></el-option>
              </el-select>
            </el-form-item>
            <el-form-item label="旧密码" prop="old_password">
              <el-input
                type="password"
                v-model="fundPasswordForm.old_password"
                placeholder="请输入旧密码"
                show-password
              ></el-input>
            </el-form-item>
            <el-form-item label="新密码" prop="new_password">
              <el-input
                type="password"
                v-model="fundPasswordForm.new_password"
                placeholder="请输入新密码"
                show-password
              ></el-input>
            </el-form-item>
            <el-form-item label="确认密码" prop="confirm_password">
              <el-input
                type="password"
                v-model="fundPasswordForm.confirm_password"
                placeholder="请再次输入新密码"
                show-password
              ></el-input>
            </el-form-item>
            <el-form-item>
              <el-button type="primary" @click="submitFundPassword" :loading="submitting">修改资金密码</el-button>
            </el-form-item>
          </el-form>
        </el-card>
      </el-col>
    </el-row>

    <!-- 密码安全提示 -->
    <el-card class="tips-card">
      <div slot="header">
        <span>密码安全提示</span>
      </div>
      <ul class="tips-list">
        <li>交易密码用于下单、撤单等交易操作的验证</li>
        <li>资金密码用于出入金、银期转账等资金操作的验证</li>
        <li>密码长度建议6-20位，包含数字和字母</li>
        <li>请勿使用过于简单的密码，如123456、password等</li>
        <li>请勿将密码告诉他人或在不安全的环境下输入密码</li>
        <li>如忘记密码，请联系客服进行重置</li>
      </ul>
    </el-card>
  </div>
</template>

<script>
/**
 * 密码管理页面 @yutiansut @quantaxis
 */
import { changePassword, getUserAccounts } from '@/api'

export default {
  name: 'PasswordManagement',

  data() {
    const validateConfirmPassword = (rule, value, callback) => {
      const formName = rule.field.includes('trading') ? 'tradingPasswordForm' : 'fundPasswordForm'
      const form = formName === 'tradingPasswordForm' ? this.tradingPasswordForm : this.fundPasswordForm
      if (value !== form.new_password) {
        callback(new Error('两次输入的密码不一致'))
      } else {
        callback()
      }
    }

    return {
      submitting: false,
      accounts: [],
      tradingPasswordForm: {
        account_id: '',
        old_password: '',
        new_password: '',
        confirm_password: ''
      },
      fundPasswordForm: {
        account_id: '',
        old_password: '',
        new_password: '',
        confirm_password: ''
      },
      passwordRules: {
        account_id: [{ required: true, message: '请选择账户', trigger: 'change' }],
        old_password: [{ required: true, message: '请输入旧密码', trigger: 'blur' }],
        new_password: [
          { required: true, message: '请输入新密码', trigger: 'blur' },
          { min: 6, max: 20, message: '密码长度6-20位', trigger: 'blur' }
        ],
        confirm_password: [
          { required: true, message: '请再次输入新密码', trigger: 'blur' },
          { validator: validateConfirmPassword, trigger: 'blur' }
        ]
      }
    }
  },

  created() {
    this.loadAccounts()
  },

  methods: {
    async loadAccounts() {
      try {
        const userId = localStorage.getItem('userId')
        if (!userId) return
        const res = await getUserAccounts(userId)
        if (res.success) {
          this.accounts = res.data || []
          if (this.accounts.length > 0) {
            this.tradingPasswordForm.account_id = this.accounts[0].account_id
            this.fundPasswordForm.account_id = this.accounts[0].account_id
          }
        }
      } catch (err) {
        console.error('加载账户列表失败:', err)
      }
    },

    async submitTradingPassword() {
      this.$refs.tradingPasswordForm.validate(async (valid) => {
        if (!valid) return

        this.submitting = true
        try {
          const data = {
            account_id: this.tradingPasswordForm.account_id,
            old_password: this.tradingPasswordForm.old_password,
            new_password: this.tradingPasswordForm.new_password,
            password_type: 'Trading'
          }
          const res = await changePassword(data)
          if (res.success) {
            this.$message.success('交易密码修改成功')
            this.tradingPasswordForm.old_password = ''
            this.tradingPasswordForm.new_password = ''
            this.tradingPasswordForm.confirm_password = ''
          } else {
            this.$message.error(res.error || '修改失败')
          }
        } catch (err) {
          console.error('修改交易密码失败:', err)
          this.$message.error('修改交易密码失败')
        } finally {
          this.submitting = false
        }
      })
    },

    async submitFundPassword() {
      this.$refs.fundPasswordForm.validate(async (valid) => {
        if (!valid) return

        this.submitting = true
        try {
          const data = {
            account_id: this.fundPasswordForm.account_id,
            old_password: this.fundPasswordForm.old_password,
            new_password: this.fundPasswordForm.new_password,
            password_type: 'Fund'
          }
          const res = await changePassword(data)
          if (res.success) {
            this.$message.success('资金密码修改成功')
            this.fundPasswordForm.old_password = ''
            this.fundPasswordForm.new_password = ''
            this.fundPasswordForm.confirm_password = ''
          } else {
            this.$message.error(res.error || '修改失败')
          }
        } catch (err) {
          console.error('修改资金密码失败:', err)
          this.$message.error('修改资金密码失败')
        } finally {
          this.submitting = false
        }
      })
    }
  }
}
</script>

<style scoped>
.password-container {
  padding: 20px;
}

.page-header {
  margin-bottom: 20px;
}

.page-header h2 {
  margin: 0;
  color: #303133;
}

.password-card {
  margin-bottom: 20px;
}

.tips-card {
  margin-top: 20px;
}

.tips-list {
  margin: 0;
  padding-left: 20px;
  color: #606266;
}

.tips-list li {
  margin-bottom: 10px;
  line-height: 1.6;
}
</style>
