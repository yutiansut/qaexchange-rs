<template>
  <div ref="container" class="kline-chart-container">
    <!-- ✨ 数据加载提示 @yutiansut @quantaxis -->
    <div v-if="!klineData || klineData.length === 0" class="loading-overlay">
      <div class="loading-content">
        <i class="el-icon-loading"></i>
        <span>等待 {{ symbol }} {{ periodLabel }} K线数据...</span>
      </div>
    </div>
    <div ref="chart" class="kline-chart"></div>
  </div>
</template>

<script>
// ✨ 修复：HQChart导出格式 @yutiansut @quantaxis
// HQChart 有多种导出方式，需要正确处理
import HQChart from 'hqchart'

// ✨ HQChart.Chart 包含 jsChartInit 和 JSChart 构造函数 @yutiansut @quantaxis
const JSChartLib = HQChart.Chart

// ✨ 禁用 HQChart 内部调试日志 @yutiansut @quantaxis
// HQChart 使用 JSConsole 对象控制日志输出（通过 HQChart.Chart.JSConsole 导出）
// 覆盖为空函数可以禁用 JSComplier.Execute、parser 等内部日志
if (JSChartLib && JSChartLib.JSConsole) {
  JSChartLib.JSConsole.Chart.Log = function() {}
  JSChartLib.JSConsole.Complier.Log = function() {}
  if (JSChartLib.JSConsole.JSTable) {
    JSChartLib.JSConsole.JSTable.Log = function() {}
  }
}

