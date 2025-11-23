<template>
  <div class="risk-container">
    <!-- 页面标题 -->
    <div class="page-header">
      <h2>风控监控</h2>
      <div class="header-actions">
        <el-button icon="el-icon-refresh" @click="loadData">刷新</el-button>
        <el-switch
          v-model="autoRefresh"
          active-text="自动刷新"
          @change="toggleAutoRefresh"
        ></el-switch>
      </div>
    </div>

    <!-- 风险统计卡片 -->
    <el-row :gutter="20" class="stats-row">
      <el-col :span="6">
        <div class="stat-card danger">
          <div class="stat-icon">
            <i class="el-icon-warning"></i>
          </div>
          <div class="stat-content">
            <div class="stat-value">{{ statistics.highRiskCount }}</div>
            <div class="stat-label">高风险账户 (>80%)</div>
          </div>
        </div>
      </el-col>

      <el-col :span="6">
        <div class="stat-card critical">
          <div class="stat-icon">
            <i class="el-icon-warning-outline"></i>
          </div>
          <div class="stat-content">
            <div class="stat-value">{{ statistics.criticalRiskCount }}</div>
            <div class="stat-label">临近爆仓 (>90%)</div>
          </div>
        </div>
      </el-col>

      <el-col :span="6">
        <div class="stat-card warning">
          <div class="stat-icon">
            <i class="el-icon-s-finance"></i>
          </div>
          <div class="stat-content">
            <div class="stat-value">{{ statistics.todayLiquidations }}</div>
            <div class="stat-label">今日强平次数</div>
          </div>
        </div>
      </el-col>

      <el-col :span="6">
        <div class="stat-card info">
          <div class="stat-icon">
            <i class="el-icon-data-line"></i>
          </div>
          <div class="stat-content">
            <div class="stat-value">{{ (statistics.averageRiskRatio * 100).toFixed(1) }}%</div>
            <div class="stat-label">平均风险率</div>
          </div>
        </div>
      </el-col>
    </el-row>

    <!-- 标签页 -->
    <el-tabs v-model="activeTab" class="tabs-container">
      <!-- 实时风险监控 -->
      <el-tab-pane label="实时风险监控" name="realtime">
        <div class="table-toolbar">
          <el-input
            v-model="searchKeyword"
            placeholder="搜索用户ID"
            prefix-icon="el-icon-search"
            style="width: 200px;"
            clearable
          ></el-input>

          <div class="toolbar-right">
            <span style="margin-right: 10px; color: #909399;">
              共 {{ filteredAccounts.length }} 个账户
            </span>
          </div>
        </div>

        <vxe-table
          ref="accountTable"
          :data="filteredAccounts"
          border
          stripe
          resizable
          highlight-hover-row
          :loading="loading"
          :sort-config="{ trigger: 'cell', defaultSort: { field: 'risk_ratio', order: 'desc' } }"
          height="500"
        >
          <vxe-table-column field="user_id" title="账户ID" width="180" sortable></vxe-table-column>
          <vxe-table-column field="balance" title="总权益" width="130" align="right" sortable>
            <template slot-scope="scope">
              {{ scope.row.balance.toLocaleString('zh-CN', { minimumFractionDigits: 2 }) }}
            </template>
          </vxe-table-column>
          <vxe-table-column field="margin_used" title="占用保证金" width="130" align="right" sortable>
            <template slot-scope="scope">
              {{ scope.row.margin_used.toLocaleString('zh-CN', { minimumFractionDigits: 2 }) }}
            </template>
          </vxe-table-column>
          <vxe-table-column field="available" title="可用资金" width="130" align="right" sortable>
            <template slot-scope="scope">
              {{ scope.row.available.toLocaleString('zh-CN', { minimumFractionDigits: 2 }) }}
            </template>
          </vxe-table-column>
          <vxe-table-column field="float_profit" title="浮动盈亏" width="130" align="right" sortable>
            <template slot-scope="scope">
              <span :style="{ color: scope.row.float_profit >= 0 ? '#F56C6C' : '#67C23A' }">
                {{ scope.row.float_profit >= 0 ? '+' : '' }}{{ scope.row.float_profit.toLocaleString('zh-CN', { minimumFractionDigits: 2 }) }}
              </span>
            </template>
          </vxe-table-column>
          <vxe-table-column field="position_count" title="持仓数" width="90" align="center" sortable></vxe-table-column>
          <vxe-table-column field="risk_ratio" title="风险率" width="120" align="right" sortable>
            <template slot-scope="scope">
              <el-tag :type="getRiskTagType(scope.row.risk_ratio)" size="small">
                {{ (scope.row.risk_ratio * 100).toFixed(1) }}%
              </el-tag>
            </template>
          </vxe-table-column>
          <vxe-table-column field="position_count" title="持仓品种数" width="120" align="center" sortable></vxe-table-column>
          <vxe-table-column title="操作" width="150" fixed="right">
            <template slot-scope="scope">
              <el-button
                size="mini"
                type="text"
                @click="viewAccountDetail(scope.row)"
              >
                详情
              </el-button>
              <el-button
                size="mini"
                type="text"
                style="color: #F56C6C"
                v-if="scope.row.risk_ratio >= 0.9"
                @click="forceLiquidate(scope.row)"
              >
                强平
              </el-button>
            </template>
          </vxe-table-column>
        </vxe-table>
      </el-tab-pane>

      <!-- 强平记录 -->
      <el-tab-pane label="强平记录" name="liquidations">
        <div class="table-toolbar">
          <el-date-picker
            v-model="dateRange"
            type="daterange"
            range-separator="至"
            start-placeholder="开始日期"
            end-placeholder="结束日期"
            value-format="yyyy-MM-dd"
            @change="loadLiquidations"
          ></el-date-picker>
        </div>

        <vxe-table
          ref="liquidationTable"
          :data="liquidations"
          border
          stripe
          resizable
          highlight-hover-row
          :loading="liquidationLoading"
          height="500"
        >
          <vxe-table-column field="liquidation_time" title="强平时间" width="180"></vxe-table-column>
          <vxe-table-column field="user_id" title="用户ID" width="150"></vxe-table-column>
          <vxe-table-column field="user_name" title="用户名" width="150"></vxe-table-column>
          <vxe-table-column field="pre_balance" title="强平前权益" width="130" align="right">
            <template slot-scope="scope">
              {{ scope.row.pre_balance.toLocaleString('zh-CN', { minimumFractionDigits: 2 }) }}
            </template>
          </vxe-table-column>
          <vxe-table-column field="loss_amount" title="亏损金额" width="130" align="right">
            <template slot-scope="scope">
              <span style="color: #67C23A">
                -{{ scope.row.loss_amount.toLocaleString('zh-CN', { minimumFractionDigits: 2 }) }}
              </span>
            </template>
          </vxe-table-column>
          <vxe-table-column field="instrument_id" title="强平合约" width="120"></vxe-table-column>
          <vxe-table-column field="liquidation_price" title="强平价格" width="120" align="right">
            <template slot-scope="scope">
              {{ scope.row.liquidation_price.toFixed(2) }}
            </template>
          </vxe-table-column>
          <vxe-table-column field="liquidation_volume" title="强平数量" width="100" align="center"></vxe-table-column>
          <vxe-table-column field="trigger_type" title="触发类型" width="120">
            <template slot-scope="scope">
              <el-tag :type="scope.row.trigger_type === 'auto' ? 'danger' : 'warning'" size="small">
                {{ scope.row.trigger_type === 'auto' ? '自动强平' : '手动强平' }}
              </el-tag>
            </template>
          </vxe-table-column>
          <vxe-table-column field="operator" title="操作员" width="120"></vxe-table-column>
        </vxe-table>
      </el-tab-pane>
    </el-tabs>

    <!-- 账户详情对话框 -->
    <el-dialog
      title="账户详情"
      :visible.sync="detailDialogVisible"
      width="900px"
    >
      <div v-if="currentAccount" v-loading="detailLoading">
        <el-descriptions :column="2" border class="detail-summary">
          <el-descriptions-item label="账户ID">{{ currentAccount.user_id }}</el-descriptions-item>
          <el-descriptions-item label="总权益">{{ formatNumber(accountDetail.info && accountDetail.info.balance || currentAccount.balance) }}</el-descriptions-item>
          <el-descriptions-item label="可用资金">{{ formatNumber(accountDetail.info && accountDetail.info.available || currentAccount.available) }}</el-descriptions-item>
          <el-descriptions-item label="占用保证金">{{ formatNumber(accountDetail.info && accountDetail.info.margin || currentAccount.margin_used || 0) }}</el-descriptions-item>
          <el-descriptions-item label="浮动盈亏">
            <span :style="{ color: (accountDetail.info && accountDetail.info.float_profit || currentAccount.float_profit) >= 0 ? '#F56C6C' : '#67C23A' }">
              {{ formatNumber(accountDetail.info && accountDetail.info.float_profit || currentAccount.float_profit || 0) }}
            </span>
          </el-descriptions-item>
          <el-descriptions-item label="风险率">
            <el-tag :type="getRiskTagType(currentAccount.risk_ratio)">
              {{ (currentAccount.risk_ratio * 100).toFixed(1) }}%
            </el-tag>
          </el-descriptions-item>
        </el-descriptions>

        <el-tabs v-model="detailTab">
          <el-tab-pane label="持仓明细" name="positions">
            <el-table
              :data="accountDetail.positions"
              border
              stripe
              size="mini"
              empty-text="暂无持仓"
            >
              <el-table-column prop="instrument_id" label="合约" width="140" />
              <el-table-column prop="volume_long" label="多头" width="100" align="right" />
              <el-table-column prop="volume_short" label="空头" width="100" align="right" />
              <el-table-column prop="cost_long" label="多头均价" width="120" align="right" />
              <el-table-column prop="cost_short" label="空头均价" width="120" align="right" />
              <el-table-column prop="float_profit" label="浮动盈亏" width="140" align="right">
                <template slot-scope="{ row }">
                  <span :style="{ color: (row.float_profit || 0) >= 0 ? '#F56C6C' : '#67C23A' }">
                    {{ formatNumber(row.float_profit || 0) }}
                  </span>
                </template>
              </el-table-column>
            </el-table>
          </el-tab-pane>
          <el-tab-pane label="订单明细" name="orders">
            <el-table
              :data="accountDetail.orders"
              border
              stripe
              size="mini"
              empty-text="暂无订单"
            >
              <el-table-column prop="order_id" label="订单号" width="160" />
              <el-table-column prop="instrument_id" label="合约" width="120" />
              <el-table-column prop="direction" label="方向" width="80" align="center" />
              <el-table-column prop="offset" label="开平" width="80" align="center" />
              <el-table-column prop="price" label="价格" width="100" align="right" />
              <el-table-column prop="volume" label="数量" width="80" align="right" />
              <el-table-column prop="status" label="状态" width="120" />
            </el-table>
          </el-tab-pane>
        </el-tabs>
      </div>
    </el-dialog>
  </div>
