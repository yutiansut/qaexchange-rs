<template>
  <div class="transactions-container">
    <!-- 页面标题 -->
    <div class="page-header">
      <h2>资金流水管理</h2>
    </div>

    <!-- 筛选条件 -->
    <div class="filter-container">
      <el-input
        v-model="filters.userId"
        placeholder="用户ID"
        clearable
        style="width: 200px; margin-right: 10px"
      />
      <el-select
        v-model="filters.transactionType"
        placeholder="交易类型"
        clearable
        style="width: 150px; margin-right: 10px"
      >
        <el-option label="全部" value=""></el-option>
        <el-option label="入金" value="deposit"></el-option>
        <el-option label="出金" value="withdrawal"></el-option>
        <el-option label="手续费" value="commission"></el-option>
        <el-option label="盈亏" value="pnl"></el-option>
        <el-option label="结算" value="settlement"></el-option>
      </el-select>
      <el-date-picker
        v-model="dateRange"
        type="daterange"
        range-separator="至"
        start-placeholder="开始日期"
        end-placeholder="结束日期"
        style="width: 300px; margin-right: 10px"
      />
      <el-button type="primary" icon="el-icon-search" @click="fetchTransactions">查询</el-button>
      <el-button icon="el-icon-refresh" @click="resetFilters">重置</el-button>
      <el-button icon="el-icon-download" @click="exportData">导出Excel</el-button>
    </div>

    <!-- 统计卡片 -->
    <div class="stats-container">
      <el-card shadow="hover" class="stat-card">
        <div class="stat-item">
          <div class="stat-label">总入金</div>
          <div class="stat-value positive">+{{ totalDeposit.toLocaleString() }}</div>
        </div>
      </el-card>
      <el-card shadow="hover" class="stat-card">
        <div class="stat-item">
          <div class="stat-label">总出金</div>
          <div class="stat-value negative">-{{ totalWithdrawal.toLocaleString() }}</div>
        </div>
      </el-card>
      <el-card shadow="hover" class="stat-card">
        <div class="stat-item">
          <div class="stat-label">净流入</div>
          <div class="stat-value" :class="{ positive: netFlow > 0, negative: netFlow < 0 }">
            {{ netFlow > 0 ? '+' : '' }}{{ netFlow.toLocaleString() }}
          </div>
        </div>
      </el-card>
      <el-card shadow="hover" class="stat-card">
        <div class="stat-item">
          <div class="stat-label">交易笔数</div>
          <div class="stat-value">{{ transactions.length }}</div>
        </div>
      </el-card>
    </div>

    <!-- 流水列表 -->
    <div class="table-container">
      <el-table
        ref="transactionTable"
        :data="filteredTransactions"
        border
        stripe
        v-loading="loading"
        height="500"
      >
        <el-table-column prop="transaction_id" label="交易ID" width="180" sortable></el-table-column>
        <el-table-column prop="user_id" label="用户ID" width="150" sortable></el-table-column>
        <el-table-column prop="transaction_type" label="交易类型" width="120" sortable>
          <template slot-scope="scope">
            <el-tag :type="getTypeTagType(scope.row.transaction_type)" size="small">
              {{ getTypeName(scope.row.transaction_type) }}
            </el-tag>
          </template>
        </el-table-column>
        <el-table-column prop="amount" label="金额" width="140" align="right" sortable>
          <template slot-scope="scope">
            <span :class="getAmountClass(scope.row.transaction_type)">
              {{ formatAmount(scope.row.transaction_type, scope.row.amount) }}
            </span>
          </template>
        </el-table-column>
        <el-table-column prop="balance_before" label="交易前余额" width="140" align="right">
          <template slot-scope="scope">
            {{ scope.row.balance_before.toLocaleString('zh-CN', { minimumFractionDigits: 2 }) }}
          </template>
        </el-table-column>
        <el-table-column prop="balance_after" label="交易后余额" width="140" align="right">
          <template slot-scope="scope">
            {{ scope.row.balance_after.toLocaleString('zh-CN', { minimumFractionDigits: 2 }) }}
          </template>
        </el-table-column>
        <el-table-column prop="status" label="状态" width="100" sortable>
          <template slot-scope="scope">
            <el-tag :type="getStatusTagType(scope.row.status)" size="small">
              {{ getStatusName(scope.row.status) }}
            </el-tag>
          </template>
        </el-table-column>
        <el-table-column prop="method" label="方式" width="120">
          <template slot-scope="scope">
            {{ getMethodName(scope.row.method) }}
          </template>
        </el-table-column>
        <el-table-column prop="remark" label="备注" width="200"></el-table-column>
        <el-table-column prop="created_at" label="交易时间" width="180" sortable>
          <template slot-scope="scope">
            {{ scope.row.created_at }}
          </template>
        </el-table-column>
      </el-table>
    </div>
  </div>