// ✨ 禁用 HQChart 默认 API 请求 @yutiansut @quantaxis
// HQChart 默认会向 http://127.0.0.1:8080 发送 HTTP 请求获取数据
// 需要同时设置 JSChart 和 JSComplier 的 Domain
if (JSChartLib) {
  // 1. 设置主图表 API 域名（g_JSChartResource.Domain）
  if (typeof JSChartLib.SetDomain === 'function') {
    JSChartLib.SetDomain('', '')
  }

  // 2. 设置编译器 API 域名（g_JSComplierResource.Domain）
  if (JSChartLib.JSComplier && typeof JSChartLib.JSComplier.SetDomain === 'function') {
    JSChartLib.JSComplier.SetDomain('', '')
  }

  // 3. 直接修改 g_JSChartResource（如果可访问）
  if (typeof JSChartLib.GetResource === 'function') {
    const resource = JSChartLib.GetResource()
    if (resource) {
      resource.Domain = ''
      resource.CacheDomain = ''
      // 禁用指标相关的 API
      if (resource.Index) {
        resource.Index.StockHistoryDayApiUrl = ''
        resource.Index.MarketLongShortApiUrl = ''
        resource.Index.MarketAttentionApiUrl = ''
        resource.Index.MarketHeatApiUrl = ''
        resource.Index.CustomIndexHeatApiUrl = ''
      }
    }
  }
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

  computed: {
    // ✨ 周期标签 @yutiansut @quantaxis
    periodLabel() {
      const labels = {
        0: '日线',
        3: '3秒',
        4: '1分钟',
        5: '5分钟',
        6: '15分钟',
        7: '30分钟',
        8: '60分钟'
      }
      return labels[this.period] || `${this.period}周期`
    }
  },

  data() {
    return {
      jsChart: null,
      option: null,
      isChartReady: false,
      initRetryCount: 0,  // ✨ 初始化重试计数器 @yutiansut @quantaxis
      pendingData: null,  // ✨ 待处理数据（图表未准备好时缓存）@yutiansut @quantaxis
      needsReinit: false, // ✨ 标记是否需要重新初始化（切换周期/合约时）@yutiansut @quantaxis
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
    // ✨ 监听合约变化 @yutiansut @quantaxis
    symbol(newVal, oldVal) {
      if (newVal && newVal !== oldVal) {
        this.pendingData = null
        this.resetFactorHistory()
        this.needsReinit = true
      }
    },

    // ✨ 监听周期变化 @yutiansut @quantaxis
    period(newVal, oldVal) {
      if (newVal !== oldVal) {
        this.pendingData = null
        this.resetFactorHistory()
        this.needsReinit = true
      }
    },

    // ✨ 监听外部K线数据变化 @yutiansut @quantaxis
    klineData: {
      handler(newData, oldData) {
        if (newData && newData.length > 0) {
          if (this.needsReinit) {
            this.needsReinit = false
            this.pendingData = newData
            this.reinitChartFast()
          } else {
            this.updateChartData(newData)
          }
        }
      },
      deep: true,
      immediate: true
    },

    // ✨ 监听因子数据变化 @yutiansut @quantaxis
    factorData: {
      handler(newFactors) {
        if (this.showFactorOverlay && newFactors && Object.keys(newFactors).length > 0) {
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
      setTimeout(() => this.initChart(), 500)
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
    // HQChart格式: [[date, yclose, open, high, low, close, vol, amount], ...]
    convertToHQChartFormat(data) {
      if (!data || data.length === 0) return []

      return data.map(k => {
        const date = new Date(k.datetime)
        let dateNum
        if (this.period === 0) {
          // 日线：YYYYMMDD
          dateNum = date.getFullYear() * 10000 + (date.getMonth() + 1) * 100 + date.getDate()
        } else {
          // 分钟线：YYYYMMDDHHMM
          const datePart = date.getFullYear() * 10000 + (date.getMonth() + 1) * 100 + date.getDate()
          const timePart = date.getHours() * 100 + date.getMinutes()
          dateNum = datePart * 10000 + timePart
        }
        return [dateNum, k.open, k.open, k.high, k.low, k.close, k.volume || 0, k.amount || 0]
      })
    },

    // ✨ 初始化图表 @yutiansut @quantaxis
    initChart() {
      this.onSize()

      const container = this.$refs.container
      const chartEl = this.$refs.chart
      if (!container || !chartEl) return

      if (container.offsetWidth === 0 || container.offsetHeight === 0) {
        // 容器尺寸为0，重试
        if (!this.initRetryCount) this.initRetryCount = 0
        this.initRetryCount++
        if (this.initRetryCount < 10) {
          setTimeout(() => this.initChart(), 200)
        }
        return
      }

      // 自定义 NetworkFilter - 拦截所有 HTTP 请求，返回本地数据 @yutiansut @quantaxis
      // 关键：设置 data.PreventDefault = true 阻止 HQChart 发送默认 HTTP 请求
      const self = this
      const customNetworkFilter = function(data, callback) {
        // 阻止所有默认 HTTP 请求
        data.PreventDefault = true

        const klineRequestTypes = [
          'KLineChartContainer::RequestHistoryData',
          'KLineChartContainer::ReqeustHistoryMinuteData',
          'KLineChartContainer::RequestHistoryMinuteData',
          'KLineChartContainer::RequestMinuteRealtimeData'
        ]

        if (klineRequestTypes.includes(data.Name)) {
          const sourceData = self.pendingData || self.klineData
          callback({
            code: 0,
            symbol: self.symbol,
            name: self.symbol,
            data: self.convertToHQChartFormat(sourceData)
          })
          return true
        }

        // 拦截所有其他请求，返回空数据（避免 HTTP 请求）
        callback({ code: 0, symbol: self.symbol, name: self.symbol, data: [] })
        return true
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
        const hasJsChartInit = JSChartLib && typeof JSChartLib.jsChartInit === 'function'
        const hasJSChart = JSChartLib && typeof JSChartLib.JSChart === 'function'

        if (hasJsChartInit) {
          this.jsChart = JSChartLib.jsChartInit(this.$refs.chart)
        } else if (hasJSChart) {
          this.jsChart = new JSChartLib.JSChart(this.$refs.chart)
        } else {
          throw new Error('Cannot find valid HQChart initialization method')
        }

        this.jsChart.SetOption(this.option)
        this.isChartReady = true
        this.initRetryCount = 0

        // 加载数据
        const dataToLoad = this.pendingData || this.klineData
        if (dataToLoad && dataToLoad.length > 0) {
          this.pendingData = null
          this.$nextTick(() => this.updateChartData(dataToLoad))
        }
      } catch (error) {
        console.error('[KLineChart] Init failed:', error)
      }
    },

    // 重新初始化图表
    reinitChart() {
      if (this.jsChart) {
        this.jsChart.OnDestroy && this.jsChart.OnDestroy()
        this.jsChart = null
        this.isChartReady = false
      }
      this.$nextTick(() => this.initChart())
    },

    // 快速重新初始化
    reinitChartFast() {
      if (this.jsChart) {
        this.jsChart.OnDestroy && this.jsChart.OnDestroy()
        this.jsChart = null
        this.isChartReady = false
      }
      this.$nextTick(() => this.initChart())
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

    // 更新图表数据
    updateChartData(data) {
      if (!data || data.length === 0) return

      if (!this.jsChart || !this.isChartReady) {
        this.pendingData = data
        return
      }

      try {
        if (typeof this.jsChart.ChangeSymbol === 'function') {
          this.jsChart.ChangeSymbol(this.symbol)
        } else if (typeof this.jsChart.ReloadChartData === 'function') {
          this.jsChart.ReloadChartData()
        } else if (typeof this.jsChart.RequestHistoryData === 'function') {
          this.jsChart.RequestHistoryData()
        } else {
          this.reinitChart()
        }
      } catch (error) {
        this.reinitChart()
      }
    },

    // 切换周期
    changePeriod(period) {
      if (!this.jsChart) return
      try {
        this.jsChart.ChangePeriod(period)
      } catch (error) {
        // ignore
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
  position: relative;

  .kline-chart {
    width: 100%;
    height: 100%;
  }

  // ✨ 加载提示样式 @yutiansut @quantaxis
  .loading-overlay {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background-color: rgba(26, 26, 26, 0.9);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 10;

    .loading-content {
      text-align: center;
      color: #a6adc8;

      i {
        font-size: 32px;
        color: #89b4fa;
        margin-bottom: 12px;
        display: block;
        animation: spin 1s linear infinite;
      }

      span {
        font-size: 14px;
      }
    }
  }

  @keyframes spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }
}
</style>
