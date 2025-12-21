<template>
  <div class="accounts-container">
    <!-- 页面标题 -->
    <div class="page-header">
      <h2>账户管理</h2>
      <div class="header-stats">
        <el-card shadow="hover" class="stat-card">
          <div class="stat-item">
            <div class="stat-label">总账户数</div>
            <div class="stat-value">{{ totalAccounts }}</div>
          </div>
        </el-card>
        <el-card shadow="hover" class="stat-card">
          <div class="stat-item">
            <div class="stat-label">总资金</div>
            <div class="stat-value">{{ totalBalance.toLocaleString() }}</div>
          </div>
        </el-card>
        <el-card shadow="hover" class="stat-card">
          <div class="stat-item">
            <div class="stat-label">可用资金</div>
            <div class="stat-value">{{ totalAvailable.toLocaleString() }}</div>
          </div>
        </el-card>
      </div>
    </div>

    <!-- 筛选条件 -->
    <div class="filter-container">
      <el-input
        v-model="filters.userId"
        placeholder="用户ID"
        clearable
        style="width: 200px; margin-right: 10px"
        @clear="fetchAccounts"
      />
      <el-select
        v-model="filters.status"
        placeholder="账户状态"
        clearable
        style="width: 150px; margin-right: 10px"
      >
        <el-option label="全部" value=""></el-option>
        <el-option label="活跃" value="active"></el-option>
        <el-option label="冻结" value="frozen"></el-option>
      </el-select>
      <el-button type="primary" icon="el-icon-search" @click="fetchAccounts">查询</el-button>
      <el-button icon="el-icon-refresh" @click="resetFilters">重置</el-button>
    </div>

    <!-- 账户列表 @yutiansut @quantaxis -->
    <div class="table-container">
      <el-table
        ref="accountTable"
        :data="accounts"
        border
        stripe
        highlight-current-row
        v-loading="loading"
        height="500"
        style="width: 100%"
      >
        <el-table-column prop="account_id" label="账户ID" width="200" sortable show-overflow-tooltip></el-table-column>
        <el-table-column prop="user_id" label="用户ID" width="200" sortable show-overflow-tooltip></el-table-column>
        <el-table-column prop="account_name" label="账户名称" width="150"></el-table-column>
        <el-table-column prop="account_type" label="账户类型" width="120">
          <template slot-scope="scope">
            <el-tag :type="scope.row.account_type === 'Individual' ? 'success' : 'warning'" size="small">
              {{ scope.row.account_type === 'Individual' ? '个人' : '机构' }}
            </el-tag>
          </template>
        </el-table-column>
        <el-table-column prop="balance" label="总权益" width="140" align="right">
          <template slot-scope="scope">
            <span :class="{ 'positive': scope.row.balance > 0 }">
              {{ scope.row.balance.toLocaleString('zh-CN', { minimumFractionDigits: 2 }) }}
            </span>
          </template>
        </el-table-column>
        <el-table-column prop="available" label="可用资金" width="140" align="right">
          <template slot-scope="scope">
            {{ scope.row.available.toLocaleString('zh-CN', { minimumFractionDigits: 2 }) }}
          </template>
        </el-table-column>
        <el-table-column prop="margin_used" label="占用保证金" width="140" align="right">
          <template slot-scope="scope">
            {{ (scope.row.margin_used || 0).toLocaleString('zh-CN', { minimumFractionDigits: 2 }) }}
          </template>
        </el-table-column>
        <el-table-column prop="risk_ratio" label="风险率" width="120" align="right">
          <template slot-scope="scope">
            <el-tag :type="getRiskTagType(scope.row.risk_ratio)" size="small">
              {{ (scope.row.risk_ratio * 100).toFixed(2) }}%
            </el-tag>
          </template>
        </el-table-column>
        <el-table-column prop="created_at" label="创建时间" width="180">
          <template slot-scope="scope">
            {{ formatTimestamp(scope.row.created_at) }}
          </template>
        </el-table-column>
        <el-table-column label="操作" width="200" fixed="right">
          <template slot-scope="scope">
            <el-button size="mini" type="text" @click="showAccountDetail(scope.row)">详情</el-button>
            <el-button size="mini" type="text" @click="showDepositDialog(scope.row)">入金</el-button>
            <el-button size="mini" type="text" @click="showWithdrawDialog(scope.row)">出金</el-button>
          </template>
        </el-table-column>
      </el-table>

      <!-- 分页 -->
      <div class="pagination-container">
        <el-pagination
          background
          layout="total, sizes, prev, pager, next, jumper"
          :total="pagination.total"
          :page-size="pagination.pageSize"
          :current-page="pagination.currentPage"
          :page-sizes="[10, 20, 50, 100]"
          @size-change="handleSizeChange"
          @current-change="handleCurrentChange"
        />
      </div>
    </div>

    <!-- 入金对话框 -->
    <el-dialog
      title="入金"
      :visible.sync="depositDialogVisible"
      width="500px"
      :close-on-click-modal="false"
    >
      <el-form :model="depositForm" :rules="depositRules" ref="depositFormRef" label-width="100px">
        <el-form-item label="用户ID">
          <el-input v-model="depositForm.user_id" disabled></el-input>
        </el-form-item>
        <el-form-item label="入金金额" prop="amount">
          <el-input-number
            v-model="depositForm.amount"
            :min="0.01"
            :precision="2"
            :step="100"
            controls-position="right"
            style="width: 100%"
          ></el-input-number>
        </el-form-item>
        <el-form-item label="入金方式" prop="method">
          <el-select v-model="depositForm.method" placeholder="请选择" style="width: 100%">
            <el-option label="银行转账" value="bank_transfer"></el-option>
            <el-option label="微信支付" value="wechat"></el-option>
            <el-option label="支付宝" value="alipay"></el-option>
            <el-option label="其他" value="other"></el-option>
          </el-select>
        </el-form-item>
        <el-form-item label="备注">
          <el-input v-model="depositForm.remark" type="textarea" :rows="3"></el-input>
        </el-form-item>
      </el-form>
      <div slot="footer">
        <el-button @click="depositDialogVisible = false">取消</el-button>
        <el-button type="primary" @click="handleDeposit" :loading="submitting">确认入金</el-button>
      </div>
    </el-dialog>

    <!-- 出金对话框 -->
    <el-dialog
      title="出金"
      :visible.sync="withdrawDialogVisible"
      width="500px"
      :close-on-click-modal="false"
    >
      <el-form :model="withdrawForm" :rules="withdrawRules" ref="withdrawFormRef" label-width="100px">
        <el-form-item label="用户ID">
          <el-input v-model="withdrawForm.user_id" disabled></el-input>
        </el-form-item>
        <el-form-item label="可用资金">
          <el-input v-model="currentAccountAvailable" disabled></el-input>
        </el-form-item>
        <el-form-item label="出金金额" prop="amount">
          <el-input-number
            v-model="withdrawForm.amount"
            :min="0.01"
            :max="parseFloat(currentAccountAvailable)"
            :precision="2"
            :step="100"
            controls-position="right"
            style="width: 100%"
          ></el-input-number>
        </el-form-item>
        <el-form-item label="出金方式" prop="method">
          <el-select v-model="withdrawForm.method" placeholder="请选择" style="width: 100%">
            <el-option label="银行转账" value="bank_transfer"></el-option>
            <el-option label="微信支付" value="wechat"></el-option>
            <el-option label="支付宝" value="alipay"></el-option>
          </el-select>
        </el-form-item>
        <el-form-item label="银行账号" v-if="withdrawForm.method === 'bank_transfer'">
          <el-input v-model="withdrawForm.bank_account" placeholder="请输入银行账号"></el-input>
        </el-form-item>
      </el-form>
      <div slot="footer">
        <el-button @click="withdrawDialogVisible = false">取消</el-button>
        <el-button type="primary" @click="handleWithdraw" :loading="submitting">确认出金</el-button>
      </div>
    </el-dialog>
  </div>
