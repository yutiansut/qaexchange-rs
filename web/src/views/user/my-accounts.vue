<template>
  <div class="my-accounts-container">
    <el-card class="header-card">
      <div slot="header" class="card-header">
        <h3>我的交易账户</h3>
        <el-button
          type="primary"
          icon="el-icon-plus"
          @click="showCreateDialog = true"
        >
          创建新账户
        </el-button>
      </div>

      <!-- 账户统计 -->
      <el-row :gutter="20" class="stats-row">
        <el-col :span="6">
          <div class="stat-card">
            <div class="stat-label">账户总数</div>
            <div class="stat-value">{{ accounts.length }}</div>
          </div>
        </el-col>
        <el-col :span="6">
          <div class="stat-card">
            <div class="stat-label">总资产</div>
            <div class="stat-value">{{ totalBalance.toFixed(2) }}</div>
          </div>
        </el-col>
        <el-col :span="6">
          <div class="stat-card">
            <div class="stat-label">可用资金</div>
            <div class="stat-value">{{ totalAvailable.toFixed(2) }}</div>
          </div>
        </el-col>
        <el-col :span="6">
          <div class="stat-card">
            <div class="stat-label">占用保证金</div>
            <div class="stat-value">{{ totalMargin.toFixed(2) }}</div>
          </div>
        </el-col>
      </el-row>
    </el-card>

    <!-- 账户列表 -->
    <el-card class="accounts-card">
      <div v-loading="loading">
        <el-table
          :data="accounts"
          style="width: 100%"
          :default-sort="{ prop: 'created_at', order: 'descending' }"
        >
          <el-table-column
            prop="account_id"
            label="账户ID"
            width="280"
            show-overflow-tooltip
          >
            <template slot-scope="scope">
              <el-tag size="small" type="info">{{ scope.row.account_id }}</el-tag>
            </template>
          </el-table-column>

          <el-table-column
            prop="account_name"
            label="账户名称"
            min-width="150"
          />

          <el-table-column
            prop="account_type"
            label="账户类型"
            width="120"
          >
            <template slot-scope="scope">
              <el-tag
                :type="getAccountTypeTag(scope.row.account_type)"
                size="small"
              >
                {{ getAccountTypeLabel(scope.row.account_type) }}
              </el-tag>
            </template>
          </el-table-column>

          <el-table-column
            prop="balance"
            label="总资产"
            width="130"
            align="right"
          >
            <template slot-scope="scope">
              <span class="money">{{ scope.row.balance.toFixed(2) }}</span>
            </template>
          </el-table-column>

          <el-table-column
            prop="available"
            label="可用资金"
            width="130"
            align="right"
          >
            <template slot-scope="scope">
              <span class="money">{{ scope.row.available.toFixed(2) }}</span>
            </template>
          </el-table-column>

          <el-table-column
            prop="margin"
            label="占用保证金"
            width="130"
            align="right"
          >
            <template slot-scope="scope">
              <span class="money">{{ scope.row.margin.toFixed(2) }}</span>
            </template>
          </el-table-column>

          <el-table-column
            prop="risk_ratio"
            label="风险率"
            width="120"
            align="center"
          >
            <template slot-scope="scope">
              <el-tag
                :type="getRiskRatioTag(scope.row.risk_ratio)"
                size="small"
              >
                {{ (scope.row.risk_ratio * 100).toFixed(1) }}%
              </el-tag>
            </template>
          </el-table-column>

          <el-table-column
            prop="created_at"
            label="创建时间"
            width="180"
          >
            <template slot-scope="scope">
              {{ formatTimestamp(scope.row.created_at) }}
            </template>
          </el-table-column>

          <el-table-column
            label="操作"
            width="180"
            fixed="right"
          >
            <template slot-scope="scope">
              <el-button
                type="text"
                size="small"
                @click="handleViewAccount(scope.row)"
              >
                查看详情
              </el-button>
              <el-button
                type="text"
                size="small"
                @click="handleDeposit(scope.row)"
              >
                入金
              </el-button>
              <el-button
                type="text"
                size="small"
                @click="handleWithdraw(scope.row)"
              >
                出金
              </el-button>
            </template>
          </el-table-column>
        </el-table>

        <div v-if="accounts.length === 0" class="empty-state">
          <el-empty description="暂无交易账户">
            <el-button
              type="primary"
              @click="showCreateDialog = true"
            >
              创建第一个账户
            </el-button>
          </el-empty>
        </div>
      </div>
    </el-card>

    <!-- 创建账户对话框 -->
    <el-dialog
      title="创建交易账户"
      :visible.sync="showCreateDialog"
      width="500px"
      :close-on-click-modal="false"
    >
      <el-form
        ref="createForm"
        :model="createForm"
        :rules="createRules"
        label-width="100px"
      >
        <el-form-item label="账户名称" prop="account_name">
          <el-input
            v-model="createForm.account_name"
            placeholder="请输入账户名称"
            clearable
          />
        </el-form-item>

        <el-form-item label="账户类型" prop="account_type">
          <el-select
            v-model="createForm.account_type"
            placeholder="请选择账户类型"
            style="width: 100%"
          >
            <el-option label="个人账户" value="individual" />
            <el-option label="机构账户" value="institutional" />
            <el-option label="做市商账户" value="market_maker" />
          </el-select>
        </el-form-item>

        <el-form-item label="初始资金" prop="init_cash">
          <el-input-number
            v-model="createForm.init_cash"
            :min="10000"
            :max="100000000"
            :step="10000"
            style="width: 100%"
          />
          <div class="form-tip">最低 10,000 元</div>
        </el-form-item>
      </el-form>

      <div slot="footer">
        <el-button @click="showCreateDialog = false">取消</el-button>
        <el-button
          type="primary"
          :loading="creating"
          @click="handleCreateAccount"
        >
          创建
        </el-button>
      </div>
    </el-dialog>

    <!-- 入金对话框 -->
    <el-dialog
      title="账户入金"
      :visible.sync="showDepositDialog"
      width="400px"
    >
      <el-form ref="depositForm" :model="depositForm" label-width="80px">
        <el-form-item label="账户">
          <el-input :value="currentAccount.account_name" disabled />
        </el-form-item>
        <el-form-item label="入金金额" prop="amount">
          <el-input-number
            v-model="depositForm.amount"
            :min="100"
            :step="100"
            style="width: 100%"
          />
        </el-form-item>
      </el-form>
      <div slot="footer">
        <el-button @click="showDepositDialog = false">取消</el-button>
        <el-button type="primary" @click="handleDepositConfirm">确定</el-button>
      </div>
    </el-dialog>

    <!-- 出金对话框 -->
    <el-dialog
      title="账户出金"
      :visible.sync="showWithdrawDialog"
      width="400px"
    >
      <el-form ref="withdrawForm" :model="withdrawForm" label-width="80px">
        <el-form-item label="账户">
          <el-input :value="currentAccount.account_name" disabled />
        </el-form-item>
        <el-form-item label="可用资金">
          <el-input :value="currentAccount.available" disabled />
        </el-form-item>
        <el-form-item label="出金金额" prop="amount">
          <el-input-number
            v-model="withdrawForm.amount"
            :min="100"
            :max="currentAccount.available"
            :step="100"
            style="width: 100%"
          />
        </el-form-item>
      </el-form>
      <div slot="footer">
        <el-button @click="showWithdrawDialog = false">取消</el-button>
        <el-button type="primary" @click="handleWithdrawConfirm">确定</el-button>
      </div>
    </el-dialog>
  </div>
