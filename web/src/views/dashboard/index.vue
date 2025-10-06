<template>
  <div class="dashboard">
    <div class="stat-cards">
      <el-card class="stat-card">
        <div class="stat-icon accounts">
          <i class="el-icon-user-solid"></i>
        </div>
        <div class="stat-content">
          <div class="stat-title">总账户数</div>
          <div class="stat-value">{{ monitoring.accounts.total_count }}</div>
          <div class="stat-desc">活跃: {{ monitoring.accounts.active_count }}</div>
        </div>
      </el-card>

      <el-card class="stat-card">
        <div class="stat-icon balance">
          <i class="el-icon-wallet"></i>
        </div>
        <div class="stat-content">
          <div class="stat-title">总权益</div>
          <div class="stat-value">¥{{ formatNumber(monitoring.accounts.total_balance) }}</div>
          <div class="stat-desc">可用: ¥{{ formatNumber(monitoring.accounts.total_available) }}</div>
        </div>
      </el-card>

      <el-card class="stat-card">
        <div class="stat-icon margin">
          <i class="el-icon-pie-chart"></i>
        </div>
        <div class="stat-content">
          <div class="stat-title">保证金占用</div>
          <div class="stat-value">¥{{ formatNumber(monitoring.accounts.total_margin_used) }}</div>
          <div class="stat-desc">占用率: {{ marginUtilization }}%</div>
        </div>
      </el-card>

      <el-card class="stat-card">
        <div class="stat-icon orders">
          <i class="el-icon-document"></i>
        </div>
        <div class="stat-content">
          <div class="stat-title">总订单数</div>
          <div class="stat-value">{{ monitoring.orders.total_count }}</div>
          <div class="stat-desc">待成交: {{ monitoring.orders.pending_count }}</div>
        </div>
      </el-card>

      <el-card class="stat-card">
        <div class="stat-icon trades">
          <i class="el-icon-s-data"></i>
        </div>
        <div class="stat-content">
          <div class="stat-title">总成交数</div>
          <div class="stat-value">{{ monitoring.trades.total_count }}</div>
          <div class="stat-desc">成交量: {{ monitoring.trades.total_volume }}</div>
        </div>
      </el-card>

      <el-card class="stat-card">
        <div class="stat-icon storage">
          <i class="el-icon-coin"></i>
        </div>
        <div class="stat-content">
          <div class="stat-title">存储记录</div>
          <div class="stat-value">{{ monitoring.storage.oltp.total_records }}</div>
          <div class="stat-desc">批次: {{ monitoring.storage.oltp.total_batches }}</div>
        </div>
      </el-card>
    </div>

    <el-row :gutter="20" style="margin-top: 20px">
      <el-col :span="12">
        <el-card>
          <div slot="header">
            <span>账户余额分布</span>
          </div>
          <div id="balanceChart" style="height: 300px"></div>
        </el-card>
      </el-col>

      <el-col :span="12">
        <el-card>
          <div slot="header">
            <span>订单状态分布</span>
          </div>
          <div id="orderChart" style="height: 300px"></div>
        </el-card>
      </el-col>
    </el-row>

    <el-row :gutter="20" style="margin-top: 20px">
      <el-col :span="24">
        <el-card>
          <div slot="header">
            <span>OLAP 转换任务状态</span>
          </div>
          <div id="olapChart" style="height: 300px"></div>
        </el-card>
      </el-col>
    </el-row>
  </div>
</template>

<script>
import { mapGetters, mapActions } from 'vuex'