</template>

<script>
import {
  getRiskAccounts,
  getMarginSummary,
  getLiquidationRecords,
  getAccountDetail,
  forceLiquidateAccount
} from '@/api'

export default {
  name: 'RiskMonitoring',
  data() {
    return {
      loading: false,
      liquidationLoading: false,
      autoRefresh: false,
      refreshTimer: null,
      activeTab: 'realtime',
      searchKeyword: '',
      dateRange: [],
      accounts: [],
      liquidations: [],
      statistics: {
        highRiskCount: 0,
        criticalRiskCount: 0,
        todayLiquidations: 0,
        averageRiskRatio: 0
      },
      detailDialogVisible: false,
      detailLoading: false,
      currentAccount: null,
      accountDetail: {
        info: null,
        positions: [],
        orders: []
      }
    }
  },
  computed: {
    filteredAccounts() {
      if (!this.searchKeyword) return this.accounts

      return this.accounts.filter(account =>
        account.user_id.toLowerCase().includes(this.searchKeyword.toLowerCase())
      )
    }
  },
  mounted() {
    this.loadData()
  },
  beforeDestroy() {
    this.stopAutoRefresh()
  },
  methods: {
    // 加载数据
    async loadData() {
      await Promise.all([
        this.loadAccounts(),
        this.loadStatistics(),
        this.loadLiquidations()
      ])
    },

    // 加载账户风险数据
    async loadAccounts() {
      this.loading = true
      try {
        const params = {}
        if (this.searchKeyword) {
          params.user_id = this.searchKeyword
        }

        const response = await getRiskAccounts(params)
        const rows = (response || []).map(item => this.normalizeRiskAccount(item))
        this.accounts = rows
      } catch (error) {
        this.$message.error('加载账户数据失败')
        console.error(error)
      } finally {
        this.loading = false
      }
    },

    // 加载统计数据
    async loadStatistics() {
      try {
        const response = await getMarginSummary()
        if (response.data && response.data.success) {
          const data = response.data.data
          this.statistics = {
            highRiskCount: data.high_risk_count || 0,
            criticalRiskCount: data.critical_risk_count || 0,
            todayLiquidations: data.liquidation_count || 0,
            averageRiskRatio: data.average_risk_ratio || 0
          }
        } else {
          // 如果API失败，从账户数据中计算
          this.statistics = {
            highRiskCount: this.accounts.filter(a => a.risk_ratio >= 0.8).length,
            criticalRiskCount: this.accounts.filter(a => a.risk_ratio >= 0.9).length,
            todayLiquidations: 0,
            averageRiskRatio: this.accounts.length > 0
              ? this.accounts.reduce((sum, a) => sum + a.risk_ratio, 0) / this.accounts.length
              : 0
          }
        }
      } catch (error) {
        console.error('加载统计数据失败', error)
        // 从账户数据中计算
        this.statistics = {
          highRiskCount: this.accounts.filter(a => a.risk_ratio >= 0.8).length,
          criticalRiskCount: this.accounts.filter(a => a.risk_ratio >= 0.9).length,
          todayLiquidations: 0,
          averageRiskRatio: this.accounts.length > 0
            ? this.accounts.reduce((sum, a) => sum + a.risk_ratio, 0) / this.accounts.length
            : 0
        }
      }
    },

    // 加载强平记录
    async loadLiquidations() {
      this.liquidationLoading = true
      try {
        const params = {}
        if (this.dateRange && this.dateRange.length === 2) {
          params.start_date = this.dateRange[0]
          params.end_date = this.dateRange[1]
        }

        const response = await getLiquidationRecords(params)
        if (response.data && response.data.success) {
          this.liquidations = response.data.data || []
        } else {
          const errorMsg = (response.data && response.data.error && response.data.error.message) || '加载强平记录失败'
          this.$message.error(errorMsg)
        }
      } catch (error) {
        this.$message.error('加载强平记录失败')
        console.error(error)
      } finally {
        this.liquidationLoading = false
      }
    },

    normalizeRiskAccount(item) {
      return {
        user_id: item.user_id,
        balance: item.balance || 0,
        available: item.available || 0,
        margin_used: item.margin_used || 0,
        float_profit: item.unrealized_pnl || 0,
        risk_ratio: item.risk_ratio || 0,
        position_count: item.position_count || 0,
        risk_level: item.risk_level || 'low'
      }
    },

    // 获取风险标签颜色
    getRiskTagType(ratio) {
      if (ratio >= 0.9) return 'danger'
      if (ratio >= 0.8) return 'warning'
      if (ratio >= 0.6) return 'info'
      return 'success'
    },

    // 查看账户详情
    async viewAccountDetail(account) {
      this.currentAccount = account
      this.detailDialogVisible = true
      this.detailLoading = true
      try {
        const detail = await getAccountDetail(account.user_id)
        this.accountDetail = {
          info: detail.account_info || account,
          positions: detail.positions || [],
          orders: detail.orders || []
        }
      } catch (error) {
        this.$message.error('加载账户详情失败')
        console.error(error)
        this.accountDetail = {
          info: account,
          positions: [],
          orders: []
        }
      } finally {
        this.detailLoading = false
      }
    },

    // 强平操作
    async forceLiquidate(account) {
      try {
        await this.$confirm(
          `确定要强制平仓账户 ${account.user_id} 吗？当前风险率：${(account.risk_ratio * 100).toFixed(1)}%`,
          '强制平仓确认',
          {
            type: 'warning',
            confirmButtonText: '确定强平',
            cancelButtonText: '取消'
          }
        )

        await forceLiquidateAccount({
          account_id: account.user_id,
          reason: 'Manual force liquidation from admin UI'
        })
        this.$message.success('强平任务已提交')
        this.loadData()
      } catch (error) {
        if (error !== 'cancel') {
          const msg = (error.response && error.response.data && error.response.data.error && error.response.data.error.message) || error.message
          this.$message.error('强平失败：' + msg)
        }
      }
    },

    formatNumber(val) {
      return Number(val || 0).toLocaleString('zh-CN', { minimumFractionDigits: 2 })
    },

    // 切换自动刷新
    toggleAutoRefresh() {
      if (this.autoRefresh) {
        this.startAutoRefresh()
      } else {
        this.stopAutoRefresh()
      }
    },

    // 开始自动刷新
    startAutoRefresh() {
      this.stopAutoRefresh()
      this.refreshTimer = setInterval(() => {
        this.loadData()
      }, 10000) // 每10秒刷新一次
    },

    // 停止自动刷新
    stopAutoRefresh() {
      if (this.refreshTimer) {
        clearInterval(this.refreshTimer)
        this.refreshTimer = null
      }
    }
  }
}
</script>