</template>

<script>
import dayjs from 'dayjs'
import { getTransactions } from '@/api'

export default {
  name: 'TransactionManagement',
  data() {
    return {
      loading: false,
      transactions: [],
      filters: {
        userId: '',
        transactionType: ''
      },
      dateRange: null
    }
  },
  computed: {
    filteredTransactions() {
      let result = this.transactions

      if (this.filters.userId) {
        result = result.filter(t => t.user_id.includes(this.filters.userId))
      }

      if (this.filters.transactionType) {
        result = result.filter(t => t.transaction_type === this.filters.transactionType)
      }

      return result
    },
    totalDeposit() {
      return this.transactions
        .filter(t => t.transaction_type === 'deposit')
        .reduce((sum, t) => sum + t.amount, 0)
    },
    totalWithdrawal() {
      return this.transactions
        .filter(t => t.transaction_type === 'withdrawal')
        .reduce((sum, t) => sum + t.amount, 0)
    },
    netFlow() {
      return this.totalDeposit - this.totalWithdrawal
    }
  },
  mounted() {
    // 默认查询近7天
    const end = new Date()
    const start = new Date()
    start.setDate(start.getDate() - 7)
    this.dateRange = [start, end]
    this.fetchTransactions()
  },
  methods: {
    async fetchTransactions() {
      if (!this.filters.userId) {
        this.$message.warning('请输入用户ID')
        return
      }

      this.loading = true
      try {
        const params = {}
        if (this.dateRange && this.dateRange.length === 2) {
          params.start_date = this.formatDate(this.dateRange[0])
          params.end_date = this.formatDate(this.dateRange[1])
        }

        // axios 拦截器已处理响应 @yutiansut @quantaxis
        const data = await getTransactions(this.filters.userId, params)
        this.transactions = data || []
      } catch (error) {
        this.$message.error('获取资金流水失败: ' + error.message)
      } finally {
        this.loading = false
      }
    },
    resetFilters() {
      this.filters = {
        userId: '',
        transactionType: ''
      }
      const end = new Date()
      const start = new Date()
      start.setDate(start.getDate() - 7)
      this.dateRange = [start, end]
      this.transactions = []
    },
    formatDate(date) {
      const year = date.getFullYear()
      const month = String(date.getMonth() + 1).padStart(2, '0')
      const day = String(date.getDate()).padStart(2, '0')
      return `${year}-${month}-${day}`
    },
    getTypeTagType(type) {
      const types = {
        deposit: 'success',
        withdrawal: 'warning',
        commission: 'info',
        pnl: 'primary',
        settlement: 'info'
      }
      return types[type] || 'info'
    },
    getTypeName(type) {
      const names = {
        deposit: '入金',
        withdrawal: '出金',
        commission: '手续费',
        pnl: '盈亏',
        settlement: '结算'
      }
      return names[type] || type
    },
    getStatusTagType(status) {
      const types = {
        completed: 'success',
        pending: 'warning',
        failed: 'danger',
        cancelled: 'info'
      }
      return types[status] || 'info'
    },
    getStatusName(status) {
      const names = {
        completed: '已完成',
        pending: '处理中',
        failed: '失败',
        cancelled: '已取消'
      }
      return names[status] || status
    },
    getMethodName(method) {
      const names = {
        bank_transfer: '银行转账',
        wechat: '微信支付',
        alipay: '支付宝',
        other: '其他'
      }
      return names[method] || method || '-'
    },
    getAmountClass(type) {
      return type === 'deposit' ? 'positive' : type === 'withdrawal' ? 'negative' : ''
    },
    formatAmount(type, amount) {
      const prefix = type === 'deposit' ? '+' : type === 'withdrawal' ? '-' : ''
      return prefix + amount.toLocaleString('zh-CN', { minimumFractionDigits: 2 })
    },
    exportData() {
      if (this.transactions.length === 0) {
        this.$message.warning('暂无数据可导出')
        return
      }
      const headers = ['时间', '账户', '类型', '金额', '状态', '方式', '备注']
      const rows = this.transactions.map(item => [
        item.timestamp || item.created_at || '',
        item.user_id || '',
        this.getTypeName(item.transaction_type || item.type || ''),
        Number(item.amount || 0).toFixed(2),
        this.getStatusName(item.status || ''),
        this.getMethodName(item.method || ''),
        item.remark || ''
      ])

      const csvContent = [headers.join(','), ...rows.map(r => r.join(','))].join('\n')
      const blob = new Blob(['\ufeff' + csvContent], { type: 'text/csv;charset=utf-8;' })
      const link = document.createElement('a')
      link.href = URL.createObjectURL(blob)
      link.download = `transactions_${dayjs().format('YYYYMMDD_HHmmss')}.csv`
      document.body.appendChild(link)
      link.click()
      document.body.removeChild(link)
    }
  }
}
</script>

