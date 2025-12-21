<template>
  <div class="accounts-page">
    <el-card>
      <div slot="header" class="card-header">
        <span>账户管理</span>
        <el-button type="primary" size="small" @click="showOpenDialog">
          <i class="el-icon-plus"></i> 开户
        </el-button>
      </div>

      <el-form :inline="true" size="small">
        <el-form-item label="用户ID">
          <el-input v-model="queryForm.userId" placeholder="输入用户ID" clearable />
        </el-form-item>
        <el-form-item>
          <el-button type="primary" @click="handleQuery">查询</el-button>
          <el-button @click="handleReset">重置</el-button>
        </el-form-item>
      </el-form>

      <el-table
        :data="accountList"
        border
        stripe
        height="500"
        v-loading="loading"
        style="width: 100%"
      >
        <el-table-column prop="account_id" label="账户ID" width="200" show-overflow-tooltip />
        <el-table-column prop="account_name" label="账户名称" width="150" />
        <el-table-column prop="account_type" label="账户类型" width="100" />
        <el-table-column prop="balance" label="总权益" width="120" align="right">
          <template slot-scope="scope">
            ¥{{ formatNumber(scope.row.balance) }}
          </template>
        </el-table-column>
        <el-table-column prop="available" label="可用资金" width="120" align="right">
          <template slot-scope="scope">
            ¥{{ formatNumber(scope.row.available) }}
          </template>
        </el-table-column>
        <el-table-column prop="margin" label="保证金" width="120" align="right">
          <template slot-scope="scope">
            ¥{{ formatNumber(scope.row.margin) }}
          </template>
        </el-table-column>
        <el-table-column prop="risk_ratio" label="风险率" width="100" align="right">
          <template slot-scope="scope">
            <el-tag :type="getRiskType(scope.row.risk_ratio)" size="mini">
              {{ (scope.row.risk_ratio * 100).toFixed(1) }}%
            </el-tag>
          </template>
        </el-table-column>
        <el-table-column label="操作" width="200" fixed="right">
          <template slot-scope="scope">
            <el-button type="text" size="small" @click="handleDeposit(scope.row)">
              存款
            </el-button>
            <el-button type="text" size="small" @click="handleWithdraw(scope.row)">
              取款
            </el-button>
            <el-button type="text" size="small" @click="handleViewDetail(scope.row)">
              详情
            </el-button>
          </template>
        </el-table-column>
      </el-table>
    </el-card>

    <!-- 开户对话框 -->
    <el-dialog title="开户" :visible.sync="openDialogVisible" width="500px">
      <el-form :model="openForm" :rules="openRules" ref="openForm" label-width="100px">
        <!-- 管理员模式：显示用户选择下拉框 -->
        <el-form-item v-if="isAdmin" label="选择用户" prop="user_id">
          <el-select
            v-model="openForm.user_id"
            filterable
            placeholder="请选择用户"
            style="width: 100%"
            @change="handleUserChange"
          >
            <el-option
              v-for="user in userList"
              :key="user.user_id"
              :label="`${user.username} (${user.real_name || '未实名'})`"
              :value="user.user_id"
            >
              <span style="float: left">{{ user.username }}</span>
              <span style="float: right; color: #8492a6; font-size: 13px">
                {{ user.real_name || user.user_id.slice(0, 8) }}
              </span>
            </el-option>
          </el-select>
        </el-form-item>

        <!-- 普通用户模式：显示当前用户信息（只读） -->
        <el-form-item v-else label="用户">
          <el-input :value="userInfo ? userInfo.username : currentUser" disabled />
        </el-form-item>

        <el-form-item label="用户名称" prop="user_name">
          <el-input v-model="openForm.user_name" placeholder="请输入用户名称" />
        </el-form-item>
        <el-form-item label="初始资金" prop="init_cash">
          <el-input-number
            v-model="openForm.init_cash"
            :min="0"
            :step="10000"
            style="width: 100%"
          />
        </el-form-item>
        <el-form-item label="账户类型" prop="account_type">
          <el-select v-model="openForm.account_type" style="width: 100%">
            <el-option label="个人账户" value="individual" />
            <el-option label="机构账户" value="institutional" />
          </el-select>
        </el-form-item>
        <el-form-item label="密码" prop="password">
          <el-input v-model="openForm.password" type="password" placeholder="请输入密码" />
        </el-form-item>
      </el-form>
      <div slot="footer">
        <el-button @click="openDialogVisible = false">取消</el-button>
        <el-button type="primary" @click="handleOpenAccount" :loading="submitting">
          确定
        </el-button>
      </div>
    </el-dialog>

    <!-- 存款对话框 -->
    <el-dialog title="存款" :visible.sync="depositDialogVisible" width="400px">
      <el-form :model="depositForm" ref="depositForm" label-width="80px">
        <el-form-item label="账户ID">
          <el-input v-model="depositForm.user_id" disabled />
        </el-form-item>
        <el-form-item label="存款金额" prop="amount">
          <el-input-number
            v-model="depositForm.amount"
            :min="0"
            :step="1000"
            style="width: 100%"
          />
        </el-form-item>
      </el-form>
      <div slot="footer">
        <el-button @click="depositDialogVisible = false">取消</el-button>
        <el-button type="primary" @click="handleDepositSubmit" :loading="submitting">
          确定
        </el-button>
      </div>
    </el-dialog>

    <!-- 取款对话框 -->
    <el-dialog title="取款" :visible.sync="withdrawDialogVisible" width="400px">
      <el-form :model="withdrawForm" ref="withdrawForm" label-width="80px">
        <el-form-item label="账户ID">
          <el-input v-model="withdrawForm.user_id" disabled />
        </el-form-item>
        <el-form-item label="取款金额" prop="amount">
          <el-input-number
            v-model="withdrawForm.amount"
            :min="0"
            :step="1000"
            style="width: 100%"
          />
        </el-form-item>
      </el-form>
      <div slot="footer">
        <el-button @click="withdrawDialogVisible = false">取消</el-button>
        <el-button type="primary" @click="handleWithdrawSubmit" :loading="submitting">
          确定
        </el-button>
      </div>
    </el-dialog>
  </div>