</template>

<script>
import { getUserAccounts, createUserAccount, deposit, withdraw } from '@/api'
import { mapGetters } from 'vuex'

export default {
  name: 'MyAccounts',

  data() {
    return {
      loading: false,
      accounts: [],

      // 创建账户
      showCreateDialog: false,
      creating: false,
      createForm: {
        account_name: '',
        account_type: 'individual',
        init_cash: 100000
      },
      createRules: {
        account_name: [
          { required: true, message: '请输入账户名称', trigger: 'blur' },
          { min: 2, max: 50, message: '长度在 2 到 50 个字符', trigger: 'blur' }
        ],
        account_type: [
          { required: true, message: '请选择账户类型', trigger: 'change' }
        ],
        init_cash: [
          { required: true, message: '请输入初始资金', trigger: 'blur' }
        ]
      },

      // 入金/出金
      showDepositDialog: false,
      showWithdrawDialog: false,
      currentAccount: {},
      depositForm: {
        amount: 10000
      },
      withdrawForm: {
        amount: 1000
      }
    }
  },

  computed: {
    ...mapGetters(['currentUser']),

    totalBalance() {
      return this.accounts.reduce((sum, acc) => sum + acc.balance, 0)
    },

    totalAvailable() {
      return this.accounts.reduce((sum, acc) => sum + acc.available, 0)
    },

    totalMargin() {
      return this.accounts.reduce((sum, acc) => sum + acc.margin, 0)
    }
  },

  mounted() {
    this.fetchAccounts()
  },

  methods: {
    async fetchAccounts() {
      if (!this.currentUser) {
        this.$message.error('请先登录')
        return
      }

      this.loading = true
      try {
        const res = await getUserAccounts(this.currentUser)
        this.accounts = res.accounts || []
      } catch (error) {
        this.$message.error('获取账户列表失败：' + (error.message || '未知错误'))
      } finally {
        this.loading = false
      }
    },

    async handleCreateAccount() {
      this.$refs.createForm.validate(async (valid) => {
        if (!valid) return

        this.creating = true
        try {
          await createUserAccount(this.currentUser, this.createForm)
          this.$message.success('账户创建成功')
          this.showCreateDialog = false
          this.resetCreateForm()
          await this.fetchAccounts()
        } catch (error) {
          this.$message.error('创建账户失败：' + (error.message || '未知错误'))
        } finally {
          this.creating = false
        }
      })
    },

    resetCreateForm() {
      this.createForm = {
        account_name: '',
        account_type: 'individual',
        init_cash: 100000
      }
      this.$refs.createForm && this.$refs.createForm.clearValidate()
    },

    handleViewAccount(account) {
      // TODO: 跳转到账户详情页
      this.$router.push({
        name: 'AccountDetail',
        params: { accountId: account.account_id }
      })
    },

    handleDeposit(account) {
      this.currentAccount = account
      this.depositForm.amount = 10000
      this.showDepositDialog = true
    },

    async handleDepositConfirm() {
      try {
        await deposit({
          user_id: this.currentUser,
          amount: this.depositForm.amount
        })
        this.$message.success('入金成功')
        this.showDepositDialog = false
        await this.fetchAccounts()
      } catch (error) {
        this.$message.error('入金失败：' + (error.message || '未知错误'))
      }
    },

    handleWithdraw(account) {
      this.currentAccount = account
      this.withdrawForm.amount = 1000
      this.showWithdrawDialog = true
    },

    async handleWithdrawConfirm() {
      try {
        await withdraw({
          user_id: this.currentUser,
          amount: this.withdrawForm.amount
        })
        this.$message.success('出金成功')
        this.showWithdrawDialog = false
        await this.fetchAccounts()
      } catch (error) {
        this.$message.error('出金失败：' + (error.message || '未知错误'))
      }
    },

    getAccountTypeLabel(type) {
      const labels = {
        'Individual': '个人',
        'Institutional': '机构',
        'MarketMaker': '做市商'
      }
      return labels[type] || type
    },

    getAccountTypeTag(type) {
      const tags = {
        'Individual': 'success',
        'Institutional': 'warning',
        'MarketMaker': 'danger'
      }
      return tags[type] || ''
    },

    getRiskRatioTag(ratio) {
      if (ratio >= 0.8) return 'danger'
      if (ratio >= 0.5) return 'warning'
      return 'success'
    },

    formatTimestamp(timestamp) {
      if (!timestamp) return '-'
      const date = new Date(timestamp * 1000)
      return date.toLocaleString('zh-CN')
    }
  }
}
</script>

<style scoped>
.my-accounts-container {
  padding: 20px;
}

.header-card {
  margin-bottom: 20px;
}

.card-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.card-header h3 {
  margin: 0;
  font-size: 18px;
  font-weight: 600;
}

.stats-row {
  margin-top: 20px;
}

.stat-card {
  text-align: center;
  padding: 20px;
  background: #f5f7fa;
  border-radius: 4px;
}

.stat-label {
  font-size: 14px;
  color: #909399;
  margin-bottom: 8px;
}

.stat-value {
  font-size: 24px;
  font-weight: 600;
  color: #303133;
}

.accounts-card {
  margin-top: 20px;
}

.money {
  font-family: 'Monaco', 'Courier New', monospace;
  font-weight: 500;
}

.empty-state {
  padding: 40px 0;
  text-align: center;
}

.form-tip {
  font-size: 12px;
  color: #909399;
  margin-top: 5px;
}
</style>