<style lang="scss" scoped>
// @yutiansut @quantaxis - 资金流水管理页面深色主题
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

.transactions-container {
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

  ::v-deep .el-date-editor {
    background: $dark-bg-tertiary !important;
    border-color: $dark-border !important;

    .el-range-input {
      background: transparent !important;
      color: $dark-text-primary !important;

      &::placeholder {
        color: $dark-text-muted;
      }
    }

    .el-range-separator {
      color: $dark-text-secondary;
    }

    .el-range__icon,
    .el-range__close-icon {
      color: $dark-text-secondary;
    }
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

.stats-container {
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
      font-size: 12px;
      text-transform: uppercase;
      letter-spacing: 0.5px;
    }

    td.el-table__cell {
      background: $dark-bg-card !important;
      border-bottom: 1px solid $dark-border !important;
      color: $dark-text-primary !important;
      font-family: 'JetBrains Mono', monospace;
      font-size: 13px;
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

    .el-table__empty-block {
      background: $dark-bg-card;
    }

    .el-table__empty-text {
      color: $dark-text-muted;
    }

    // 滚动条
    .el-table__body-wrapper {
      &::-webkit-scrollbar {
        width: 8px;
        height: 8px;
      }

      &::-webkit-scrollbar-track {
        background: $dark-bg-secondary;
      }

      &::-webkit-scrollbar-thumb {
        background: $dark-border;
        border-radius: 4px;

        &:hover {
          background: #484f58;
        }
      }
    }
  }
}

.positive {
  color: $success-color !important;
  font-weight: 600;
  text-shadow: 0 0 10px rgba($success-color, 0.3);
}

.negative {
  color: $danger-color !important;
  font-weight: 600;
  text-shadow: 0 0 10px rgba($danger-color, 0.3);
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

  &--info {
    background: rgba($dark-text-secondary, 0.15);
    color: $dark-text-secondary;
  }

  &--danger {
    background: rgba($danger-color, 0.15);
    color: $danger-color;
  }

  &--primary {
    background: rgba($primary-color, 0.15);
    color: $primary-color;
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
</style>