</template>

<script>
import { listAllAccounts, managementDeposit, managementWithdraw } from '@/api'

export default {
  name: 'AccountManagement',
  data() {
    return {
      loading: false,
      submitting: false,
      accounts: [],
      filters: {
        userId: '',
        status: ''
      },
      pagination: {
        currentPage: 1,
        pageSize: 20,
        total: 0
      },
      // 入金表单
      depositDialogVisible: false,
      depositForm: {
        user_id: '',
        amount: 0,
        method: '',
        remark: ''
      },
      depositRules: {
        amount: [{ required: true, message: '请输入入金金额', trigger: 'blur' }],
        method: [{ required: true, message: '请选择入金方式', trigger: 'change' }]
      },
      // 出金表单
      withdrawDialogVisible: false,
      withdrawForm: {
        user_id: '',
        amount: 0,
        method: '',
        bank_account: ''
      },
      withdrawRules: {
        amount: [{ required: true, message: '请输入出金金额', trigger: 'blur' }],
        method: [{ required: true, message: '请选择出金方式', trigger: 'change' }]
      },
      currentAccountAvailable: '0'
    }
  },
  computed: {
    totalAccounts() {
      return this.pagination.total
    },
    totalBalance() {
      return this.accounts.reduce((sum, acc) => sum + acc.balance, 0)
    },
    totalAvailable() {
      return this.accounts.reduce((sum, acc) => sum + acc.available, 0)
    }
  },
  mounted() {
    this.fetchAccounts()
  },
  methods: {
    async fetchAccounts() {
      this.loading = true
      try {
        const params = {
          page: this.pagination.currentPage,
          page_size: this.pagination.pageSize,
          status: this.filters.status
        }

        // 如果设置了用户ID筛选，添加到查询参数
        if (this.filters.userId && this.filters.userId.trim()) {
          params.user_id = this.filters.userId.trim()
        }

        // axios 拦截器已处理响应，直接返回 data 内容 @yutiansut @quantaxis
        const data = await listAllAccounts(params)

        // 如果有本地用户ID筛选，进行前端过滤（备用方案）
        let accounts = (data && data.accounts) || []
        if (this.filters.userId && this.filters.userId.trim()) {
          const userIdFilter = this.filters.userId.trim().toLowerCase()
          accounts = accounts.filter(acc =>
            acc.user_id && acc.user_id.toLowerCase().includes(userIdFilter)
          )
        }

        this.accounts = accounts
        this.pagination.total = (data && data.total) || 0
      } catch (error) {
        this.$message.error('获取账户列表失败: ' + error.message)
      } finally {
        this.loading = false
      }
    },
    resetFilters() {
      this.filters = {
        userId: '',
        status: ''
      }
      this.pagination.currentPage = 1
      this.fetchAccounts()
    },
    handleSizeChange(val) {
      this.pagination.pageSize = val
      this.pagination.currentPage = 1
      this.fetchAccounts()
    },
    handleCurrentChange(val) {
      this.pagination.currentPage = val
      this.fetchAccounts()
    },
    getRiskTagType(riskRatio) {
      if (riskRatio < 0.6) return 'success'
      if (riskRatio < 0.8) return 'warning'
      return 'danger'
    },
    formatTimestamp(timestamp) {
      const date = new Date(timestamp * 1000)
      return date.toLocaleString('zh-CN')
    },
    showAccountDetail(row) {
      this.$router.push({ path: `/admin/account-detail/${row.user_id}` })
    },
    showDepositDialog(row) {
      this.depositForm = {
        user_id: row.user_id,
        amount: 0,
        method: '',
        remark: ''
      }
      this.depositDialogVisible = true
    },
    showWithdrawDialog(row) {
      this.withdrawForm = {
        user_id: row.user_id,
        amount: 0,
        method: '',
        bank_account: ''
      }
      this.currentAccountAvailable = row.available.toString()
      this.withdrawDialogVisible = true
    },
    async handleDeposit() {
      this.$refs.depositFormRef.validate(async (valid) => {
        if (!valid) return

        this.submitting = true
        try {
          await managementDeposit(this.depositForm)
          this.$message.success('入金成功')
          this.depositDialogVisible = false
          this.fetchAccounts()
        } catch (error) {
          this.$message.error('入金失败: ' + error.message)
        } finally {
          this.submitting = false
        }
      })
    },
    async handleWithdraw() {
      this.$refs.withdrawFormRef.validate(async (valid) => {
        if (!valid) return

        this.submitting = true
        try {
          await managementWithdraw(this.withdrawForm)
          this.$message.success('出金成功')
          this.withdrawDialogVisible = false
          this.fetchAccounts()
        } catch (error) {
          this.$message.error('出金失败: ' + error.message)
        } finally {
          this.submitting = false
        }
      })
    }
  }
}
</script>

