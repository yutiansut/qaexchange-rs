<template>
  <div class="account-curve-container" v-loading="loading">
    <!-- 页面标题 -->
    <div class="page-header">
      <h2>账户资金曲线</h2>
      <div class="header-controls">
        <el-select
          v-model="selectedAccountId"
          placeholder="选择账户"
          style="width: 240px;"
          :disabled="accountOptions.length === 0"
        >
          <el-option
            v-for="account in accountOptions"
            :key="account.account_id"
            :label="`${account.account_name} (${account.account_id})`"
            :value="account.account_id"
          />
        </el-select>

        <el-radio-group v-model="timeRange" size="small" @change="loadData">
          <el-radio-button label="today">今日</el-radio-button>
          <el-radio-button label="week">本周</el-radio-button>
          <el-radio-button label="month">本月</el-radio-button>
          <el-radio-button label="all">全部</el-radio-button>
        </el-radio-group>
      </div>
    </div>

    <!-- 统计卡片 -->
    <el-row :gutter="20" class="stats-row">
      <el-col :span="6">
        <div class="stat-card">
          <div class="stat-label">累计收益</div>
          <div class="stat-value" :style="{ color: statistics.totalProfit >= 0 ? '#F56C6C' : '#67C23A' }">
            {{ statistics.totalProfit >= 0 ? '+' : '' }}{{ statistics.totalProfit.toLocaleString('zh-CN', { minimumFractionDigits: 2 }) }}
          </div>
          <div class="stat-extra">
            收益率: {{ statistics.totalProfitRate >= 0 ? '+' : '' }}{{ (statistics.totalProfitRate * 100).toFixed(2) }}%
          </div>
        </div>
      </el-col>

      <el-col :span="6">
        <div class="stat-card">
          <div class="stat-label">最大回撤</div>
          <div class="stat-value" style="color: #67C23A">
            -{{ statistics.maxDrawdown.toLocaleString('zh-CN', { minimumFractionDigits: 2 }) }}
          </div>
          <div class="stat-extra">
            回撤率: -{{ (statistics.maxDrawdownRate * 100).toFixed(2) }}%
          </div>
        </div>
      </el-col>

      <el-col :span="6">
        <div class="stat-card">
          <div class="stat-label">盈利天数 / 亏损天数</div>
          <div class="stat-value">
            <span style="color: #F56C6C">{{ statistics.profitDays }}</span>
            /
            <span style="color: #67C23A">{{ statistics.lossDays }}</span>
          </div>
          <div class="stat-extra">
            胜率: {{ (statistics.winRate * 100).toFixed(1) }}%
          </div>
        </div>
      </el-col>

      <el-col :span="6">
        <div class="stat-card">
          <div class="stat-label">平均日收益</div>
          <div class="stat-value" :style="{ color: statistics.avgDailyProfit >= 0 ? '#F56C6C' : '#67C23A' }">
            {{ statistics.avgDailyProfit >= 0 ? '+' : '' }}{{ statistics.avgDailyProfit.toLocaleString('zh-CN', { minimumFractionDigits: 2 }) }}
          </div>
          <div class="stat-extra">
            夏普比率: {{ statistics.sharpeRatio.toFixed(2) }}
          </div>
        </div>
      </el-col>
    </el-row>

    <!-- 权益曲线图表 -->
    <el-card class="chart-card">
      <div slot="header">
        <span>权益曲线</span>
      </div>
      <div id="equity-chart" style="width: 100%; height: 400px;"></div>
    </el-card>

    <!-- 详细数据表格 -->
    <el-card class="data-card">
      <div slot="header">
        <span>每日数据</span>
        <el-button style="float: right;" size="small" type="primary" @click="exportData">
          导出数据
        </el-button>
      </div>

      <vxe-table
        ref="dataTable"
        :data="curveData"
        border
        stripe
        resizable
        highlight-hover-row
        height="300"
      >
        <vxe-table-column field="date" title="日期" width="120"></vxe-table-column>
        <vxe-table-column field="balance" title="权益" width="130" align="right">
          <template slot-scope="{ row }">
            {{ row.balance.toLocaleString('zh-CN', { minimumFractionDigits: 2 }) }}
          </template>
        </vxe-table-column>
        <vxe-table-column field="available" title="可用资金" width="130" align="right">
          <template slot-scope="{ row }">
            {{ row.available.toLocaleString('zh-CN', { minimumFractionDigits: 2 }) }}
          </template>
        </vxe-table-column>
        <vxe-table-column field="margin" title="保证金" width="130" align="right">
          <template slot-scope="{ row }">
            {{ row.margin.toLocaleString('zh-CN', { minimumFractionDigits: 2 }) }}
          </template>
        </vxe-table-column>
        <vxe-table-column field="daily_profit" title="日盈亏" width="130" align="right">
          <template slot-scope="{ row }">
            <span :style="{ color: row.daily_profit >= 0 ? '#F56C6C' : '#67C23A' }">
              {{ row.daily_profit >= 0 ? '+' : '' }}{{ row.daily_profit.toLocaleString('zh-CN', { minimumFractionDigits: 2 }) }}
            </span>
          </template>
        </vxe-table-column>
        <vxe-table-column field="daily_profit_rate" title="日收益率" width="120" align="right">
          <template slot-scope="{ row }">
            <span :style="{ color: row.daily_profit_rate >= 0 ? '#F56C6C' : '#67C23A' }">
              {{ row.daily_profit_rate >= 0 ? '+' : '' }}{{ (row.daily_profit_rate * 100).toFixed(2) }}%
            </span>
          </template>
        </vxe-table-column>
        <vxe-table-column field="trade_count" title="交易笔数" width="100" align="center"></vxe-table-column>
        <vxe-table-column field="commission" title="手续费" width="120" align="right">
          <template slot-scope="{ row }">
            {{ row.commission.toLocaleString('zh-CN', { minimumFractionDigits: 2 }) }}
          </template>
        </vxe-table-column>
      </vxe-table>
    </el-card>
  </div>
