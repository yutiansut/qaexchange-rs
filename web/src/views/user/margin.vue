<template>
  <div class="margin-container">
    <!-- 页面标题 -->
    <div class="page-header">
      <h2>保证金查询</h2>
      <div class="header-actions">
        <el-button icon="el-icon-refresh" @click="loadData">刷新</el-button>
      </div>
    </div>

    <!-- 账户保证金汇总 -->
    <el-card class="summary-card">
      <div slot="header">
        <span>保证金汇总</span>
      </div>
      <el-row :gutter="20">
        <el-col :span="6">
          <el-form-item label="选择账户">
            <el-select v-model="selectedAccountId" placeholder="请选择账户" style="width: 200px" @change="loadSummary">
              <el-option
                v-for="account in accounts"
                :key="account.account_id"
                :label="account.account_id"
                :value="account.account_id"
              ></el-option>
            </el-select>
          </el-form-item>
        </el-col>
      </el-row>
      <el-row :gutter="20" v-if="summary">
        <el-col :span="4">
          <div class="stat-item">
            <div class="stat-label">占用保证金</div>
            <div class="stat-value primary">{{ summary.total_margin.toFixed(2) }}</div>
          </div>
        </el-col>
        <el-col :span="4">
          <div class="stat-item">
            <div class="stat-label">冻结保证金</div>
            <div class="stat-value warning">{{ summary.frozen_margin.toFixed(2) }}</div>
          </div>
        </el-col>
        <el-col :span="4">
          <div class="stat-item">
            <div class="stat-label">可用保证金</div>
            <div class="stat-value success">{{ summary.available_margin.toFixed(2) }}</div>
          </div>
        </el-col>
        <el-col :span="4">
          <div class="stat-item">
            <div class="stat-label">保证金占比</div>
            <div class="stat-value">{{ (summary.margin_ratio * 100).toFixed(2) }}%</div>
          </div>
        </el-col>
        <el-col :span="4">
          <div class="stat-item">
            <div class="stat-label">风险度</div>
            <div :class="['stat-value', getRiskClass(summary.risk_degree)]">
              {{ (summary.risk_degree * 100).toFixed(2) }}%
            </div>
          </div>
        </el-col>
      </el-row>

      <!-- 持仓保证金明细 -->
      <el-table
        v-if="summary && summary.positions && summary.positions.length > 0"
        :data="summary.positions"
        stripe
        border
        style="width: 100%; margin-top: 20px;"
      >
        <el-table-column prop="instrument_id" label="合约" width="150"></el-table-column>
        <el-table-column label="多头" align="center">
          <el-table-column prop="volume_long" label="持仓" width="80"></el-table-column>
          <el-table-column prop="margin_long" label="保证金" width="120">
            <template slot-scope="scope">
              {{ scope.row.margin_long.toFixed(2) }}
            </template>
          </el-table-column>
          <el-table-column prop="margin_rate_long" label="保证金率" width="100">
            <template slot-scope="scope">
              {{ (scope.row.margin_rate_long * 100).toFixed(2) }}%
            </template>
          </el-table-column>
        </el-table-column>
        <el-table-column label="空头" align="center">
          <el-table-column prop="volume_short" label="持仓" width="80"></el-table-column>
          <el-table-column prop="margin_short" label="保证金" width="120">
            <template slot-scope="scope">
              {{ scope.row.margin_short.toFixed(2) }}
            </template>
          </el-table-column>
          <el-table-column prop="margin_rate_short" label="保证金率" width="100">
            <template slot-scope="scope">
              {{ (scope.row.margin_rate_short * 100).toFixed(2) }}%
            </template>
          </el-table-column>
        </el-table-column>
        <el-table-column prop="last_price" label="最新价" width="100">
          <template slot-scope="scope">
            {{ scope.row.last_price.toFixed(2) }}
          </template>
        </el-table-column>
      </el-table>
    </el-card>

    <!-- 保证金率表 -->
    <el-card class="rates-card">
      <div slot="header">
        <span>保证金率表</span>
        <el-input
          v-model="searchKeyword"
          placeholder="搜索合约"
          prefix-icon="el-icon-search"
          style="width: 200px; float: right;"
          clearable
        ></el-input>
      </div>
      <el-table
        :data="filteredRates"
        v-loading="loading"
        stripe
        border
        style="width: 100%"
        max-height="400"
      >
        <el-table-column prop="instrument_id" label="合约" width="150"></el-table-column>
        <el-table-column label="多头保证金率" align="center">
          <el-table-column prop="long_margin_ratio_by_money" label="按金额" width="120">
            <template slot-scope="scope">
              {{ (scope.row.long_margin_ratio_by_money * 100).toFixed(2) }}%
            </template>
          </el-table-column>
          <el-table-column prop="long_margin_ratio_by_volume" label="按手数" width="120">
            <template slot-scope="scope">
              {{ scope.row.long_margin_ratio_by_volume > 0 ? scope.row.long_margin_ratio_by_volume.toFixed(2) + '元/手' : '-' }}
            </template>
          </el-table-column>
        </el-table-column>
        <el-table-column label="空头保证金率" align="center">
          <el-table-column prop="short_margin_ratio_by_money" label="按金额" width="120">
            <template slot-scope="scope">
              {{ (scope.row.short_margin_ratio_by_money * 100).toFixed(2) }}%
            </template>
          </el-table-column>
          <el-table-column prop="short_margin_ratio_by_volume" label="按手数" width="120">
            <template slot-scope="scope">
              {{ scope.row.short_margin_ratio_by_volume > 0 ? scope.row.short_margin_ratio_by_volume.toFixed(2) + '元/手' : '-' }}
            </template>
          </el-table-column>
        </el-table-column>
      </el-table>
    </el-card>
  </div>
