<template>
  <div class="dashboard">
    <!-- 统计卡片区域 -->
    <div class="stat-cards">
      <div class="stat-card">
        <div class="stat-header">
          <div class="stat-icon accounts">
            <i class="el-icon-user-solid"></i>
          </div>
          <div class="stat-trend up">
            <i class="el-icon-top"></i>
            <span>12%</span>
          </div>
        </div>
        <div class="stat-body">
          <div class="stat-value">{{ monitoring.accounts.total_count }}</div>
          <div class="stat-title">总账户数</div>
        </div>
        <div class="stat-footer">
          <span class="label">活跃账户</span>
          <span class="value">{{ monitoring.accounts.active_count }}</span>
        </div>
      </div>

      <div class="stat-card">
        <div class="stat-header">
          <div class="stat-icon balance">
            <i class="el-icon-wallet"></i>
          </div>
          <div class="stat-trend up">
            <i class="el-icon-top"></i>
            <span>8.5%</span>
          </div>
        </div>
        <div class="stat-body">
          <div class="stat-value">¥{{ formatNumber(monitoring.accounts.total_balance) }}</div>
          <div class="stat-title">总权益</div>
        </div>
        <div class="stat-footer">
          <span class="label">可用资金</span>
          <span class="value">¥{{ formatNumber(monitoring.accounts.total_available) }}</span>
        </div>
      </div>

      <div class="stat-card">
        <div class="stat-header">
          <div class="stat-icon margin">
            <i class="el-icon-pie-chart"></i>
          </div>
          <div class="stat-trend" :class="marginUtilization > 50 ? 'warning' : 'normal'">
            <span>{{ marginUtilization }}%</span>
          </div>
        </div>
        <div class="stat-body">
          <div class="stat-value">¥{{ formatNumber(monitoring.accounts.total_margin_used) }}</div>
          <div class="stat-title">保证金占用</div>
        </div>
        <div class="stat-footer">
          <div class="progress-bar">
            <div class="progress" :style="{ width: marginUtilization + '%' }"></div>
          </div>
        </div>
      </div>

      <div class="stat-card">
        <div class="stat-header">
          <div class="stat-icon orders">
            <i class="el-icon-document"></i>
          </div>
          <div class="stat-trend normal">
            <span>{{ monitoring.orders.pending_count }} 待处理</span>
          </div>
        </div>
        <div class="stat-body">
          <div class="stat-value">{{ monitoring.orders.total_count }}</div>
          <div class="stat-title">总订单数</div>
        </div>
        <div class="stat-footer">
          <span class="label">已成交</span>
          <span class="value">{{ monitoring.orders.filled_count }}</span>
        </div>
      </div>

      <div class="stat-card">
        <div class="stat-header">
          <div class="stat-icon trades">
            <i class="el-icon-s-data"></i>
          </div>
          <div class="stat-trend up">
            <i class="el-icon-top"></i>
            <span>15%</span>
          </div>
        </div>
        <div class="stat-body">
          <div class="stat-value">{{ monitoring.trades.total_count }}</div>
          <div class="stat-title">总成交数</div>
        </div>
        <div class="stat-footer">
          <span class="label">成交量</span>
          <span class="value">{{ monitoring.trades.total_volume }}</span>
        </div>
      </div>

      <div class="stat-card">
        <div class="stat-header">
          <div class="stat-icon storage">
            <i class="el-icon-coin"></i>
          </div>
          <div class="stat-trend normal">
            <span>{{ monitoring.storage.oltp.total_batches }} 批次</span>
          </div>
        </div>
        <div class="stat-body">
          <div class="stat-value">{{ formatLargeNumber(monitoring.storage.oltp.total_records) }}</div>
          <div class="stat-title">存储记录</div>
        </div>
        <div class="stat-footer">
          <span class="label">WAL 文件</span>
          <span class="value">{{ monitoring.storage.oltp.total_batches }}</span>
        </div>
      </div>
    </div>

    <!-- 图表区域 -->
    <div class="charts-section">
      <div class="chart-row">
        <div class="chart-card">
          <div class="chart-header">
            <h3>账户资金分布</h3>
            <el-radio-group v-model="balanceChartType" size="mini">
              <el-radio-button label="pie">饼图</el-radio-button>
              <el-radio-button label="bar">柱图</el-radio-button>
            </el-radio-group>
          </div>
          <div class="chart-body">
            <div id="balanceChart" class="chart"></div>
          </div>
        </div>

        <div class="chart-card">
          <div class="chart-header">
            <h3>订单状态分布</h3>
            <div class="chart-legend">
              <span class="legend-item pending">待成交</span>
              <span class="legend-item filled">已成交</span>
              <span class="legend-item cancelled">已撤销</span>
            </div>
          </div>
          <div class="chart-body">
            <div id="orderChart" class="chart"></div>
          </div>
        </div>
      </div>

      <div class="chart-row full">
        <div class="chart-card wide">
          <div class="chart-header">
            <h3>OLAP 转换任务监控</h3>
            <div class="task-stats">
              <span class="task-stat">
                <i class="dot pending"></i>
                待转换: {{ monitoring.storage.olap.pending_tasks }}
              </span>
              <span class="task-stat">
                <i class="dot converting"></i>
                转换中: {{ monitoring.storage.olap.converting_tasks }}
              </span>
              <span class="task-stat">
                <i class="dot success"></i>
                成功: {{ monitoring.storage.olap.success_tasks }}
              </span>
              <span class="task-stat">
                <i class="dot failed"></i>
                失败: {{ monitoring.storage.olap.failed_tasks }}
              </span>
            </div>
          </div>
          <div class="chart-body">
            <div id="olapChart" class="chart"></div>
          </div>
        </div>
      </div>
    </div>

    <!-- 系统状态卡片 -->
    <div class="system-status">
      <div class="status-card">
        <div class="status-icon healthy">
          <i class="el-icon-success"></i>
        </div>
        <div class="status-info">
          <div class="status-title">系统状态</div>
          <div class="status-value">运行正常</div>
        </div>
      </div>
      <div class="status-card">
        <div class="status-icon">
          <i class="el-icon-time"></i>
        </div>
        <div class="status-info">
          <div class="status-title">运行时间</div>
          <div class="status-value">{{ uptime }}</div>
        </div>
      </div>
      <div class="status-card">
        <div class="status-icon">
          <i class="el-icon-cpu"></i>
        </div>
        <div class="status-info">
          <div class="status-title">撮合延迟</div>
          <div class="status-value">P99 < 100μs</div>
        </div>
      </div>
      <div class="status-card">
        <div class="status-icon">
          <i class="el-icon-connection"></i>
        </div>
        <div class="status-info">
          <div class="status-title">WebSocket</div>
          <div class="status-value">{{ wsConnections }} 连接</div>
        </div>
      </div>
    </div>
  </div>
