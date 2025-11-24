<template>
  <div ref="container" class="kline-chart-container">
    <div ref="chart" class="kline-chart"></div>
  </div>
</template>

<script>
import JSCommon from 'hqchart'

/**
 * K线图表组件
 *
 * 使用 HQChart 显示K线数据，支持从 WebSocket 接收实时数据
 *
 * @yutiansut @quantaxis
 */
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
    // 格式: [{ datetime, open, high, low, close, volume, amount }, ...]
    klineData: {
      type: Array,
      default: () => []
    }
  },

  data() {
    return {
      jsChart: null,
      option: null,
      isChartReady: false
    }
  },

  watch: {
    symbol(newVal) {
      if (this.jsChart && newVal) {
        console.log('[KLineChart] Symbol changed to:', newVal)
        this.reinitChart()
      }
    },

    period(newVal) {
      if (this.jsChart) {
        console.log('[KLineChart] Period changed to:', newVal)
        this.reinitChart()
      }
    },

    klineData: {
      handler(newData) {
        console.log('[KLineChart] klineData updated, length:', newData ? newData.length : 0)
        if (newData && newData.length > 0) {
          this.updateChartData(newData)
        }
      },
      deep: true,
      immediate: true
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
    // 转换K线数据为HQChart格式 @yutiansut @quantaxis
    // ✨ 修复：分钟K线需要 YYYYMMDDHHMMSS 格式
    // HQChart格式: [[date, yclose, open, high, low, close, vol, amount], ...]
    convertToHQChartFormat(data) {
      if (!data || data.length === 0) {
        console.log('[KLineChart] convertToHQChartFormat: no data')
        return []
      }

      console.log('[KLineChart] Converting', data.length, 'bars, period:', this.period)

      return data.map(k => {
        const date = new Date(k.datetime)

        let dateNum
        if (this.period === 0) {
          // 日线：YYYYMMDD 格式
          dateNum = date.getFullYear() * 10000 +
                   (date.getMonth() + 1) * 100 +
                   date.getDate()
        } else {
          // 分钟线：YYYYMMDDHHMMSS 格式
          // HQChart 分钟K线需要完整的日期时间
          dateNum = date.getFullYear() * 10000000000 +
                   (date.getMonth() + 1) * 100000000 +
                   date.getDate() * 1000000 +
                   date.getHours() * 10000 +
                   date.getMinutes() * 100 +
                   date.getSeconds()
        }

        // HQChart K线数据格式：
        // [日期, 前收, 开, 高, 低, 收, 量, 额]
        // 注意：我们没有前收价，用开盘价代替
        return [
          dateNum,           // 日期（日线YYYYMMDD，分钟线YYYYMMDDHHMMSS）
          k.open,            // 前收（用开盘价代替）
          k.open,            // 开盘价
          k.high,            // 最高价
          k.low,             // 最低价
          k.close,           // 收盘价
          k.volume || 0,     // 成交量
          k.amount || 0      // 成交额
        ]
      })
    },

    // ✨ 初始化图表（使用自定义数据源）@yutiansut @quantaxis
    initChart() {
      console.log('[KLineChart] Initializing chart for:', this.symbol)

      // 调整容器大小
      this.onSize()

      // 检查容器尺寸
      const container = this.$refs.container
      const chartEl = this.$refs.chart
      if (!container || !chartEl) {
        console.error('[KLineChart] Container or chart element not found!')
        return
      }
      console.log('[KLineChart] Container size:', container.offsetWidth, 'x', container.offsetHeight)
      console.log('[KLineChart] Chart element size:', chartEl.offsetWidth, 'x', chartEl.offsetHeight)

      if (container.offsetWidth === 0 || container.offsetHeight === 0) {
        console.warn('[KLineChart] Container has zero dimensions, delaying initialization')
        setTimeout(() => this.initChart(), 100)
        return
      }

      // 转换初始数据
      const hqData = this.convertToHQChartFormat(this.klineData)
      console.log('[KLineChart] Initial data converted:', hqData.length, 'bars')

      // 自定义数据 NetworkFilter - 直接返回本地数据
      const self = this
      const customNetworkFilter = function(data, callback) {
        console.log('[KLineChart] NetworkFilter called, request:', data.Name)

        // 返回K线历史数据
        if (data.Name === 'KLineChartContainer::RequestHistoryData') {
          const klineData = self.convertToHQChartFormat(self.klineData)
          console.log('[KLineChart] Returning', klineData.length, 'K-line bars')

          // HQChart 期望的返回格式
          const result = {
            code: 0,
            symbol: self.symbol,
            name: self.symbol,
            data: klineData
          }
          callback(result)
          return true
        }

        // 其他请求走默认处理
        return false
      }

      // K线图配置
      this.option = {
        Type: '历史K线图',

        // ✨ 使用自定义网络过滤器提供数据
        NetworkFilter: customNetworkFilter,

        // 窗口指标
        Windows: [
          { Index: 'MA', Modify: false, Change: false },      // 主图：均线
          { Index: 'VOL', Modify: false, Change: false }      // 副图：成交量
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
          PageSize: 50,             // 一屏显示50根K线
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
          { SplitCount: 5, StringFormat: 0, Height: 10 },  // 主图K线
          { SplitCount: 3, StringFormat: 0, Height: 3 }    // 副图：成交量
        ]
      }

      // 创建图表
      try {
        this.jsChart = JSCommon.JSChart.Init(this.$refs.chart)
        this.jsChart.SetOption(this.option)
        this.isChartReady = true

        console.log('[KLineChart] Chart initialized successfully')

        // 如果已有数据，触发更新
        if (this.klineData && this.klineData.length > 0) {
          this.$nextTick(() => {
            this.updateChartData(this.klineData)
          })
        }
      } catch (error) {
        console.error('[KLineChart] Failed to initialize chart:', error)
      }
    },

    // ✨ 重新初始化图表（周期/合约变化时）@yutiansut @quantaxis
    reinitChart() {
      if (this.jsChart) {
        this.jsChart.OnDestroy && this.jsChart.OnDestroy()
        this.jsChart = null
        this.isChartReady = false
      }

      this.$nextTick(() => {
        this.initChart()
      })
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

    // ✨ 更新图表数据（核心方法）@yutiansut @quantaxis
    updateChartData(data) {
      if (!this.jsChart || !this.isChartReady) {
        console.log('[KLineChart] Chart not ready, skipping update. jsChart:', !!this.jsChart, 'isChartReady:', this.isChartReady)
        return
      }

      if (!data || data.length === 0) {
        console.log('[KLineChart] No data to update')
        return
      }

      console.log('[KLineChart] Updating chart with', data.length, 'bars')

      // 打印第一条和最后一条数据用于调试
      if (data.length > 0) {
        console.log('[KLineChart] First bar:', JSON.stringify(data[0]))
        console.log('[KLineChart] Last bar:', JSON.stringify(data[data.length - 1]))
      }

      try {
        // 检查可用的更新方法
        const methods = {
          ReloadChartData: typeof this.jsChart.ReloadChartData === 'function',
          RequestHistoryData: typeof this.jsChart.RequestHistoryData === 'function',
          ChangeSymbol: typeof this.jsChart.ChangeSymbol === 'function',
          SetOption: typeof this.jsChart.SetOption === 'function'
        }
        console.log('[KLineChart] Available update methods:', methods)

        // 方法1：使用 ChangeSymbol 触发重新加载
        if (methods.ChangeSymbol) {
          console.log('[KLineChart] Using ChangeSymbol to reload')
          this.jsChart.ChangeSymbol(this.symbol)
        } else if (methods.ReloadChartData) {
          // 方法2：使用 ReloadChartData 重新加载数据
          console.log('[KLineChart] Using ReloadChartData')
          this.jsChart.ReloadChartData()
        } else if (methods.RequestHistoryData) {
          // 方法3：直接请求历史数据
          console.log('[KLineChart] Using RequestHistoryData')
          this.jsChart.RequestHistoryData()
        } else {
          // 方法4：重新初始化图表（最后手段）
          console.log('[KLineChart] No update method available, reinitializing chart')
          this.reinitChart()
        }

        console.log('[KLineChart] Chart data updated successfully')
      } catch (error) {
        console.error('[KLineChart] Failed to update chart data:', error)
        // 出错时尝试重新初始化
        console.log('[KLineChart] Attempting reinit after error')
        this.reinitChart()
      }
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