<style lang="scss" scoped>
// @yutiansut @quantaxis - 账户管理页面深色主题
$dark-bg-primary: #0d1117;
$dark-bg-secondary: #161b22;
$dark-bg-card: #1c2128;
$dark-bg-tertiary: #21262d;
$dark-border: #30363d;
$dark-text-primary: #f0f6fc;
$dark-text-secondary: #8b949e;
$dark-text-muted: #6e7681;
$primary-color: #1890ff;
$success-color: #52c41a;
$danger-color: #f5222d;
$warning-color: #faad14;

.accounts-container {
  padding: 20px;
  min-height: 100%;
  background: $dark-bg-primary;
}

.page-header {
  margin-bottom: 20px;

  h2 {
    margin: 0 0 20px 0;
    font-size: 24px;
    font-weight: 600;
    color: $dark-text-primary;
  }
}

.header-stats {
  display: flex;
  gap: 20px;
  margin-bottom: 20px;
}

.stat-card {
  flex: 1;

  ::v-deep.el-card {
    background: $dark-bg-card !important;
    border: 1px solid $dark-border !important;
    border-radius: 10px;

    .el-card__body {
      padding: 20px;
    }
  }
}

.stat-item {
  text-align: center;
}

.stat-label {
  font-size: 13px;
  color: $dark-text-secondary;
  margin-bottom: 12px;
  text-transform: uppercase;
  letter-spacing: 0.5px;
}