</template>

<script>
// @yutiansut @quantaxis - 系统监控仪表盘
import { mapGetters, mapActions } from 'vuex'
import { getSystemStatus } from '@/api'

export default {
  name: 'Dashboard',
  data() {
    return {
      balanceChartType: 'pie',
      uptime: '0d 0h 0m',
      wsConnections: 0,
      charts: {},
      statusLoading: false
    }
  },
  computed: {
    ...mapGetters(['monitoring', 'marginUtilization'])
  },
  watch: {
    balanceChartType() {
      this.$nextTick(() => {
        this.initBalanceChart()
      })
    }
  },
  mounted() {
    this.startAutoRefresh()
    this.$nextTick(() => {
      this.initCharts()
    })
    this.updateUptime()
    this.uptimeInterval = setInterval(this.updateUptime, 60000)
  },
  beforeDestroy() {
    this.stopAutoRefresh()
    if (this.uptimeInterval) {
      clearInterval(this.uptimeInterval)
    }
    Object.values(this.charts).forEach(chart => chart && chart.dispose())
  },
  methods: {
    ...mapActions(['startAutoRefresh', 'stopAutoRefresh']),

    formatNumber(num) {
      return (num || 0).toFixed(2).replace(/\B(?=(\d{3})+(?!\d))/g, ',')
    },

    formatLargeNumber(num) {
      if (num >= 1000000) {
        return (num / 1000000).toFixed(1) + 'M'
      } else if (num >= 1000) {
        return (num / 1000).toFixed(1) + 'K'
      }
      return num || 0
    },

    async updateUptime() {
      // 从后端 API 获取真实的系统运行状态 @yutiansut @quantaxis
      try {
        const data = await getSystemStatus()
        if (data) {
          this.uptime = data.uptime_display || '0d 0h 0m'
          this.wsConnections = data.ws_connections || 0
        }
      } catch (error) {
        console.error('获取系统状态失败:', error)
        // 失败时保持上一次的值
      }
    },

    initCharts() {
      this.initBalanceChart()
      this.initOrderChart()
      this.initOlapChart()
    },

    initBalanceChart() {
      if (this.charts.balance) {
        this.charts.balance.dispose()
      }

      const chart = this.$echarts.init(document.getElementById('balanceChart'))
      this.charts.balance = chart

      const colors = ['#1890ff', '#52c41a', '#faad14']

      let option
      if (this.balanceChartType === 'pie') {
        option = {
          tooltip: {
            trigger: 'item',
            formatter: '{b}: ¥{c} ({d}%)'
          },
          color: colors,
          series: [{
            name: '账户资金',
            type: 'pie',
            radius: ['45%', '70%'],
            center: ['50%', '50%'],
            avoidLabelOverlap: false,
            itemStyle: {
              borderRadius: 8,
              borderColor: '#fff',
              borderWidth: 2
            },
            label: {
              show: false
            },
            emphasis: {
              label: {
                show: true,
                fontSize: 16,
                fontWeight: 'bold'
              }
            },
            data: [
              { value: this.monitoring.accounts.total_balance, name: '总权益' },
              { value: this.monitoring.accounts.total_available, name: '可用资金' },
              { value: this.monitoring.accounts.total_margin_used, name: '保证金' }
            ]
          }]
        }
      } else {
        option = {
          tooltip: {
            trigger: 'axis',
            formatter: '{b}: ¥{c}'
          },
          color: colors,
          grid: {
            left: '3%',
            right: '4%',
            bottom: '3%',
            top: '10%',
            containLabel: true
          },
          xAxis: {
            type: 'category',
            data: ['总权益', '可用资金', '保证金'],
            axisLine: { lineStyle: { color: '#e4e7ed' } },
            axisLabel: { color: '#606266' }
          },
          yAxis: {
            type: 'value',
            axisLine: { show: false },
            splitLine: { lineStyle: { color: '#e4e7ed', type: 'dashed' } },
            axisLabel: { color: '#909399' }
          },
          series: [{
            type: 'bar',
            data: [
              { value: this.monitoring.accounts.total_balance, itemStyle: { color: colors[0] } },
              { value: this.monitoring.accounts.total_available, itemStyle: { color: colors[1] } },
              { value: this.monitoring.accounts.total_margin_used, itemStyle: { color: colors[2] } }
            ],
            barWidth: '40%',
            itemStyle: {
              borderRadius: [4, 4, 0, 0]
            }
          }]
        }
      }
      chart.setOption(option)

      this.$watch('monitoring.accounts', () => {
        const data = [
          { value: this.monitoring.accounts.total_balance, name: '总权益' },
          { value: this.monitoring.accounts.total_available, name: '可用资金' },
          { value: this.monitoring.accounts.total_margin_used, name: '保证金' }
        ]
        if (this.balanceChartType === 'pie') {
          chart.setOption({ series: [{ data }] })
        } else {
          chart.setOption({
            series: [{
              data: data.map((d, i) => ({ value: d.value, itemStyle: { color: colors[i] } }))
            }]
          })
        }
      }, { deep: true })
    },

    initOrderChart() {
      const chart = this.$echarts.init(document.getElementById('orderChart'))
      this.charts.order = chart

      const option = {
        tooltip: { trigger: 'item' },
        color: ['#faad14', '#52c41a', '#909399'],
        series: [{
          name: '订单状态',
          type: 'pie',
          radius: '70%',
          center: ['50%', '50%'],
          data: [
            { value: this.monitoring.orders.pending_count, name: '待成交' },
            { value: this.monitoring.orders.filled_count, name: '已成交' },
            { value: this.monitoring.orders.cancelled_count, name: '已撤销' }
          ],
          itemStyle: {
            borderRadius: 6,
            borderColor: '#fff',
            borderWidth: 2
          },
          label: {
            formatter: '{b}\n{d}%',
            fontSize: 12
          },
          emphasis: {
            itemStyle: {
              shadowBlur: 10,
              shadowOffsetX: 0,
              shadowColor: 'rgba(0, 0, 0, 0.2)'
            }
          }
        }]
      }
      chart.setOption(option)

      this.$watch('monitoring.orders', () => {
        chart.setOption({
          series: [{
            data: [
              { value: this.monitoring.orders.pending_count, name: '待成交' },
              { value: this.monitoring.orders.filled_count, name: '已成交' },
              { value: this.monitoring.orders.cancelled_count, name: '已撤销' }
            ]
          }]
        })
      }, { deep: true })
    },

    initOlapChart() {
      const chart = this.$echarts.init(document.getElementById('olapChart'))
      this.charts.olap = chart

      const option = {
        tooltip: {
          trigger: 'axis',
          axisPointer: { type: 'shadow' }
        },
        grid: {
          left: '3%',
          right: '4%',
          bottom: '3%',
          top: '10%',
          containLabel: true
        },
        xAxis: {
          type: 'value',
          axisLine: { show: false },
          splitLine: { lineStyle: { color: '#e4e7ed', type: 'dashed' } }
        },
        yAxis: {
          type: 'category',
          data: ['任务状态'],
          axisLine: { lineStyle: { color: '#e4e7ed' } }
        },
        series: [
          {
            name: '待转换',
            type: 'bar',
            stack: 'total',
            data: [this.monitoring.storage.olap.pending_tasks],
            itemStyle: { color: '#faad14', borderRadius: [4, 0, 0, 4] }
          },
          {
            name: '转换中',
            type: 'bar',
            stack: 'total',
            data: [this.monitoring.storage.olap.converting_tasks],
            itemStyle: { color: '#1890ff' }
          },
          {
            name: '成功',
            type: 'bar',
            stack: 'total',
            data: [this.monitoring.storage.olap.success_tasks],
            itemStyle: { color: '#52c41a' }
          },
          {
            name: '失败',
            type: 'bar',
            stack: 'total',
            data: [this.monitoring.storage.olap.failed_tasks],
            itemStyle: { color: '#f5222d', borderRadius: [0, 4, 4, 0] }
          }
        ]
      }
      chart.setOption(option)

      this.$watch('monitoring.storage.olap', () => {
        chart.setOption({
          series: [
            { data: [this.monitoring.storage.olap.pending_tasks] },
            { data: [this.monitoring.storage.olap.converting_tasks] },
            { data: [this.monitoring.storage.olap.success_tasks] },
            { data: [this.monitoring.storage.olap.failed_tasks] }
          ]
        })
      }, { deep: true })
    }
  }
}
</script>

