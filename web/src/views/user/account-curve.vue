<template>
  <div class="account-curve-container" v-loading="loading">
    <!-- 页面标题 -->
    <div class="page-header">
      <h2>账户资金曲线</h2>
      <div class="header-controls">
        <!-- 240px 装不下 `名称 (ACC_32位hex)` —— 最长标签实测需文本 390.75px
             + padding 45px + border 2px = 437.75px,440px 够用(余 2.25px)。
             ⚠️ box-sizing 是 border-box,左右各 1px 边框要算进去 ——
             我第一版漏了它,把余量写成了 4.25px(实际只有一半)。
             ⚠️ 原先用**定值** 440px,理由是「父级 `.el-form-item__content` 是
             shrink-to-fit 的 inline-block,百分比无效」—— 那个判断在桌面宽度下成立,
             但在窄屏制造了**功能不可达**:实测 400px 视口下 `.header-controls`
             溢出 176px,「今日/本周/本月/全部」的 el-radio-group 整体在屏外,
             而 `.content-wrapper` 是 `overflow-x:hidden`,滚都滚不到;
             页面标题还被挤成每行一个字的竖排。
             改回 `width:100% + max-width:440px + min-width:0`,并给父容器加
             `flex-wrap: wrap` —— 宽屏仍是 440px,窄屏能收缩换行。
             @yutiansut @quantaxis -->
        <el-select
          v-model="selectedAccountId"
          placeholder="选择账户"
          :style="{ flex: '1 1 440px', maxWidth: '440px', minWidth: 0 }"
          :disabled="accountOptions.length === 0"
        >
          <el-option
            v-for="account in accountOptions"
            :key="account.account_id"
            :label="`${account.account_name} (${account.account_id})`"
            :value="account.account_id"
          />
        </el-select>

        <!-- ⚠️ 原来这里是 @change="loadData" —— methods 里根本没有 loadData,
             Vue 渲染时报 `Property or method "loadData" is not defined`,
             并把 change 处理器置为 undefined(`Invalid handler for event "change"`)。
             而 watch.timeRange 已经调了 applyTimeFilter,本就不需要这个处理器。
             @yutiansut @quantaxis -->
        <el-radio-group v-model="timeRange" size="small">
          <el-radio-button label="today">今日</el-radio-button>
          <el-radio-button label="week">本周</el-radio-button>
          <el-radio-button label="month">本月</el-radio-button>
          <el-radio-button label="all">全部</el-radio-button>
        </el-radio-group>
      </div>
    </div>

    <!-- 统计卡片 -->
    <el-row :gutter="20" class="stats-row">
      <el-col :xs="24" :sm="12" :lg="6">
        <div class="stat-card">
          <div class="stat-label">累计收益</div>
          <div class="stat-value" :style="{ color: (Number(statistics.totalProfit) || 0) >= 0 ? '#F56C6C' : '#67C23A' }">
            {{ (Number(statistics.totalProfit) || 0) >= 0 ? '+' : '' }}{{ $money(statistics.totalProfit) }}
          </div>
          <div class="stat-extra">
            收益率: {{ statistics.totalProfitRate >= 0 ? '+' : '' }}{{ $pct(statistics.totalProfitRate, 2) }}
          </div>
        </div>
      </el-col>

      <el-col :xs="24" :sm="12" :lg="6">
        <div class="stat-card">
          <div class="stat-label">最大回撤</div>
          <div class="stat-value" style="color: #67C23A">
            -{{ statistics.maxDrawdown.toLocaleString('zh-CN', { minimumFractionDigits: 2, maximumFractionDigits: 2 }) }}
          </div>
          <div class="stat-extra">
            回撤率: -{{ $pct(statistics.maxDrawdownRate, 2) }}
          </div>
        </div>
      </el-col>

      <el-col :xs="24" :sm="12" :lg="6">
        <div class="stat-card">
          <div class="stat-label">盈利天数 / 亏损天数</div>
          <div class="stat-value">
            <span style="color: #F56C6C">{{ statistics.profitDays }}</span>
            /
            <span style="color: #67C23A">{{ statistics.lossDays }}</span>
          </div>
          <div class="stat-extra">
            胜率: {{ $pct(statistics.winRate, 1) }}
          </div>
        </div>
      </el-col>

      <el-col :xs="24" :sm="12" :lg="6">
        <div class="stat-card">
          <div class="stat-label">平均日收益</div>
          <div class="stat-value" :style="{ color: (Number(statistics.avgDailyProfit) || 0) >= 0 ? '#F56C6C' : '#67C23A' }">
            {{ (Number(statistics.avgDailyProfit) || 0) >= 0 ? '+' : '' }}{{ $money(statistics.avgDailyProfit) }}
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

      <el-table

        :empty-text="$emptyText(loadError)"
        ref="dataTable"
        :data="curveData"
        border
        stripe
        height="300"
      >
        <el-table-column prop="date" label="日期" min-width="120"></el-table-column>
        <el-table-column prop="balance" label="权益" min-width="130" align="right" show-overflow-tooltip>
          <template slot-scope="scope">
            {{ $money(scope.row.balance) }}
          </template>
        </el-table-column>
        <el-table-column prop="available" label="可用资金" min-width="130" align="right" show-overflow-tooltip>
          <template slot-scope="scope">
            {{ $money(scope.row.available) }}
          </template>
        </el-table-column>
        <el-table-column prop="margin" label="保证金" min-width="130" align="right" show-overflow-tooltip>
          <template slot-scope="scope">
            {{ $money(scope.row.margin) }}
          </template>
        </el-table-column>
        <el-table-column prop="daily_profit" label="日盈亏" min-width="130" align="right" show-overflow-tooltip>
          <template slot-scope="scope">
            <span :style="{ color: (Number(scope.row.daily_profit) || 0) >= 0 ? '#F56C6C' : '#67C23A' }">
              {{ (Number(scope.row.daily_profit) || 0) >= 0 ? '+' : '' }}{{ $money(scope.row.daily_profit) }}
            </span>
          </template>
        </el-table-column>
        <el-table-column prop="daily_profit_rate" label="日收益率" min-width="120" align="right" show-overflow-tooltip>
          <template slot-scope="scope">
            <span :style="{ color: (Number(scope.row.daily_profit_rate) || 0) >= 0 ? '#F56C6C' : '#67C23A' }">
              {{ (Number(scope.row.daily_profit_rate) || 0) >= 0 ? '+' : '' }}{{ $pct(scope.row.daily_profit_rate, 2) }}
            </span>
          </template>
        </el-table-column>
        <el-table-column prop="trade_count" label="交易笔数" min-width="100" align="center"></el-table-column>
        <el-table-column prop="commission" label="手续费" min-width="120" align="right" show-overflow-tooltip>
          <template slot-scope="scope">
            {{ $money(scope.row.commission) }}
          </template>
        </el-table-column>
      </el-table>
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
      // 加载失败原因 —— 与「真的没数据」区分开,交给 $emptyText 渲染
      // @yutiansut @quantaxis
      loadError: null,
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
      resizeRaf: null,   // ✨ resize 合并帧句柄 @yutiansut @quantaxis
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
    // ✨ 资金曲线图此前没有 resize 处理, 视口变化后 canvas 尺寸不更新 @yutiansut @quantaxis
    window.addEventListener('resize', this.onWindowResize)
  },
  beforeDestroy() {
    window.removeEventListener('resize', this.onWindowResize)
    if (this.resizeRaf) {
      window.cancelAnimationFrame(this.resizeRaf)
      this.resizeRaf = null
    }
    if (this.chart) {
      this.chart.dispose()
    }
  },
  methods: {
    // ✨ 用 rAF 合并 resize 回调 @yutiansut @quantaxis
    onWindowResize() {
      if (this.resizeRaf) return
      this.resizeRaf = window.requestAnimationFrame(() => {
        this.resizeRaf = null
        if (this.chart) this.chart.resize()
      })
    },

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
        this.loadError = this.$errMsg(error, '加载失败')
        this.$message.error('获取账户列表失败')
        console.error(error)
      }
    },

    async fetchEquityCurve() {
      if (!this.currentUser) return
      this.loading = true; this.loadError = null
      try {
        const res = await getEquityCurve(this.currentUser)
        this.allCurveData = {}
        this.accountStatistics = {}
        ;(res.accounts || []).forEach(account => {
          const id = account.accountId || account.account_id
          // Vue 2 新增属性需 $set,理由同 FactorDashboard.updateHistory。@yutiansut @quantaxis
          this.$set(this.allCurveData, id, account.points || [])
          this.$set(this.accountStatistics, id, account.statistics || null)
          if (!this.selectedAccountId) {
            this.selectedAccountId = id
          }
        })
        this.applyTimeFilter()
      } catch (error) {
        this.loadError = this.$errMsg(error, '加载失败')
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
        totalProfit: Number(stats.totalProfit) || 0,
        totalProfitRate: Number(stats.totalProfitRate) || 0,
        maxDrawdown: Number(stats.maxDrawdown) || 0,
        maxDrawdownRate: Number(stats.maxDrawdownRate) || 0,
        profitDays: Number(stats.profitDays) || 0,
        lossDays: Number(stats.lossDays) || 0,
        winRate: Number(stats.winRate) || 0,
        avgDailyProfit: Number(stats.avgDailyProfit) || 0,
        sharpeRatio: Number(stats.sharpeRatio) || 0
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
              权益: ${(Number(item.balance) || 0).toLocaleString()}<br/>
              可用: ${(Number(item.available) || 0).toLocaleString()}<br/>
              保证金: ${(Number(item.margin) || 0).toLocaleString()}<br/>
              日盈亏: ${(Number(item.daily_profit) || 0) >= 0 ? '+' : ''}${(Number(item.daily_profit) || 0).toLocaleString()}
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
        ((Number(item.daily_profit_rate) || 0) * 100).toFixed(2) + '%',
        item.commission.toFixed(2)
      ])

      const csvContent = [headers.join(','), ...rows.map(row => row.join(','))].join('\n')
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