.stat-value {
  font-size: 26px;
  font-weight: 700;
  color: $dark-text-primary;
  font-family: 'JetBrains Mono', monospace;
}

.filter-container {
  margin-bottom: 20px;
  padding: 16px;
  background: $dark-bg-secondary;
  border-radius: 8px;
  border: 1px solid $dark-border;

  ::v-deep .el-input__inner {
    background: $dark-bg-tertiary !important;
    border-color: $dark-border !important;
    color: $dark-text-primary !important;

    &::placeholder {
      color: $dark-text-muted;
    }
  }

  ::v-deep .el-select .el-input__inner {
    background: $dark-bg-tertiary !important;
    border-color: $dark-border !important;
    color: $dark-text-primary !important;
  }

  ::v-deep .el-button--primary {
    background: $primary-color;
    border-color: $primary-color;
  }

  ::v-deep .el-button--default {
    background: $dark-bg-tertiary;
    border-color: $dark-border;
    color: $dark-text-secondary;

    &:hover {
      border-color: $primary-color;
      color: $primary-color;
    }
  }
}

.table-container {
  background: $dark-bg-card;
  padding: 20px;
  border-radius: 8px;
  border: 1px solid $dark-border;

  ::v-deep .el-table {
    background: transparent !important;

    &::before {
      display: none;
    }

    th.el-table__cell {
      background: $dark-bg-secondary !important;
      border-bottom: 1px solid $dark-border !important;
      color: $dark-text-secondary !important;
      font-weight: 600;
    }

    td.el-table__cell {
      background: $dark-bg-card !important;
      border-bottom: 1px solid $dark-border !important;
      color: $dark-text-primary !important;
    }

    tr {
      background: $dark-bg-card !important;
    }

    .el-table__row--striped td.el-table__cell {
      background: $dark-bg-secondary !important;
    }

    .el-table__row:hover > td.el-table__cell {
      background: $dark-bg-tertiary !important;
    }

    .cell {
      color: $dark-text-primary !important;
    }

    .el-table__fixed-right {
      th.el-table__cell,
      td.el-table__cell {
        background: $dark-bg-card !important;
      }
    }

    .el-table__empty-block {
      background: $dark-bg-card;
    }

    .el-table__empty-text {
      color: $dark-text-muted;
    }
  }
}