<style lang="scss" scoped>
// @yutiansut @quantaxis - 深色主题仪表盘样式
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
$warning-color: #faad14;
$danger-color: #f5222d;

.dashboard {
  padding: 0;
  background: $dark-bg-primary;
  min-height: 100%;
}

// 统计卡片
.stat-cards {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
  gap: 20px;
  margin-bottom: 24px;
}

.stat-card {
  background: $dark-bg-card !important;
  border: 1px solid $dark-border;
  border-radius: 12px;
  padding: 20px;
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.3);
  transition: all 0.3s ease;

  &:hover {
    transform: translateY(-4px);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
    border-color: rgba($primary-color, 0.3);
  }

  .stat-header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    margin-bottom: 16px;
  }

  .stat-icon {
    width: 48px;
    height: 48px;
    border-radius: 12px;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 24px;
    color: white;

    &.accounts { background: linear-gradient(135deg, #667eea 0%, #764ba2 100%); }
    &.balance { background: linear-gradient(135deg, #f093fb 0%, #f5576c 100%); }
    &.margin { background: linear-gradient(135deg, #4facfe 0%, #00f2fe 100%); }
    &.orders { background: linear-gradient(135deg, #43e97b 0%, #38f9d7 100%); }
    &.trades { background: linear-gradient(135deg, #fa709a 0%, #fee140 100%); }
    &.storage { background: linear-gradient(135deg, #30cfd0 0%, #330867 100%); }
  }

  .stat-trend {
    font-size: 12px;
    padding: 4px 8px;
    border-radius: 4px;
    display: flex;
    align-items: center;
    gap: 4px;

    &.up {
      background: rgba($success-color, 0.15);
      color: $success-color;
    }
    &.down {
      background: rgba($danger-color, 0.15);
      color: $danger-color;
    }
    &.warning {
      background: rgba($warning-color, 0.15);
      color: $warning-color;
    }
    &.normal {
      background: rgba($dark-text-muted, 0.15);
      color: $dark-text-secondary;
    }
  }

  .stat-body {
    margin-bottom: 12px;

    .stat-value {
      font-size: 28px;
      font-weight: 700;
      color: $dark-text-primary !important;
      line-height: 1.2;
      font-family: 'JetBrains Mono', monospace;
    }

    .stat-title {
      font-size: 14px;
      color: $dark-text-secondary !important;
      margin-top: 4px;
    }
  }

  .stat-footer {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding-top: 12px;
    border-top: 1px solid $dark-border;

    .label {
      font-size: 12px;
      color: $dark-text-muted !important;
    }

    .value {
      font-size: 13px;
      font-weight: 600;
      color: $dark-text-secondary !important;
      font-family: 'JetBrains Mono', monospace;
    }

    .progress-bar {
      flex: 1;
      height: 6px;
      background: $dark-bg-tertiary;
      border-radius: 3px;
      overflow: hidden;

      .progress {
        height: 100%;
        background: linear-gradient(90deg, $primary-color, #40a9ff);
        border-radius: 3px;
        transition: width 0.5s ease;
      }
    }
  }
}

// 图表区域
.charts-section {
  margin-bottom: 24px;
}

.chart-row {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: 20px;
  margin-bottom: 20px;

  &.full {
    grid-template-columns: 1fr;
  }
}

.chart-card {
  background: $dark-bg-card !important;
  border: 1px solid $dark-border;
  border-radius: 12px;
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.3);
  overflow: hidden;

  &.wide {
    grid-column: 1 / -1;
  }

  .chart-header {
    padding: 16px 20px;
    border-bottom: 1px solid $dark-border;
    display: flex;
    justify-content: space-between;
    align-items: center;
    background: $dark-bg-secondary !important;

    h3 {
      margin: 0;
      font-size: 16px;
      font-weight: 600;
      color: $dark-text-primary !important;
    }

    .chart-legend {
      display: flex;
      gap: 16px;

      .legend-item {
        font-size: 12px;
        color: $dark-text-secondary !important;

        &::before {
          content: '';
          display: inline-block;
          width: 8px;
          height: 8px;
          border-radius: 50%;
          margin-right: 6px;
        }

        &.pending::before { background: $warning-color; }
        &.filled::before { background: $success-color; }
        &.cancelled::before { background: $dark-text-muted; }
      }
    }

    .task-stats {
      display: flex;
      gap: 16px;

      .task-stat {
        font-size: 12px;
        color: $dark-text-secondary !important;
        display: flex;
        align-items: center;
        gap: 6px;

        .dot {
          width: 8px;
          height: 8px;
          border-radius: 50%;

          &.pending { background: $warning-color; }
          &.converting { background: $primary-color; }
          &.success { background: $success-color; }
          &.failed { background: $danger-color; }
        }
      }
    }
  }

  .chart-body {
    padding: 16px;
    background: $dark-bg-card !important;

    .chart {
      height: 280px;
    }
  }
}

// 单选按钮组 - 深色主题
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

// 系统状态
.system-status {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
  gap: 16px;
}

.status-card {
  background: $dark-bg-card !important;
  border: 1px solid $dark-border;
  border-radius: 12px;
  padding: 16px 20px;
  display: flex;
  align-items: center;
  gap: 16px;
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.3);

  .status-icon {
    width: 44px;
    height: 44px;
    border-radius: 10px;
    background: rgba($primary-color, 0.15);
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 20px;
    color: $primary-color;

    &.healthy {
      background: rgba($success-color, 0.15);
      color: $success-color;
    }
  }

  .status-info {
    .status-title {
      font-size: 12px;
      color: $dark-text-muted !important;
      margin-bottom: 4px;
    }

    .status-value {
      font-size: 15px;
      font-weight: 600;
      color: $dark-text-primary !important;
    }
  }
}

// 响应式
@media (max-width: 768px) {
  .chart-row {
    grid-template-columns: 1fr;
  }

  .stat-card .stat-body .stat-value {
    font-size: 24px;
  }
}
</style>