<style lang="scss" scoped>
// @yutiansut @quantaxis - 深色主题样式
$dark-bg-primary: #0d1117;
$dark-bg-secondary: #161b22;
$dark-bg-card: #1c2128;
$dark-bg-tertiary: #21262d;
$dark-border: #30363d;
$dark-text-primary: #f0f6fc;
$dark-text-secondary: #8b949e;
$dark-text-muted: #6e7681;
$primary-color: #1890ff;
$buy-color: #f5222d;
$sell-color: #52c41a;

.account-curve-container {
  padding: 20px;
  background: $dark-bg-primary;
  // ⚠️ 用 --qa-content-h,不要手写 calc(100vh - N)。@yutiansut @quantaxis
  //    原值 -60px 少算了 content-wrapper 的上下 padding(各 20px):
  //    真实 chrome = top-header 56 + padding 40 = 96px(有公告条时再 +40)。
  //    实测本页在无内容可滚时仍有 41px 的死滚动条
  //    (.content-wrapper 的 scrollHeight - clientHeight = 41)。
  //    `layout/index.vue:546` 早就写明「各页面请一律用 var(--qa-content-h)」。
  min-height: var(--qa-content-h, calc(100vh - 96px));
}

.page-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 20px;
}

.page-header h2 {
  margin: 0;
  color: $dark-text-primary !important;
}

