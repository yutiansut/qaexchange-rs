<template>
  <div ref="container" class="kline-chart-container">
    <div ref="chart" class="kline-chart"></div>
  </div>
</template>

<script>
import JSCommon from 'hqchart'

export default {
  name: 'KLineChart',

  props: {
    // 合约代码
    symbol: {
      type: String,
      default: 'IF2501'
    },

    // K线周期：0-日线, 4-1分钟, 5-5分钟, 6-15分钟, 7-30分钟, 8-60分钟
    period: {
      type: Number,
      default: 5  // 默认5分钟
    },

    // 复权方式：0-不复权, 1-前复权, 2-后复权
    right: {
      type: Number,
      default: 0
    },

    // K线数据（外部传入）
    klineData: {
      type: Array,
      default: () => []
    }
  },

  data() {
    return {
      jsChart: null,
      option: null
    }
  },

  watch: {
    symbol(newVal) {
      if (this.jsChart && newVal) {
        this.jsChart.ChangeSymbol(newVal)
      }
    },

    period(newVal) {
      if (this.jsChart) {
        this.changePeriod(newVal)
      }
    },

    klineData: {
      handler(newData) {
        if (newData && newData.length > 0) {
          this.updateChart()
        }
      },
      deep: true
    }
  },

  mounted() {
    this.$nextTick(() => {
      this.initChart()
    })
  },

  beforeDestroy() {
    if (this.jsChart) {
      this.jsChart.OnDestroy && this.jsChart.OnDestroy()
      this.jsChart = null
    }
  },

  methods: {
    // 初始化图表
    initChart() {
      console.log('[KLineChart] Initializing chart for:', this.symbol)

      // 调整容器大小
      this.onSize()

      // K线图配置
      this.option = {
        Type: '历史K线图',

        // 窗口指标
        Windows: [
          { Index: 'MA', Modify: false, Change: false },      // 主图：均线
          { Index: 'VOL', Modify: false, Change: false },     // 副图1：成交量
          { Index: 'MACD', Modify: false, Change: false }     // 副图2：MACD
        ],

        IsAutoUpdate: false,  // 手动更新数据
        IsShowRightMenu: true,  // 显示右键菜单
        IsShowCorssCursorInfo: true,  // 显示十字光标信息

        Symbol: this.symbol,

        KLine: {
          DragMode: 1,              // 拖拽模式：1-数据拖拽
          Right: this.right,        // 复权方式
          Period: this.period,      // K线周期
          MaxReqeustDataCount: 1000,
          PageSize: 100,            // 一屏显示100根K线
          IsShowTooltip: true       // 显示K线提示信息
        },

        KLineTitle: {
          IsShowName: true,         // 显示股票名称
          IsShowSettingInfo: true   // 显示周期/复权信息
        },

        // 边框间距
        Border: {
          Left: 60,
          Right: 80,
          Top: 25,
          Bottom: 20
        },

        // 子框架设置
        Frame: [
          { SplitCount: 5, StringFormat: 0, Height: 13 },  // 主图K线
          { SplitCount: 3, StringFormat: 0, Height: 4 },   // 副图1：成交量
          { SplitCount: 2, StringFormat: 0, Height: 3 }    // 副图2：MACD
        ],

        Symbol: this.symbol || 'IF2501',

        // 是否自动更新
        IsAutoUpdate: false
      }

      // 创建图表
      try {
        this.jsChart = JSCommon.JSChart.Init(this.$refs.chart)
        this.jsChart.SetOption(this.option)

        console.log('[KLineChart] Chart initialized successfully')
      } catch (error) {
        console.error('[KLineChart] Failed to initialize chart:', error)
      }
    },

    // 调整容器大小
    onSize() {
      if (!this.$refs.container || !this.$refs.chart) return

      const container = this.$refs.container
      const chart = this.$refs.chart

      const height = container.offsetHeight
      const width = container.offsetWidth

      chart.style.width = width + 'px'
      chart.style.height = height + 'px'

      if (this.jsChart && height > 0 && width > 0) {
        this.jsChart.OnSize()
      }
    },

    // 更新图表
    updateChart() {
      if (!this.jsChart) return

      // HQChart 会自动刷新
      console.log('[KLineChart] Chart updated')
    },

    // 切换周期
    changePeriod(period) {
      if (!this.jsChart) return

      try {
        this.jsChart.ChangePeriod(period)
        console.log('[KLineChart] Changed period to:', period)
      } catch (error) {
        console.error('[KLineChart] Failed to change period:', error)
      }
    }
  }
}
</script>

<style scoped lang="scss">
.kline-chart-container {
  width: 100%;
  height: 100%;
  background-color: #1a1a1a;

  .kline-chart {
    width: 100%;
    height: 100%;
  }
}
</style>