<style scoped>
.risk-container {
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

.header-actions {
  display: flex;
  align-items: center;
}

.header-actions > * {
  margin-left: 15px;
}

.header-actions > *:first-child {
  margin-left: 0;
}

.stats-row {
  margin-bottom: 20px;
}

.stat-card {
  display: flex;
  align-items: center;
  padding: 20px;
  background: #fff;
  border-radius: 8px;
  box-shadow: 0 2px 4px rgba(0, 0, 0, 0.1);
}

.stat-icon {
  font-size: 40px;
  margin-right: 15px;
}

.stat-card.danger .stat-icon {
  color: #F56C6C;
}

.stat-card.critical .stat-icon {
  color: #E6A23C;
}

.stat-card.warning .stat-icon {
  color: #E6A23C;
}

.stat-card.info .stat-icon {
  color: #409EFF;
}

.stat-value {
  font-size: 28px;
  font-weight: bold;
  color: #303133;
}

.stat-label {
  font-size: 14px;
  color: #909399;
  margin-top: 5px;
}

.tabs-container {
  background: #fff;
  border-radius: 4px;
  box-shadow: 0 2px 4px rgba(0, 0, 0, 0.1);
  padding: 20px;
}

.table-toolbar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 15px;
}

.toolbar-right {
  display: flex;
  align-items: center;
}

.toolbar-right > * {
  margin-left: 10px;
}

.toolbar-right > *:first-child {
  margin-left: 0;
}
</style>