</template>

<script>
import * as echarts from 'echarts'
import dayjs from 'dayjs'
import { mapGetters } from 'vuex'
import { getEquityCurve, getUserAccounts } from '@/api'

export default {
  name: 'AccountCurve',
  data() {
    return {
      timeRange: 'month',
      accountOptions: [],
      selectedAccountId: '',
      curveData: [],
      allCurveData: {},
      accountStatistics: {},
      statistics: {
        totalProfit: 0,
        totalProfitRate: 0,
        maxDrawdown: 0,
        maxDrawdownRate: 0,
        profitDays: 0,
        lossDays: 0,
        winRate: 0,
        avgDailyProfit: 0,
        sharpeRatio: 0
      },
      chart: null,
      loading: false
    }
  },
  computed: {
    ...mapGetters(['currentUser'])
  },
  watch: {
    currentUser() {
      this.initialize()
    },
    timeRange() {
      this.applyTimeFilter()
    },
    selectedAccountId() {
      this.applyTimeFilter()
    }
  },
  mounted() {
    this.initChart()
    this.initialize()
  },
  beforeDestroy() {
    if (this.chart) {
      this.chart.dispose()
    }
  },
  methods: {
    async initialize() {
      if (!this.currentUser) {
        this.$message.error('请先登录')
        return
      }
      this.selectedAccountId = ''
      this.allCurveData = {}
      this.accountStatistics = {}
      await this.fetchAccounts()
      await this.fetchEquityCurve()
    },

    async fetchAccounts() {
      try {
        const res = await getUserAccounts(this.currentUser)
        this.accountOptions = res.accounts || []
        if (!this.selectedAccountId && this.accountOptions.length > 0) {
          this.selectedAccountId = this.accountOptions[0].account_id
        }
      } catch (error) {
        this.$message.error('获取账户列表失败')
        console.error(error)
      }
    },

    async fetchEquityCurve() {
      if (!this.currentUser) return
      this.loading = true
      try {
        const res = await getEquityCurve(this.currentUser)
        this.allCurveData = {}
        this.accountStatistics = {}
        ;(res.accounts || []).forEach(account => {
          const id = account.accountId || account.account_id
          this.allCurveData[id] = account.points || []
          this.accountStatistics[id] = account.statistics || null
          if (!this.selectedAccountId) {
            this.selectedAccountId = id
          }
        })
        this.applyTimeFilter()
      } catch (error) {
        this.$message.error('加载资金曲线失败')
        console.error(error)
      } finally {
        this.loading = false
      }
    },

    initChart() {
      const chartDom = document.getElementById('equity-chart')
      this.chart = echarts.init(chartDom)
    },

    applyTimeFilter() {
      if (!this.selectedAccountId) {
        this.curveData = []
        this.statistics = this.calculateStatisticsFromCurve([])
        this.updateChart()
        return
      }

      const points = this.allCurveData[this.selectedAccountId] || []
      const filtered = this.filterByRange(points)
      this.curveData = filtered

      const stats = this.accountStatistics[this.selectedAccountId]
      if (stats) {
        this.statistics = this.normalizeStatistics(stats)
      } else {
        this.statistics = this.calculateStatisticsFromCurve(filtered)
      }

      this.updateChart()
    },

    filterByRange(points) {
      if (!points || points.length === 0) return []
      if (this.timeRange === 'all') {
        return [...points]
      }

      const rangeMap = {
        today: 1,
        week: 7,
        month: 30
      }
      const limit = rangeMap[this.timeRange]
      if (!limit || points.length <= limit) {
        return [...points]
      }
      const start = Math.max(points.length - limit, 0)
      return points.slice(start)
    },

    normalizeStatistics(stats) {
      return {
        totalProfit: stats.totalProfit || 0,
        totalProfitRate: stats.totalProfitRate || 0,
        maxDrawdown: stats.maxDrawdown || 0,
        maxDrawdownRate: stats.maxDrawdownRate || 0,
        profitDays: stats.profitDays || 0,
        lossDays: stats.lossDays || 0,
        winRate: stats.winRate || 0,
        avgDailyProfit: stats.avgDailyProfit || 0,
        sharpeRatio: stats.sharpeRatio || 0
      }
    },

    calculateStatisticsFromCurve(data) {
      if (!data || data.length === 0) {
        return {
          totalProfit: 0,
          totalProfitRate: 0,
          maxDrawdown: 0,
          maxDrawdownRate: 0,
          profitDays: 0,
          lossDays: 0,
          winRate: 0,
          avgDailyProfit: 0,
          sharpeRatio: 0
        }
      }

      const startBalance = data[0].balance
      const endBalance = data[data.length - 1].balance
      const totalProfit = endBalance - startBalance
      const totalProfitRate = startBalance !== 0 ? totalProfit / startBalance : 0

      let maxBalance = startBalance
      let maxDrawdown = 0
      let maxDrawdownRate = 0
      let profitDays = 0
      let lossDays = 0
      const dailyReturns = []

      for (let i = 1; i < data.length; i++) {
        const prev = data[i - 1]
        const current = data[i]
        if (current.balance > maxBalance) {
          maxBalance = current.balance
        }
        const drawdown = maxBalance - current.balance
        const drawdownRate = maxBalance > 0 ? drawdown / maxBalance : 0
        if (drawdown > maxDrawdown) {
          maxDrawdown = drawdown
          maxDrawdownRate = drawdownRate
        }

        const dailyProfit = current.balance - prev.balance
        if (dailyProfit >= 0) {
          profitDays += 1
        } else {
          lossDays += 1
        }

        if (prev.balance !== 0) {
          dailyReturns.push(dailyProfit / prev.balance)
        }
      }

      const avgDailyProfit = data.length > 1 ? totalProfit / (data.length - 1) : 0
      const winRate = profitDays + lossDays > 0 ? profitDays / (profitDays + lossDays) : 0

      const avgReturn = dailyReturns.length
        ? dailyReturns.reduce((sum, r) => sum + r, 0) / dailyReturns.length
        : 0
      const variance = dailyReturns.length
        ? dailyReturns.reduce((sum, r) => sum + Math.pow(r - avgReturn, 2), 0) / dailyReturns.length
        : 0
      const stdDev = Math.sqrt(variance)
      const sharpeRatio = stdDev > 0 ? (avgReturn * Math.sqrt(252)) / stdDev : 0

      return {
        totalProfit,
        totalProfitRate,
        maxDrawdown,
        maxDrawdownRate,
        profitDays,
        lossDays,
        winRate,
        avgDailyProfit,
        sharpeRatio
      }
    },

    updateChart() {
      if (!this.chart) {
        return
      }
      const account = this.accountOptions.find(acc => acc.account_id === this.selectedAccountId)
      const title = account ? `${account.account_name} (${account.account_id})` : this.selectedAccountId || '账户'

      const option = {
        title: {
          text: `${title} 的权益曲线`,
          left: 'center'
        },
        tooltip: {
          trigger: 'axis',
          formatter: (params) => {
            if (!params.length) return ''
            const item = this.curveData[params[0].dataIndex]
            return `
              ${item.date}<br/>
              权益: ${item.balance.toLocaleString()}<br/>
              可用: ${item.available.toLocaleString()}<br/>
              保证金: ${item.margin.toLocaleString()}<br/>
              日盈亏: ${item.daily_profit >= 0 ? '+' : ''}${item.daily_profit.toLocaleString()}
            `
          }
        },
        legend: {
          data: ['权益', '可用资金', '保证金'],
          top: 30
        },
        grid: {
          left: '3%',
          right: '4%',
          bottom: '3%',
          containLabel: true
        },
        xAxis: {
          type: 'category',
          boundaryGap: false,
          data: this.curveData.map(d => d.date)
        },
        yAxis: {
          type: 'value',
          axisLabel: {
            formatter: (value) => `${(value / 10000).toFixed(1)}w`
          }
        },
        series: [
          {
            name: '权益',
            type: 'line',
            data: this.curveData.map(d => d.balance),
            smooth: true,
            itemStyle: { color: '#409EFF' },
            areaStyle: {
              color: new echarts.graphic.LinearGradient(0, 0, 0, 1, [
                { offset: 0, color: 'rgba(64, 158, 255, 0.3)' },
                { offset: 1, color: 'rgba(64, 158, 255, 0.05)' }
              ])
            }
          },
          {
            name: '可用资金',
            type: 'line',
            data: this.curveData.map(d => d.available),
            smooth: true,
            itemStyle: { color: '#67C23A' }
          },
          {
            name: '保证金',
            type: 'line',
            data: this.curveData.map(d => d.margin),
            smooth: true,
            itemStyle: { color: '#E6A23C' }
          }
        ]
      }

      this.chart.setOption(option)
    },

    exportData() {
      if (!this.curveData.length) {
        this.$message.warning('暂无数据可导出')
        return
      }

      const headers = ['日期', '权益', '可用资金', '保证金', '日盈亏', '日收益率', '手续费']
      const rows = this.curveData.map(item => [
        item.date,
        item.balance.toFixed(2),
        item.available.toFixed(2),
        item.margin.toFixed(2),
        item.daily_profit.toFixed(2),
        (item.daily_profit_rate * 100).toFixed(2) + '%',
        item.commission.toFixed(2)
      ])

      const csvContent = [headers.join(','), ...rows.map(row => row.join(','))].join('
')
      const blob = new Blob(['﻿' + csvContent], { type: 'text/csv;charset=utf-8;' })
      const url = window.URL.createObjectURL(blob)
      const link = document.createElement('a')
      const filename = `equity_curve_${this.selectedAccountId || 'account'}_${dayjs().format('YYYYMMDD_HHmmss')}.csv`
      link.href = url
      link.setAttribute('download', filename)
      document.body.appendChild(link)
      link.click()
      document.body.removeChild(link)
      window.URL.revokeObjectURL(url)
    }
  }
}
</script>


<style scoped>
.account-curve-container {
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

.header-controls {
  display: flex;
  align-items: center;
  gap: 15px;
}

.stats-row {
  margin-bottom: 20px;
}

.stat-card {
  padding: 20px;
  background: #fff;
  border-radius: 8px;
  box-shadow: 0 2px 4px rgba(0, 0, 0, 0.1);
}

.stat-label {
  font-size: 14px;
  color: #909399;
  margin-bottom: 10px;
}

.stat-value {
  font-size: 28px;
  font-weight: bold;
  margin-bottom: 5px;
}

.stat-extra {
  font-size: 12px;
  color: #606266;
}

.chart-card, .data-card {
  margin-bottom: 20px;
}
</style>