</template>

<script>
import { openAccount, queryAccount, deposit, withdraw, listUsers, getUserAccounts } from '@/api'
import { mapGetters } from 'vuex'

export default {
  name: 'Accounts',
  computed: {
    ...mapGetters(['currentUser', 'isAdmin', 'userInfo'])
  },
  data() {
    return {
      loading: false,
      submitting: false,
      accountList: [],
      queryForm: {
        userId: ''
      },
      openDialogVisible: false,
      openForm: {
        user_id: '',
        user_name: '',
        init_cash: 1000000,
        account_type: 'individual',
        password: ''
      },
      openRules: {
        user_id: [{ required: true, message: '请选择用户', trigger: 'change' }],
        user_name: [{ required: true, message: '请输入用户名称', trigger: 'blur' }],
        init_cash: [{ required: true, message: '请输入初始资金', trigger: 'blur' }],
        account_type: [{ required: true, message: '请选择账户类型', trigger: 'change' }],
        password: [{ required: true, message: '请输入密码', trigger: 'blur' }]
      },
      depositDialogVisible: false,
      depositForm: {
        user_id: '',
        amount: 0
      },
      withdrawDialogVisible: false,
      withdrawForm: {
        user_id: '',
        amount: 0
      },
      userList: []  // 用户列表（管理员使用）
    }
  },
  mounted() {
    this.loadAccounts()
  },
  methods: {
    formatNumber(num) {
      return (num || 0).toFixed(2).replace(/\B(?=(\d{3})+(?!\d))/g, ',')
    },

    getRiskType(ratio) {
      if (ratio >= 0.8) return 'danger'
      if (ratio >= 0.5) return 'warning'
      return 'success'
    },

    async loadAccounts() {
      if (!this.currentUser) {
        this.$message.warning('请先登录')
        return
      }

      this.loading = true
      try {
        const res = await getUserAccounts(this.currentUser)
        this.accountList = res.accounts || []
        this.loading = false
      } catch (error) {
        this.$message.error('加载账户信息失败: ' + ((error.response && error.response.data && error.response.data.error) || error.message))
        this.loading = false
      }
    },

    handleQuery() {
      if (this.queryForm.userId) {
        this.loading = true
        getUserAccounts(this.queryForm.userId)
          .then(res => {
            this.accountList = res.accounts || []
          })
          .catch(() => {
            this.$message.error('查询失败')
          })
          .finally(() => {
            this.loading = false
          })
      } else {
        this.loadAccounts()
      }
    },

    handleReset() {
      this.queryForm.userId = ''
      this.loadAccounts()
    },

    async showOpenDialog() {
      this.openDialogVisible = true

      // 如果是管理员，加载用户列表
      if (this.isAdmin) {
        try {
          // axios 拦截器已处理响应 @yutiansut @quantaxis
          const data = await listUsers()
          this.userList = (data && data.users) || []
        } catch (error) {
          this.$message.error('加载用户列表失败: ' + (error.message || '未知错误'))
        }
      } else {
        // 普通用户自动填充当前用户ID
        this.openForm.user_id = this.currentUser
      }

      this.$nextTick(() => {
        this.$refs.openForm.resetFields()
        // 重新设置用户ID（因为resetFields会清空）
        if (!this.isAdmin) {
          this.openForm.user_id = this.currentUser
        }
      })
    },

    handleUserChange(userId) {
      // 管理员选择用户后，可以自动填充一些信息
      const selectedUser = this.userList.find(u => u.user_id === userId)
      if (selectedUser && selectedUser.username) {
        this.openForm.user_name = selectedUser.username
      }
    },

    handleOpenAccount() {
      this.$refs.openForm.validate(valid => {
        if (valid) {
          this.submitting = true
          openAccount(this.openForm)
            .then(() => {
              this.$message.success('开户成功')
              this.openDialogVisible = false
              this.loadAccounts()
            })
            .catch(() => {
              this.$message.error('开户失败')
            })
            .finally(() => {
              this.submitting = false
            })
        }
      })
    },

    handleDeposit(row) {
      // 注意：后端API字段名是user_id，但实际传的是account_id
      this.depositForm.user_id = row.account_id
      this.depositForm.amount = 10000
      this.depositDialogVisible = true
    },

    handleDepositSubmit() {
      this.submitting = true
      deposit(this.depositForm)
        .then(() => {
          this.$message.success('存款成功')
          this.depositDialogVisible = false
          this.loadAccounts()
        })
        .catch(() => {
          this.$message.error('存款失败')
        })
        .finally(() => {
          this.submitting = false
        })
    },

    handleWithdraw(row) {
      // 注意：后端API字段名是user_id，但实际传的是account_id
      this.withdrawForm.user_id = row.account_id
      this.withdrawForm.amount = 10000
      this.withdrawDialogVisible = true
    },

    handleWithdrawSubmit() {
      this.submitting = true
      withdraw(this.withdrawForm)
        .then(() => {
          this.$message.success('取款成功')
          this.withdrawDialogVisible = false
          this.loadAccounts()
        })
        .catch(() => {
          this.$message.error('取款失败')
        })
        .finally(() => {
          this.submitting = false
        })
    },

    handleViewDetail(row) {
      this.$alert(JSON.stringify(row, null, 2), '账户详情', {
        confirmButtonText: '确定'
      })
    }
  }
}
</script>

<style lang="scss" scoped>
.accounts-page {
  padding: 20px;

  .card-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
}
</style>