.pagination-container {
  margin-top: 20px;
  text-align: right;

  ::v-deep .el-pagination {
    .btn-prev,
    .btn-next,
    .el-pager li {
      background: $dark-bg-tertiary !important;
      color: $dark-text-secondary !important;
      border: none;

      &:hover {
        color: $primary-color !important;
      }

      &.active {
        background: $primary-color !important;
        color: white !important;
      }
    }

    .el-pagination__total,
    .el-pagination__jump {
      color: $dark-text-secondary;
    }

    .el-input__inner {
      background: $dark-bg-tertiary !important;
      border-color: $dark-border !important;
      color: $dark-text-primary !important;
    }
  }
}

.positive {
  color: $success-color !important;
  font-weight: 600;
}

// 标签样式
::v-deep .el-tag {
  border: none;
  font-weight: 600;

  &--success {
    background: rgba($success-color, 0.15);
    color: $success-color;
  }

  &--warning {
    background: rgba($warning-color, 0.15);
    color: $warning-color;
  }

  &--danger {
    background: rgba($danger-color, 0.15);
    color: $danger-color;
  }
}

// 对话框
::v-deep .el-dialog {
  background: $dark-bg-card;
  border: 1px solid $dark-border;
  border-radius: 12px;

  .el-dialog__header {
    background: $dark-bg-secondary;
    border-bottom: 1px solid $dark-border;
    border-radius: 12px 12px 0 0;

    .el-dialog__title {
      color: $dark-text-primary;
    }

    .el-dialog__headerbtn .el-dialog__close {
      color: $dark-text-secondary;
    }
  }

  .el-dialog__body {
    background: $dark-bg-card;
  }

  .el-dialog__footer {
    background: $dark-bg-secondary;
    border-top: 1px solid $dark-border;
    border-radius: 0 0 12px 12px;
  }

  .el-form-item__label {
    color: $dark-text-secondary;
  }

  .el-input__inner,
  .el-textarea__inner {
    background: $dark-bg-tertiary !important;
    border-color: $dark-border !important;
    color: $dark-text-primary !important;
  }

  .el-input-number {
    .el-input__inner {
      background: $dark-bg-tertiary !important;
      border-color: $dark-border !important;
      color: $dark-text-primary !important;
    }

    .el-input-number__decrease,
    .el-input-number__increase {
      background: $dark-bg-secondary;
      border-color: $dark-border;
      color: $dark-text-secondary;
    }
  }
}

// 下拉菜单
::v-deep .el-select-dropdown {
  background: $dark-bg-card !important;
  border: 1px solid $dark-border !important;

  .el-select-dropdown__item {
    color: $dark-text-primary;

    &:hover {
      background: $dark-bg-tertiary;
    }

    &.selected {
      color: $primary-color;
      font-weight: 600;
    }
  }
}

// 操作按钮
::v-deep .el-button--text {
  color: $primary-color;

  &:hover {
    color: lighten($primary-color, 10%);
  }
}
</style>