</template>

<script>
/**
 * 保证金查询页面 @yutiansut @quantaxis
 */
import { getMarginRates, getAccountMarginSummary, getUserAccounts } from '@/api'

export default {
  name: 'MarginQuery',

  data() {
    return {
      loading: false,
      accounts: [],
      selectedAccountId: '',
      summary: null,
      rates: [],
      searchKeyword: ''
    }
  },

  computed: {
    filteredRates() {
      if (!this.searchKeyword) return this.rates
      const keyword = this.searchKeyword.toLowerCase()
      return this.rates.filter(rate =>
        rate.instrument_id.toLowerCase().includes(keyword)
      )
    }
  },

  created() {
    this.loadData()
  },

  methods: {
    async loadData() {
      await Promise.all([
        this.loadAccounts(),
        this.loadRates()
      ])
    },

    async loadAccounts() {
      try {
        const userId = localStorage.getItem('userId')
        if (!userId) return
        const res = await getUserAccounts(userId)
        if (res.success) {
          this.accounts = res.data || []
          if (this.accounts.length > 0 && !this.selectedAccountId) {
            this.selectedAccountId = this.accounts[0].account_id
            this.loadSummary()
          }
        }
      } catch (err) {
        console.error('加载账户列表失败:', err)
      }
    },

    async loadRates() {
      this.loading = true
      try {
        const res = await getMarginRates()
        if (res.success) {
          this.rates = res.data || []
        }
      } catch (err) {
        console.error('加载保证金率失败:', err)
        this.$message.error('加载保证金率失败')
      } finally {
        this.loading = false
      }
    },

    async loadSummary() {
      if (!this.selectedAccountId) return
      try {
        const res = await getAccountMarginSummary(this.selectedAccountId)
        if (res.success) {
          this.summary = res.data
        }
      } catch (err) {
        console.error('加载保证金汇总失败:', err)
      }
    },

    getRiskClass(risk) {
      if (risk >= 0.9) return 'danger'
      if (risk >= 0.8) return 'warning'
      return 'success'
    }
  }
}
</script>

<style scoped>
.margin-container {
  padding: 20px;
}

.page-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 20px;
}

.page-header h2 {
  margin: 0;
  color: #303133;
}

.summary-card {
  margin-bottom: 20px;
}

.rates-card {
  margin-bottom: 20px;
}

.stat-item {
  text-align: center;
  padding: 20px;
  background: #f5f7fa;
  border-radius: 8px;
}

.stat-label {
  font-size: 14px;
  color: #909399;
  margin-bottom: 10px;
}

.stat-value {
  font-size: 20px;
  font-weight: bold;
}

.stat-value.danger {
  color: #F56C6C;
}

.stat-value.warning {
  color: #E6A23C;
}

.stat-value.primary {
  color: #409EFF;
}

.stat-value.success {
  color: #67C23A;
}
</style>
