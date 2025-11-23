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
      <vxe-table
        ref="transactionTable"
        :data="filteredTransactions"
        border
        stripe
        resizable
        highlight-hover-row
        :loading="loading"
        :sort-config="{ trigger: 'cell', remote: false }"
        height="500"
      >
        <vxe-table-column field="transaction_id" title="交易ID" width="180" sortable></vxe-table-column>
        <vxe-table-column field="user_id" title="用户ID" width="150" sortable></vxe-table-column>
        <vxe-table-column field="transaction_type" title="交易类型" width="120" sortable>
          <template slot-scope="{ row }">
            <el-tag :type="getTypeTagType(row.transaction_type)" size="small">
              {{ getTypeName(row.transaction_type) }}
            </el-tag>
          </template>
        </vxe-table-column>
        <vxe-table-column field="amount" title="金额" width="140" align="right" sortable>
          <template slot-scope="{ row }">
            <span :class="getAmountClass(row.transaction_type)">
              {{ formatAmount(row.transaction_type, row.amount) }}
            </span>
          </template>
        </vxe-table-column>
        <vxe-table-column field="balance_before" title="交易前余额" width="140" align="right">
          <template slot-scope="{ row }">
            {{ row.balance_before.toLocaleString('zh-CN', { minimumFractionDigits: 2 }) }}
          </template>
        </vxe-table-column>
        <vxe-table-column field="balance_after" title="交易后余额" width="140" align="right">
          <template slot-scope="{ row }">
            {{ row.balance_after.toLocaleString('zh-CN', { minimumFractionDigits: 2 }) }}
          </template>
        </vxe-table-column>
        <vxe-table-column field="status" title="状态" width="100" sortable>
          <template slot-scope="{ row }">
            <el-tag :type="getStatusTagType(row.status)" size="small">
              {{ getStatusName(row.status) }}
            </el-tag>
          </template>
        </vxe-table-column>
        <vxe-table-column field="method" title="方式" width="120">
          <template slot-scope="{ row }">
            {{ getMethodName(row.method) }}
          </template>
        </vxe-table-column>
        <vxe-table-column field="remark" title="备注" width="200"></vxe-table-column>
        <vxe-table-column field="created_at" title="交易时间" width="180" sortable>
          <template slot-scope="{ row }">
            {{ row.created_at }}
          </template>
        </vxe-table-column>
      </vxe-table>
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

        const { data } = await getTransactions(this.filters.userId, params)
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

<style scoped>
.transactions-container {
  padding: 20px;
}

.page-header {
  margin-bottom: 20px;
}

.page-header h2 {
  margin: 0 0 20px 0;
  font-size: 24px;
  font-weight: 500;
}

.filter-container {
  margin-bottom: 20px;
}

.stats-container {
  display: flex;
  gap: 20px;
  margin-bottom: 20px;
}

.stat-card {
  flex: 1;
}

.stat-item {
  text-align: center;
}

.stat-label {
  font-size: 14px;
  color: #909399;
  margin-bottom: 8px;
}

.stat-value {
  font-size: 24px;
  font-weight: 500;
  color: #303133;
}

.table-container {
  background: white;
  padding: 20px;
  border-radius: 4px;
}

.positive {
  color: #67C23A;
}

.negative {
  color: #F56C6C;
}
</style>