.header-controls {
  display: flex;
  // ⚠️ 必须 `flex:1 1 auto` —— 否则子项的 flex-basis 根本不起作用。
  //    @yutiansut @quantaxis
  //
  //    本容器是 `.page-header`(display:flex)里 `flex:0 1 auto` 的收缩项,
  //    宽度取自身 max-content。而它自己是**多行 flex 容器**(flex-wrap:wrap),
  //    Chrome 算多行容器的 max-content = 各项 max-content **直接求和**:
  //      196(select) + 15(gap) + 221(radio) = 432
  //    子项的 `flex-basis:440px` **完全不参与**这个求和 —— 所以无论写
  //    `width:100%` 还是 `flex:1 1 440px`,容器都被定成 432,
  //    单行需要 440+15+221=676 > 432 ⇒ 换行 ⇒ select 被 432 限住。
  //    实测两种写法**逐像素相同**(432 / 2 行 / 79px),1920~768 全中招。
  //    给容器 flex-grow 让它撑到父级剩余空间,basis 才起作用:
  //    实测 ≥1366 变成 440 / 1 行 / 32px;≤1100 仍换行(那是**正确**的,
  //    因为 126(标题)+676 确实超宽)。
  //    ⚠️ 不要改用 `min-width: min(676px, 100%)` —— 768 档会把标题挤成 28px。
  flex: 1 1 auto;
  // ⚠️ 窄屏必须允许换行且子项可收缩,否则整行溢出且不可达。
  //    @yutiansut @quantaxis
  flex-wrap: wrap;
  // ⚠️ select 用 `flex:1 1 440px` 而非 `width:100%`。@yutiansut @quantaxis
  //    `width:100%` 在**固有尺寸阶段按 auto 算** —— el-select 的 max-content
  //    贡献 196px,本容器是 shrink-to-fit 的 flex 项 ⇒ 宽度定为
  //    196+15(gap)+221(radio)=432;布局阶段 `width:100%` 再取 432 ⇒
  //    432+15+221=668 > 432 ⇒ radio 换行,容器由 1 行 32px 变 2 行 79px,
  //    且 select 实际是 432 而非 440(max-width 永不生效)。1920~768 全中招。
  //    明确的 flex-basis 不参与那个循环。
  min-width: 0;
  align-items: center;
  gap: 15px;

  // 下拉选择框
  ::v-deep .el-select .el-input__inner {
    background: $dark-bg-tertiary !important;
    border-color: $dark-border !important;
    color: $dark-text-primary !important;
  }

  // 单选按钮组
  ::v-deep .el-radio-group {
    .el-radio-button__inner {
      background: $dark-bg-tertiary !important;
      border-color: $dark-border !important;
      color: $dark-text-secondary !important;
    }

    .el-radio-button__orig-radio:checked + .el-radio-button__inner {
      background: $primary-color !important;
      border-color: $primary-color !important;
      color: white !important;
    }
  }
}

