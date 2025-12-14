<template>
  <div class="commission-container">
    <!-- 页面标题 -->
    <div class="page-header">
      <h2>手续费查询</h2>
      <div class="header-actions">
        <el-button icon="el-icon-refresh" @click="loadData">刷新</el-button>
      </div>
    </div>

    <!-- 账户选择和统计 -->
    <el-card class="stats-card">
      <div slot="header">
        <span>手续费统计</span>
      </div>
      <el-row :gutter="20">
        <el-col :span="6">
          <el-form-item label="选择账户">
            <el-select v-model="selectedAccountId" placeholder="请选择账户" style="width: 200px" @change="loadStatistics">
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
      <el-row :gutter="20" v-if="statistics">
        <el-col :span="6">
          <div class="stat-item">
            <div class="stat-label">累计手续费</div>
            <div class="stat-value danger">{{ statistics.total_commission.toFixed(2) }}</div>
          </div>
        </el-col>
        <el-col :span="6">
          <div class="stat-item">
            <div class="stat-label">今日手续费</div>
            <div class="stat-value warning">{{ statistics.today_commission.toFixed(2) }}</div>
          </div>
        </el-col>
        <el-col :span="6">
          <div class="stat-item">
            <div class="stat-label">本月手续费</div>
            <div class="stat-value primary">{{ statistics.this_month_commission.toFixed(2) }}</div>
          </div>
        </el-col>
        <el-col :span="6">
          <div class="stat-item">
            <div class="stat-label">交易笔数</div>
            <div class="stat-value success">{{ statistics.trade_count }}</div>
          </div>
        </el-col>
      </el-row>

      <!-- 按品种明细 -->
      <el-table
        v-if="statistics && statistics.by_instrument && statistics.by_instrument.length > 0"
        :data="statistics.by_instrument"
        stripe
        border
        style="width: 100%; margin-top: 20px;"
      >
        <el-table-column prop="instrument_id" label="合约" width="150"></el-table-column>
        <el-table-column prop="trade_count" label="交易笔数" width="100"></el-table-column>
        <el-table-column prop="total_volume" label="成交量" width="100"></el-table-column>
        <el-table-column prop="commission" label="手续费">
          <template slot-scope="scope">
            {{ scope.row.commission.toFixed(2) }}
          </template>
        </el-table-column>
      </el-table>
    </el-card>

    <!-- 手续费率表 -->
    <el-card class="rates-card">
      <div slot="header">
        <span>手续费率表</span>
        <el-input
          v-model="searchKeyword"
          placeholder="搜索品种"
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
        max-height="500"
      >
        <el-table-column prop="product_id" label="品种" width="100"></el-table-column>
        <el-table-column prop="exchange_id" label="交易所" width="100"></el-table-column>
        <el-table-column label="开仓手续费" align="center">
          <el-table-column prop="open_ratio_by_money" label="按金额" width="120">
            <template slot-scope="scope">
              {{ scope.row.open_ratio_by_money > 0 ? (scope.row.open_ratio_by_money * 100).toFixed(4) + '%' : '-' }}
            </template>
          </el-table-column>
          <el-table-column prop="open_ratio_by_volume" label="按手数" width="120">
            <template slot-scope="scope">
              {{ scope.row.open_ratio_by_volume > 0 ? scope.row.open_ratio_by_volume.toFixed(2) + '元/手' : '-' }}
            </template>
          </el-table-column>
        </el-table-column>
        <el-table-column label="平仓手续费" align="center">
          <el-table-column prop="close_ratio_by_money" label="按金额" width="120">
            <template slot-scope="scope">
              {{ scope.row.close_ratio_by_money > 0 ? (scope.row.close_ratio_by_money * 100).toFixed(4) + '%' : '-' }}
            </template>
          </el-table-column>
          <el-table-column prop="close_ratio_by_volume" label="按手数" width="120">
            <template slot-scope="scope">
              {{ scope.row.close_ratio_by_volume > 0 ? scope.row.close_ratio_by_volume.toFixed(2) + '元/手' : '-' }}
            </template>
          </el-table-column>
        </el-table-column>
        <el-table-column label="平今手续费" align="center">
          <el-table-column prop="close_today_ratio_by_money" label="按金额" width="120">
            <template slot-scope="scope">
              {{ scope.row.close_today_ratio_by_money > 0 ? (scope.row.close_today_ratio_by_money * 100).toFixed(4) + '%' : '-' }}
            </template>
          </el-table-column>
          <el-table-column prop="close_today_ratio_by_volume" label="按手数" width="120">
            <template slot-scope="scope">
              {{ scope.row.close_today_ratio_by_volume > 0 ? scope.row.close_today_ratio_by_volume.toFixed(2) + '元/手' : '-' }}
            </template>
          </el-table-column>
        </el-table-column>
      </el-table>
    </el-card>
  </div>
</template>

<script>
/**
 * 手续费查询页面 @yutiansut @quantaxis
 */
import { getCommissionRates, getCommissionStatistics, getUserAccounts } from '@/api'

export default {
  name: 'CommissionQuery',

  data() {
    return {
      loading: false,
      accounts: [],
      selectedAccountId: '',
      statistics: null,
      rates: [],
      searchKeyword: ''
    }
  },

  computed: {
    filteredRates() {
      if (!this.searchKeyword) return this.rates
      const keyword = this.searchKeyword.toLowerCase()
      return this.rates.filter(rate =>
        rate.product_id.toLowerCase().includes(keyword) ||
        rate.exchange_id.toLowerCase().includes(keyword)
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
            this.loadStatistics()
          }
        }
      } catch (err) {
        console.error('加载账户列表失败:', err)
      }
    },

    async loadRates() {
      this.loading = true
      try {
        const res = await getCommissionRates()
        if (res.success) {
          this.rates = res.data || []
        }
      } catch (err) {
        console.error('加载手续费率失败:', err)
        this.$message.error('加载手续费率失败')
      } finally {
        this.loading = false
      }
    },

    async loadStatistics() {
      if (!this.selectedAccountId) return
      try {
        const res = await getCommissionStatistics(this.selectedAccountId)
        if (res.success) {
          this.statistics = res.data
        }
      } catch (err) {
        console.error('加载手续费统计失败:', err)
      }
    }
  }
}
</script>

<style scoped>
.commission-container {
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

.stats-card {
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
  font-size: 24px;
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
