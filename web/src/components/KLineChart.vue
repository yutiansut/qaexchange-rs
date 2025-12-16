<template>
  <div ref="container" class="kline-chart-container">
    <div ref="chart" class="kline-chart"></div>
  </div>
</template>

<script>
// ✨ 修复：HQChart导出格式为 module.exports.Chart，需要解构导入 @yutiansut @quantaxis
import { Chart as JSChart } from 'hqchart'

// 创建全局 JSCommon 对象以兼容 HQChart API
const JSCommon = {
  JSChart: JSChart
}

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
    },

    // ✨ 因子数据（从WebSocket实时获取）@yutiansut @quantaxis
    // 格式: { ma5, ma10, ma20, ema12, ema26, rsi14, macd_dif, macd_dea, macd_hist }
    factorData: {
      type: Object,
      default: () => ({})
    },

    // ✨ 是否显示因子叠加层 @yutiansut @quantaxis
    showFactorOverlay: {
      type: Boolean,
      default: true
    },

    // ✨ 需要显示的因子列表 @yutiansut @quantaxis
    enabledFactors: {
      type: Array,
      default: () => ['ma5', 'ma10', 'ma20']
    }
  },

  data() {
    return {
      jsChart: null,
      option: null,
      isChartReady: false,
      initRetryCount: 0,  // ✨ 初始化重试计数器 @yutiansut @quantaxis
      // ✨ 因子历史数据缓存（用于叠加显示）@yutiansut @quantaxis
      factorHistory: {
        ma5: [],
        ma10: [],
        ma20: [],
        ema12: [],
        ema26: []
      },
      maxFactorHistory: 100,  // 最多保存100个因子数据点
      // ✨ 因子颜色配置 @yutiansut @quantaxis
      factorColors: {
        ma5: '#f9e2af',    // 黄色
        ma10: '#89b4fa',   // 蓝色
        ma20: '#cba6f7',   // 紫色
        ema12: '#a6e3a1',  // 绿色
        ema26: '#fab387'   // 橙色
      }
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
    },

    // ✨ 监听因子数据变化 @yutiansut @quantaxis
    factorData: {
      handler(newFactors) {
        if (this.showFactorOverlay && newFactors && Object.keys(newFactors).length > 0) {
          console.log('[KLineChart] Factor data updated:', Object.keys(newFactors))
          this.updateFactorHistory(newFactors)
          this.renderFactorOverlay()
        }
      },
      deep: true
    },

    // ✨ 监听因子显示开关 @yutiansut @quantaxis
    showFactorOverlay(show) {
      if (show) {
        this.renderFactorOverlay()
      } else {
        this.clearFactorOverlay()
      }
    },

    // ✨ 监听启用的因子列表变化 @yutiansut @quantaxis
    enabledFactors() {
      if (this.showFactorOverlay) {
        this.renderFactorOverlay()
      }
    }
  },

  mounted() {
    // ✨ 延迟初始化，确保父容器已渲染完成 @yutiansut @quantaxis
    this.$nextTick(() => {
      setTimeout(() => {
        this.initChart()
      }, 500)  // 延迟500ms，确保CSS已应用
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
      console.log('[KLineChart] 📊 First input data:', data[0])

      return data.map((k, index) => {
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
        const hqBar = [
          dateNum,           // 日期（日线YYYYMMDD，分钟线YYYYMMDDHHMMSS）
          k.open,            // 前收（用开盘价代替）
          k.open,            // 开盘价
          k.high,            // 最高价
          k.low,             // 最低价
          k.close,           // 收盘价
          k.volume || 0,     // 成交量
          k.amount || 0      // 成交额
        ]

        if (index === 0) {
          console.log('[KLineChart] 📊 First HQChart bar:', hqBar)
          console.log('[KLineChart] 📊 Date conversion:', {
            datetime_ms: k.datetime,
            date_object: date.toLocaleString(),
            dateNum: dateNum
          })
        }

        return hqBar
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
      console.log('[KLineChart] Container computed height:', window.getComputedStyle(container).height)

      const parent = container.parentElement
      if (parent) {
        console.log('[KLineChart] Parent element:', parent.className, parent.offsetWidth, 'x', parent.offsetHeight)
      }

      if (container.offsetWidth === 0 || container.offsetHeight === 0) {
        console.error('[KLineChart] ❌ Container has zero dimensions!')

        // 打印父元素链
        const parentChain = []
        let el = container
        for (let i = 0; i < 3; i++) {
          el = el.parentElement
          if (el) {
            parentChain.push(el.className + ' (' + el.offsetWidth + 'x' + el.offsetHeight + ')')
          } else {
            parentChain.push('null')
          }
        }
        console.error('[KLineChart] Parent chain:', parentChain)

        // ⚠️ 最多重试10次，避免无限循环
        if (!this.initRetryCount) this.initRetryCount = 0
        this.initRetryCount++
        if (this.initRetryCount < 10) {
          console.warn('[KLineChart] Retry', this.initRetryCount, '/10 in 200ms')
          setTimeout(() => this.initChart(), 200)
        } else {
          console.error('[KLineChart] ❌ Initialization failed after 10 retries!')
        }
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
        this.initRetryCount = 0  // ✨ 重置重试计数器

        console.log('[KLineChart] ✅ Chart initialized successfully!')

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
    },

    // ============================================================================
    // ✨ 因子叠加相关方法 @yutiansut @quantaxis
    // ============================================================================

    /**
     * 更新因子历史数据
     * @param {Object} factors - 最新因子数据
     */
    updateFactorHistory(factors) {
      const timestamp = Date.now()

      Object.keys(this.factorHistory).forEach(key => {
        if (factors[key] !== undefined && factors[key] !== null) {
          this.factorHistory[key].push({
            time: timestamp,
            value: factors[key]
          })

          // 限制历史长度
          if (this.factorHistory[key].length > this.maxFactorHistory) {
            this.factorHistory[key].shift()
          }
        }
      })
    },

    /**
     * 渲染因子叠加层
     * 由于HQChart不直接支持动态添加线条，使用Canvas叠加方式实现
     */
    renderFactorOverlay() {
      if (!this.$refs.chart || !this.isChartReady) return

      // 获取或创建叠加Canvas
      let overlayCanvas = this.$refs.container.querySelector('.factor-overlay-canvas')
      if (!overlayCanvas) {
        overlayCanvas = document.createElement('canvas')
        overlayCanvas.className = 'factor-overlay-canvas'
        overlayCanvas.style.cssText = `
          position: absolute;
          top: 0;
          left: 0;
          pointer-events: none;
          z-index: 100;
        `
        this.$refs.container.style.position = 'relative'
        this.$refs.container.appendChild(overlayCanvas)
      }

      // 设置Canvas尺寸
      const container = this.$refs.container
      overlayCanvas.width = container.offsetWidth
      overlayCanvas.height = container.offsetHeight

      const ctx = overlayCanvas.getContext('2d')
      ctx.clearRect(0, 0, overlayCanvas.width, overlayCanvas.height)

      // 绘制因子实时值显示区域（右上角）
      this.drawFactorLegend(ctx, overlayCanvas.width, overlayCanvas.height)

      // 如果有足够的历史数据，绘制因子趋势线
      this.enabledFactors.forEach(factorKey => {
        const history = this.factorHistory[factorKey]
        if (history && history.length > 1) {
          this.drawFactorTrendLine(ctx, factorKey, history, overlayCanvas.width, overlayCanvas.height)
        }
      })

      console.log('[KLineChart] Factor overlay rendered')
    },

    /**
     * 绘制因子图例（实时值显示）
     */
    drawFactorLegend(ctx, width, height) {
      const padding = 10
      const lineHeight = 18
      const legendX = width - 150
      let legendY = padding + 30  // 避开K线标题

      // 背景
      ctx.fillStyle = 'rgba(30, 30, 46, 0.85)'
      ctx.roundRect(legendX - 10, legendY - 5, 140, this.enabledFactors.length * lineHeight + 10, 6)
      ctx.fill()

      // 绘制每个因子的实时值
      this.enabledFactors.forEach((factorKey, index) => {
        const y = legendY + index * lineHeight + 12
        const color = this.factorColors[factorKey] || '#cdd6f4'
        const value = this.factorData[factorKey]

        // 颜色指示方块
        ctx.fillStyle = color
        ctx.fillRect(legendX, y - 8, 12, 12)

        // 因子名称
        ctx.fillStyle = '#a6adc8'
        ctx.font = '11px monospace'
        ctx.fillText(factorKey.toUpperCase(), legendX + 18, y)

        // 因子值
        ctx.fillStyle = '#cdd6f4'
        ctx.font = 'bold 11px monospace'
        const displayValue = value !== undefined && value !== null
          ? value.toFixed(2)
          : '--'
        ctx.fillText(displayValue, legendX + 65, y)
      })
    },

    /**
     * 绘制因子趋势线（迷你图）
     */
    drawFactorTrendLine(ctx, factorKey, history, width, height) {
      const color = this.factorColors[factorKey] || '#cdd6f4'
      const miniChartHeight = 30
      const miniChartWidth = 100
      const padding = 10

      // 计算迷你图位置（左下角）
      const factorIndex = this.enabledFactors.indexOf(factorKey)
      const chartX = padding + factorIndex * (miniChartWidth + 20)
      const chartY = height - padding - miniChartHeight - 20

      // 获取数值范围
      const values = history.map(h => h.value).filter(v => v !== null && v !== undefined)
      if (values.length < 2) return

      const minVal = Math.min(...values)
      const maxVal = Math.max(...values)
      const range = maxVal - minVal || 1

      // 绘制迷你图背景
      ctx.fillStyle = 'rgba(30, 30, 46, 0.7)'
      ctx.beginPath()
      ctx.roundRect(chartX - 5, chartY - 5, miniChartWidth + 10, miniChartHeight + 25, 4)
      ctx.fill()

      // 绘制趋势线
      ctx.strokeStyle = color
      ctx.lineWidth = 1.5
      ctx.beginPath()

      values.forEach((val, i) => {
        const x = chartX + (i / (values.length - 1)) * miniChartWidth
        const y = chartY + miniChartHeight - ((val - minVal) / range) * miniChartHeight

        if (i === 0) {
          ctx.moveTo(x, y)
        } else {
          ctx.lineTo(x, y)
        }
      })
      ctx.stroke()

      // 绘制因子标签
      ctx.fillStyle = color
      ctx.font = 'bold 10px sans-serif'
      ctx.fillText(factorKey.toUpperCase(), chartX, chartY + miniChartHeight + 15)
    },

    /**
     * 清除因子叠加层
     */
    clearFactorOverlay() {
      const container = this.$refs.container
      const overlayCanvas = container && container.querySelector('.factor-overlay-canvas')
      if (overlayCanvas) {
        const ctx = overlayCanvas.getContext('2d')
        ctx.clearRect(0, 0, overlayCanvas.width, overlayCanvas.height)
      }
    },

    /**
     * 重置因子历史数据
     */
    resetFactorHistory() {
      Object.keys(this.factorHistory).forEach(key => {
        this.factorHistory[key] = []
      })
    },

    /**
     * 获取因子颜色
     */
    getFactorColor(factorKey) {
      return this.factorColors[factorKey] || '#cdd6f4'
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