export default {
  name: 'Dashboard',
  computed: {
    ...mapGetters(['monitoring', 'marginUtilization'])
  },
  mounted() {
    this.startAutoRefresh()
    this.$nextTick(() => {
      this.initCharts()
    })
  },
  beforeDestroy() {
    this.stopAutoRefresh()
  },
  methods: {
    ...mapActions(['startAutoRefresh', 'stopAutoRefresh']),

    formatNumber(num) {
      return (num || 0).toFixed(2).replace(/\B(?=(\d{3})+(?!\d))/g, ',')
    },

    initCharts() {
      this.initBalanceChart()
      this.initOrderChart()
      this.initOlapChart()
    },

    initBalanceChart() {
      const chart = this.$echarts.init(document.getElementById('balanceChart'))
      const option = {
        tooltip: {
          trigger: 'item',
          formatter: '{a} <br/>{b}: ¥{c} ({d}%)'
        },
        legend: {
          bottom: 10,
          data: ['总权益', '可用资金', '保证金占用']
        },
        series: [
          {
            name: '账户余额',
            type: 'pie',
            radius: ['40%', '70%'],
            avoidLabelOverlap: false,
            label: {
              show: false,
              position: 'center'
            },
            emphasis: {
              label: {
                show: true,
                fontSize: '20',
                fontWeight: 'bold'
              }
            },
            labelLine: {
              show: false
            },
            data: [
              { value: this.monitoring.accounts.total_balance, name: '总权益' },
              { value: this.monitoring.accounts.total_available, name: '可用资金' },
              { value: this.monitoring.accounts.total_margin_used, name: '保证金占用' }
            ]
          }
        ]
      }
      chart.setOption(option)

      // 监听数据变化并更新
      this.$watch('monitoring.accounts', () => {
        option.series[0].data = [
          { value: this.monitoring.accounts.total_balance, name: '总权益' },
          { value: this.monitoring.accounts.total_available, name: '可用资金' },
          { value: this.monitoring.accounts.total_margin_used, name: '保证金占用' }
        ]
        chart.setOption(option)
      }, { deep: true })
    },

    initOrderChart() {
      const chart = this.$echarts.init(document.getElementById('orderChart'))
      const option = {
        tooltip: {
          trigger: 'item'
        },
        legend: {
          bottom: 10
        },
        series: [
          {
            name: '订单状态',
            type: 'pie',
            radius: '70%',
            data: [
              { value: this.monitoring.orders.pending_count, name: '待成交' },
              { value: this.monitoring.orders.filled_count, name: '已成交' },
              { value: this.monitoring.orders.cancelled_count, name: '已撤销' }
            ],
            emphasis: {
              itemStyle: {
                shadowBlur: 10,
                shadowOffsetX: 0,
                shadowColor: 'rgba(0, 0, 0, 0.5)'
              }
            }
          }
        ]
      }
      chart.setOption(option)

      this.$watch('monitoring.orders', () => {
        option.series[0].data = [
          { value: this.monitoring.orders.pending_count, name: '待成交' },
          { value: this.monitoring.orders.filled_count, name: '已成交' },
          { value: this.monitoring.orders.cancelled_count, name: '已撤销' }
        ]
        chart.setOption(option)
      }, { deep: true })
    },

    initOlapChart() {
      const chart = this.$echarts.init(document.getElementById('olapChart'))
      const option = {
        tooltip: {
          trigger: 'axis',
          axisPointer: {
            type: 'shadow'
          }
        },
        legend: {
          data: ['待转换', '转换中', '成功', '失败']
        },
        xAxis: {
          type: 'category',
          data: ['OLAP 转换任务']
        },
        yAxis: {
          type: 'value'
        },
        series: [
          {
            name: '待转换',
            type: 'bar',
            stack: 'total',
            data: [this.monitoring.storage.olap.pending_tasks]
          },
          {
            name: '转换中',
            type: 'bar',
            stack: 'total',
            data: [this.monitoring.storage.olap.converting_tasks]
          },
          {
            name: '成功',
            type: 'bar',
            stack: 'total',
            data: [this.monitoring.storage.olap.success_tasks]
          },
          {
            name: '失败',
            type: 'bar',
            stack: 'total',
            data: [this.monitoring.storage.olap.failed_tasks]
          }
        ]
      }
      chart.setOption(option)

      this.$watch('monitoring.storage.olap', () => {
        option.series[0].data = [this.monitoring.storage.olap.pending_tasks]
        option.series[1].data = [this.monitoring.storage.olap.converting_tasks]
        option.series[2].data = [this.monitoring.storage.olap.success_tasks]
        option.series[3].data = [this.monitoring.storage.olap.failed_tasks]
        chart.setOption(option)
      }, { deep: true })
    }
  }
}
</script>

<style lang="scss" scoped>
.dashboard {
  padding: 20px;

  .stat-cards {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(250px, 1fr));
    gap: 20px;

    .stat-card {
      ::v-deep .el-card__body {
        display: flex;
        align-items: center;
        padding: 20px;
      }

      .stat-icon {
        width: 60px;
        height: 60px;
        border-radius: 8px;
        display: flex;
        align-items: center;
        justify-content: center;
        font-size: 28px;
        color: #fff;
        margin-right: 15px;

        &.accounts {
          background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
        }

        &.balance {
          background: linear-gradient(135deg, #f093fb 0%, #f5576c 100%);
        }

        &.margin {
          background: linear-gradient(135deg, #4facfe 0%, #00f2fe 100%);
        }

        &.orders {
          background: linear-gradient(135deg, #43e97b 0%, #38f9d7 100%);
        }

        &.trades {
          background: linear-gradient(135deg, #fa709a 0%, #fee140 100%);
        }

        &.storage {
          background: linear-gradient(135deg, #30cfd0 0%, #330867 100%);
        }
      }

      .stat-content {
        flex: 1;

        .stat-title {
          font-size: 14px;
          color: #909399;
          margin-bottom: 8px;
        }

        .stat-value {
          font-size: 24px;
          font-weight: 600;
          color: #303133;
          margin-bottom: 4px;
        }

        .stat-desc {
          font-size: 12px;
          color: #c0c4cc;
        }
      }
    }
  }
}
</style>
