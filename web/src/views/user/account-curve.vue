<template>
  <div class="account-curve-container">
    <!-- 页面标题 -->
    <div class="page-header">
      <h2>账户资金曲线</h2>
      <div class="header-controls">
        <el-select v-model="currentUser" placeholder="选择账户" style="width: 200px;" @change="loadData">
          <el-option
            v-for="user in userList"
            :key="user.user_id"
            :label="`${user.user_name} (${user.user_id})`"
            :value="user.user_id"
          ></el-option>
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

export default {
  name: 'AccountCurve',
  data() {
    return {
      currentUser: 'user1',
      timeRange: 'month',
      userList: [
        { user_id: 'user1', user_name: '张三' },
        { user_id: 'user2', user_name: '李四' },
        { user_id: 'user3', user_name: '王五' }
      ],
      curveData: [],
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
      chart: null
    }
  },
  mounted() {
    this.initChart()
    this.loadData()
  },
  beforeDestroy() {
    if (this.chart) {
      this.chart.dispose()
    }
  },
  methods: {
    // 初始化图表
    initChart() {
      const chartDom = document.getElementById('equity-chart')
      this.chart = echarts.init(chartDom)
    },

    // 加载数据
    async loadData() {
      try {
        // TODO: 调用真实 API
        // const data = await getEquityCurve(this.currentUser, this.timeRange)

        // 生成模拟数据
        this.generateMockData()

        // 计算统计指标
        this.calculateStatistics()

        // 更新图表
        this.updateChart()
      } catch (error) {
        this.$message.error('加载数据失败')
        console.error(error)
      }
    },

    // 生成模拟数据
    generateMockData() {
      const days = {
        'today': 1,
        'week': 7,
        'month': 30,
        'all': 90
      }[this.timeRange]

      this.curveData = []
      let balance = 1000000
      const startDate = dayjs().subtract(days, 'day')

      for (let i = 0; i <= days; i++) {
        const date = startDate.add(i, 'day').format('YYYY-MM-DD')
        const dailyProfit = (Math.random() - 0.45) * 20000
        const prevBalance = i === 0 ? balance : this.curveData[i - 1].balance

        balance = prevBalance + dailyProfit
        const margin = balance * (0.3 + Math.random() * 0.2)
        const available = balance - margin

        this.curveData.push({
          date,
          balance: balance,
          available: available,
          margin: margin,
          daily_profit: dailyProfit,
          daily_profit_rate: dailyProfit / prevBalance,
          trade_count: Math.floor(Math.random() * 10),
          commission: Math.abs(dailyProfit) * 0.0001
        })
      }
    },

    // 计算统计指标
    calculateStatistics() {
      if (this.curveData.length === 0) {
        return
      }

      const startBalance = this.curveData[0].balance - this.curveData[0].daily_profit
      const endBalance = this.curveData[this.curveData.length - 1].balance

      // 累计收益
      this.statistics.totalProfit = endBalance - startBalance
      this.statistics.totalProfitRate = this.statistics.totalProfit / startBalance

      // 最大回撤
      let maxBalance = startBalance
      let maxDrawdown = 0
      let maxDrawdownRate = 0

      this.curveData.forEach(item => {
        if (item.balance > maxBalance) {
          maxBalance = item.balance
        }
        const drawdown = maxBalance - item.balance
        const drawdownRate = drawdown / maxBalance

        if (drawdown > maxDrawdown) {
          maxDrawdown = drawdown
          maxDrawdownRate = drawdownRate
        }
      })

      this.statistics.maxDrawdown = maxDrawdown
      this.statistics.maxDrawdownRate = maxDrawdownRate

      // 盈亏天数
      this.statistics.profitDays = this.curveData.filter(d => d.daily_profit > 0).length
      this.statistics.lossDays = this.curveData.filter(d => d.daily_profit < 0).length
      this.statistics.winRate = this.statistics.profitDays / (this.statistics.profitDays + this.statistics.lossDays)

      // 平均日收益
      const totalDailyProfit = this.curveData.reduce((sum, d) => sum + d.daily_profit, 0)
      this.statistics.avgDailyProfit = totalDailyProfit / this.curveData.length

      // 夏普比率（简化计算）
      const dailyReturns = this.curveData.map(d => d.daily_profit_rate)
      const avgReturn = dailyReturns.reduce((sum, r) => sum + r, 0) / dailyReturns.length
      const variance = dailyReturns.reduce((sum, r) => sum + Math.pow(r - avgReturn, 2), 0) / dailyReturns.length
      const stdDev = Math.sqrt(variance)
      this.statistics.sharpeRatio = stdDev > 0 ? (avgReturn * Math.sqrt(252)) / stdDev : 0
    },

    // 更新图表
    updateChart() {
      const option = {
        title: {
          text: `${this.currentUser} 的权益曲线`,
          left: 'center'
        },
        tooltip: {
          trigger: 'axis',
          formatter: (params) => {
            const data = params[0]
            const item = this.curveData[data.dataIndex]
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
            formatter: (value) => {
              return (value / 10000).toFixed(1) + 'w'
            }
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

    // 导出数据
    exportData() {
      this.$message.info('导出功能开发中...')
      // TODO: 实现 Excel 导出
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