.stats-row {
  margin-bottom: 20px;
}

.stat-card {
  padding: 20px;
  background: $dark-bg-card !important;
  border: 1px solid $dark-border;
  border-radius: 8px;
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.3);
}

.stat-label {
  font-size: 14px;
  color: $dark-text-secondary !important;
  margin-bottom: 10px;
}

.stat-value {
  font-size: 28px;
  font-weight: bold;
  margin-bottom: 5px;
  font-family: 'JetBrains Mono', monospace;
}

.stat-extra {
  font-size: 12px;
  color: $dark-text-muted !important;
}

.chart-card, .data-card {
  margin-bottom: 20px;

  ::v-deep .el-card {
    background: $dark-bg-card !important;
    border: 1px solid $dark-border !important;

    .el-card__header {
      background: $dark-bg-secondary !important;
      border-bottom: 1px solid $dark-border !important;
      color: $dark-text-primary !important;
    }

    .el-card__body {
      background: $dark-bg-card !important;
    }
  }

  // 表格样式
  ::v-deep .el-table {
    background: transparent !important;
    color: $dark-text-primary !important;

    &::before {
      background-color: $dark-border !important;
    }

    th.el-table__cell {
      background: $dark-bg-secondary !important;
      color: $dark-text-secondary !important;
      border-bottom: 1px solid $dark-border !important;
      font-weight: 600;
    }

    tr {
      background: $dark-bg-card !important;
    }

    td.el-table__cell {
      background: $dark-bg-card !important;
      color: $dark-text-primary !important;
      border-bottom: 1px solid $dark-border !important;
    }

    .el-table__row:hover > td.el-table__cell {
      background: $dark-bg-tertiary !important;
    }

    .el-table__row--striped .el-table__cell {
      background: rgba($dark-bg-tertiary, 0.5) !important;
    }
  }
}

// 下拉选择框样式
::v-deep .el-select-dropdown {
  background: $dark-bg-secondary !important;
  border-color: $dark-border !important;

  .el-select-dropdown__item {
    color: $dark-text-primary !important;

    &:hover, &.hover {
      background: $dark-bg-tertiary !important;
    }

    &.selected {
      color: $primary-color !important;
      font-weight: 600;
    }
  }
}
</style>
