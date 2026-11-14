<template>
  <div class="factor-dashboard">
    <!-- 顶部合约选择器 -->
    <div class="dashboard-header">
      <div class="header-left">
        <h2 class="dashboard-title">
          <i class="el-icon-data-analysis"></i>
          因子分析中心
        </h2>
        <span class="subtitle">Factor Analytics Center</span>
      </div>
      <div class="header-right">
        <el-select
          v-model="selectedInstruments"
          multiple
          collapse-tags
          placeholder="选择合约"
          class="instrument-select"
          @change="onInstrumentChange"
        >
          <el-option
            v-for="ins in availableInstruments"
            :key="ins"
            :label="ins"
            :value="ins"
          />
        </el-select>
        <el-button-group class="view-toggle">
          <el-button
            :type="viewMode === 'grid' ? 'primary' : 'default'"
            icon="el-icon-menu"
            size="small"
            @click="viewMode = 'grid'"
          />
          <el-button
            :type="viewMode === 'list' ? 'primary' : 'default'"
            icon="el-icon-s-unfold"
            size="small"
            @click="viewMode = 'list'"
          />
        </el-button-group>
      </div>
    </div>

    <!-- 数据状态指示器 -->
    <div class="status-bar">
      <div class="status-item" :class="{ active: hasFactors }">
        <span class="status-dot"></span>
        <span class="status-text">因子数据流</span>
      </div>
      <div class="status-item" :class="{ active: isConnected }">
        <span class="status-dot"></span>
        <span class="status-text">WebSocket</span>
      </div>
      <div class="status-item info">
        <i class="el-icon-time"></i>
        <span class="status-text">{{ lastUpdateTime }}</span>
      </div>
    </div>

    <!-- 因子概览卡片 -->
    <div class="overview-cards" v-if="selectedInstruments.length > 0">
      <div
        v-for="ins in selectedInstruments"
        :key="ins"
        class="overview-card"
        :class="{ active: activeInstrument === ins }"
        @click="activeInstrument = ins"
      >
        <div class="card-header">
          <span class="instrument-name">{{ ins }}</span>
          <span class="trend-badge" :class="getTrendClass(ins)">
            {{ getTrendLabel(ins) }}
          </span>
        </div>
        <div class="card-body">
          <div class="mini-metrics">
            <div class="metric">
              <span class="label">RSI</span>
              <span class="value" :class="getRsiClass(ins)">{{ getFactorDisplay(ins, 'rsi14') }}</span>
            </div>
            <div class="metric">
              <span class="label">MA5</span>
              <span class="value">{{ getFactorDisplay(ins, 'ma5') }}</span>
            </div>
            <div class="metric">
              <span class="label">MACD</span>
              <span class="value" :class="getMacdClass(ins)">{{ getMacdSignal(ins) }}</span>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- 主图表区域 -->
    <div class="charts-section" :class="viewMode">
      <!-- MA 均线趋势图 -->
      <div class="chart-panel">
        <div class="panel-header">
          <h3>
            <i class="el-icon-s-marketing"></i>
            均线趋势 Moving Average
          </h3>
          <div class="panel-actions">
            <el-radio-group v-model="maChartType" size="mini">
              <el-radio-button label="line">折线</el-radio-button>
              <el-radio-button label="area">面积</el-radio-button>
            </el-radio-group>
          </div>
        </div>
        <div class="panel-body">
          <div ref="maChart" class="chart-container"></div>
        </div>
        <div class="panel-footer">
          <div class="legend-items">
            <span class="legend-item ma5"><i></i>MA5</span>
            <span class="legend-item ma10"><i></i>MA10</span>
            <span class="legend-item ma20"><i></i>MA20</span>
          </div>
        </div>
      </div>

      <!-- RSI 动态仪表盘 -->
      <div class="chart-panel">
        <div class="panel-header">
          <h3>
            <i class="el-icon-odometer"></i>
            RSI 强弱指标
          </h3>
          <div class="rsi-zones">
            <span class="zone overbought">超买 >70</span>
            <span class="zone neutral">中性</span>
            <span class="zone oversold">超卖 <30</span>
          </div>
        </div>
        <div class="panel-body">
          <div ref="rsiChart" class="chart-container"></div>
        </div>
      </div>

      <!-- MACD 柱状图 -->
      <div class="chart-panel wide">
        <div class="panel-header">
          <h3>
            <i class="el-icon-s-data"></i>
            MACD 指标
          </h3>
          <div class="macd-signals">
            <span class="signal" :class="{ active: currentMacdSignal === 'golden' }">
              <i class="el-icon-top"></i> 金叉
            </span>
            <span class="signal" :class="{ active: currentMacdSignal === 'death' }">
              <i class="el-icon-bottom"></i> 死叉
            </span>
          </div>
        </div>
        <div class="panel-body">
          <div ref="macdChart" class="chart-container tall"></div>
        </div>
        <div class="panel-footer">
          <div class="legend-items">
            <span class="legend-item dif"><i></i>DIF</span>
            <span class="legend-item dea"><i></i>DEA</span>
            <span class="legend-item hist"><i></i>MACD柱</span>
          </div>
        </div>
      </div>

      <!-- 因子热力图 -->
      <div class="chart-panel">
        <div class="panel-header">
          <h3>
            <i class="el-icon-s-grid"></i>
            因子热力图
          </h3>
        </div>
        <div class="panel-body">
          <div ref="heatmapChart" class="chart-container"></div>
        </div>
      </div>
    </div>

    <!-- 实时因子数据表格 -->
    <div class="data-table-section">
      <div class="section-header">
        <h3>
          <i class="el-icon-document"></i>
          实时因子数据
        </h3>
        <el-button size="mini" icon="el-icon-download" @click="exportData">
          导出
        </el-button>
      </div>
      <el-table
        :empty-text="isConnected ? '暂无因子数据' : 'WebSocket 未连接，因子数据无法接收'"
        :data="factorTableData"
        class="factor-table"
        stripe
        border
        size="small"
      >
        <el-table-column prop="instrument" label="合约" width="120" fixed />
        <el-table-column prop="timestamp" label="时间" min-width="160" />
        <el-table-column prop="ma5" label="MA5" min-width="100">
          <template slot-scope="{ row }">
            <span :class="getValueClass(row.ma5, row.ma10)">{{ formatValue(row.ma5) }}</span>
          </template>
        </el-table-column>
        <el-table-column prop="ma10" label="MA10" min-width="100">
          <template slot-scope="{ row }">
            <span>{{ formatValue(row.ma10) }}</span>
          </template>
        </el-table-column>
        <el-table-column prop="ma20" label="MA20" min-width="100">
          <template slot-scope="{ row }">
            <span>{{ formatValue(row.ma20) }}</span>
          </template>
        </el-table-column>
        <el-table-column prop="ema12" label="EMA12" min-width="100">
          <template slot-scope="{ row }">
            <span>{{ formatValue(row.ema12) }}</span>
          </template>
        </el-table-column>
        <el-table-column prop="ema26" label="EMA26" min-width="100">
          <template slot-scope="{ row }">
            <span>{{ formatValue(row.ema26) }}</span>
          </template>
        </el-table-column>
        <el-table-column prop="rsi14" label="RSI14" min-width="100">
          <template slot-scope="{ row }">
            <span :class="getRsiValueClass(row.rsi14)">{{ formatValue(row.rsi14, 2) }}</span>
          </template>
        </el-table-column>
        <el-table-column prop="macd_dif" label="DIF" min-width="100">
          <template slot-scope="{ row }">
            <span :class="getDifClass(row)">{{ formatValue(row.macd_dif) }}</span>
          </template>
        </el-table-column>
        <el-table-column prop="macd_dea" label="DEA" min-width="100">
          <template slot-scope="{ row }">
            <span>{{ formatValue(row.macd_dea) }}</span>
          </template>
        </el-table-column>
        <el-table-column prop="macd_hist" label="MACD" min-width="100">
          <template slot-scope="{ row }">
            <span :class="row.macd_hist >= 0 ? 'positive' : 'negative'">
              {{ formatValue(row.macd_hist) }}
            </span>
          </template>
        </el-table-column>
        <el-table-column label="信号" width="100" fixed="right">
          <template slot-scope="{ row }">
            <el-tag :type="getSignalType(row)" size="mini">
              {{ getSignalText(row) }}
            </el-tag>
          </template>
        </el-table-column>
      </el-table>
    </div>
  </div>
</template>

<script>
// @yutiansut @quantaxis - 因子分析仪表盘
import { mapGetters } from 'vuex'

// 稳定序列化:键排序后递归展开,保证「同一内容 → 同一字符串」。
// 用它而不是 JSON.stringify:后者不保证键序(虽然 V8 实际稳定,但不是规范承诺),
// 且对 undefined / 函数会静默丢键。@yutiansut @quantaxis
function stableStr (v) {
  if (v === null || typeof v !== 'object') return String(v)
  if (Array.isArray(v)) return '[' + v.map(stableStr).join(',') + ']'
  const ks = Object.keys(v).sort()
  let out = '{'
  for (const k of ks) out += k + ':' + stableStr(v[k]) + ','
  return out + '}'
}

export default {
  name: 'FactorDashboard',

  data() {
    return {
      selectedInstruments: [],
      activeInstrument: '',
      viewMode: 'grid',
      maChartType: 'line',
      charts: {},
      lastUpdateTime: '--:--:--',
      // 历史数据缓存（用于图表）
      historyData: {},
      maxHistoryLength: 60
    }
  },

  computed: {
    // ⚠️ 用**原始值签名**驱动重绘,不要 watch 对象 @yutiansut @quantaxis
    //
    // Vue 2.7 `Watcher.run` 里 `isObject(value)` 让对象值 watcher **无条件触发**,
    // 引用相不相等都不看;`deep: true` 更是每条消息都走一遍。
    // 实测(dashboard 模式静置 20 秒、**因子键数 = 0**):
    //   SET_SNAPSHOT 38 次 → updateCharts 34 次、updateHistory 34 次、
    //   ECharts setOption 34 次、累计 137.9ms。
    // 而且这是**下限** —— updateMa/Rsi/MacdChart 都有 `!activeInstrument` 早退,
    // 此刻只有热力图在真画;一旦有真因子,就是 4 张图 × 每条消息。
    //
    // 我原本判断「有 selectedInstruments.length === 0 守卫,重复触发只是空转」——
    // **错了**:那个守卫只包住「自动选前 3 个合约」,下面三行在守卫之外。
    factorsSig () {
      const f = this.factors || {}
      const keys = Object.keys(f).sort()
      let sig = ''
      for (const k of keys) {
        const v = f[k]
        // ⚠️ 必须**递归**下钻,不能只拼一层 @yutiansut @quantaxis
        //
        // 后端 patch 形状是 `{period, timestamp, values:{ma5, rsi, ...}}`
        // (`diff_handler.rs:1371-1377`)—— **因子值在第二层**。
        // 我第一版注释写「因子值是数值 map」,对形状的判断就是错的:
        // 只拼一层时 `'' + v.values` 得到恒定的 `"[object Object]"`,
        // 于是签名只由 period/timestamp/合约键集驱动,**对因子值完全失明**。
        //
        // 回验实测:同一 timestamp 下把 ma5 从 10 改成 99 再改成 42,
        // 签名一字不变、updateCharts 0 次、setOption 0 次。
        // 它现在还能重绘,只因为后端每次都带新 timestamp —— **那是搭便车,不是设计**;
        // 同一 timestamp 的二次推送/重放/多周期合并推送会被静默丢弃。
        sig += k + '=' + stableStr(v) + ';'
      }
      return sig
    },
    ...mapGetters('websocket', [
      'factors',
      'factor',
      'factorValues',
      'hasFactors',
      'isConnected'
    ]),

    availableInstruments() {
      return Object.keys(this.factors)
    },

    currentMacdSignal() {
      if (!this.activeInstrument) return ''
      const values = this.factorValues(this.activeInstrument)
      if (!values || !values.macd_dif || !values.macd_dea) return ''
      return values.macd_dif > values.macd_dea ? 'golden' : 'death'
    },

    factorTableData() {
      return this.selectedInstruments.map(ins => {
        const values = this.factorValues(ins) || {}
        const factor = this.factor(ins)
        return {
          instrument: ins,
          timestamp: factor && factor.timestamp
            ? new Date(factor.timestamp).toLocaleTimeString()
            : '--:--:--',
          ma5: values.ma5,
          ma10: values.ma10,
          ma20: values.ma20,
          ema12: values.ema12,
          ema26: values.ema26,
          rsi14: values.rsi14,
          macd_dif: values.macd_dif,
          macd_dea: values.macd_dea,
          macd_hist: values.macd_hist
        }
      })
    }
  },

  watch: {
    // 监听签名(原始字符串)而非 factors 对象 —— 理由见 computed 里的 factorsSig
    factorsSig: {
      handler() {
        const newFactors = this.factors || {}
        // 自动选择前3个合约
        if (this.selectedInstruments.length === 0 && Object.keys(newFactors).length > 0) {
          this.selectedInstruments = Object.keys(newFactors).slice(0, 3)
          if (this.selectedInstruments.length > 0) {
            this.activeInstrument = this.selectedInstruments[0]
          }
        }
        // ⚠️ 顺序必须是**先追加历史,再画图** @yutiansut @quantaxis
        //
        // 原顺序是 `updateCharts(); lastUpdateTime; updateHistory()` —— 先画后追加,
        // 于是 `updateCharts` 渲染的永远是**上一次**的 historyData,
        // **最新一个因子值永远不在屏幕上**,要等下一次更新才补上。
        //
        // 回验三条独立证据:
        //   ① 推 ma5=99 后 maLen 仍是 1;推 42 后才变 2
        //   ② 签名里已是 ma5:14、histLen=2,而 ECharts series 仍是 [13]
        //   ③ 抖动臂 30 次推送 → 历史 30、图 29(恒少一)
        //
        // 这个缺陷让 remount 修复也被钝化:图画出来了,但画的是 remount 之前的值;
        // 日线级低频因子会长期停在上一根。
        this.updateHistory()
        this.lastUpdateTime = new Date().toLocaleTimeString()
        this.updateCharts()
      },
      immediate: true
    },

    activeInstrument() {
      this.updateCharts()
    }
  },

  mounted() {
    this.$nextTick(() => {
      this.initCharts()
      // ⚠️ initCharts 之后必须补一次 updateCharts @yutiansut @quantaxis
      //
      // `watch.factorsSig` 是 `immediate: true`,它在 **initState 阶段**就跑了 ——
      // **早于这里的 initCharts()**。那一次 `updateCharts()` 因为
      // `!this.charts.ma` 全部早退,等于白跑;而 initCharts 用空数据建完图后
      // 原本**不再补一次**,于是图表一直空着,直到下一次签名变化。
      //
      // 旧的 `factors: {deep:true}` 掩盖了这个洞:下一条 WS 消息(~0.5s)会
      // 无条件重触发,肉眼看不出来。改成签名式 watch 后洞就露出来了 ——
      // 回验实测:先喂 4 次因子(maSeries0Len=3),切走再切回 ⇒ maSeries0Len=0,
      // 再推一次**同值**(签名不变)仍是 0,要等 timestamp 前进才恢复。
      // 低频因子(如日线周期)会让图长时间空着。
      this.updateCharts()
    })

    // 监听窗口大小变化
    window.addEventListener('resize', this.handleResize)
  },

  beforeDestroy() {
    window.removeEventListener('resize', this.handleResize)
    Object.values(this.charts).forEach(chart => chart && chart.dispose())
  },

  methods: {
    initCharts() {
      this.initMaChart()
      this.initRsiChart()
      this.initMacdChart()
      this.initHeatmapChart()
    },

    initMaChart() {
      if (!this.$refs.maChart) return
      const chart = this.$echarts.init(this.$refs.maChart, 'dark')
      this.charts.ma = chart

      const option = {
        backgroundColor: 'transparent',
        grid: {
          left: '3%',
          right: '4%',
          bottom: '3%',
          top: '10%',
          containLabel: true
        },
        tooltip: {
          trigger: 'axis',
          backgroundColor: 'rgba(30, 30, 46, 0.9)',
          borderColor: '#3d3d5c',
          textStyle: { color: '#cdd6f4' }
        },
        xAxis: {
          type: 'category',
          data: [],
          axisLine: { lineStyle: { color: '#45475a' } },
          axisLabel: { color: '#a6adc8' }
        },
        yAxis: {
          type: 'value',
          axisLine: { show: false },
          splitLine: { lineStyle: { color: '#313244', type: 'dashed' } },
          axisLabel: { color: '#a6adc8' }
        },
        series: [
          {
            name: 'MA5',
            type: 'line',
            smooth: true,
            data: [],
            lineStyle: { color: '#f9e2af', width: 2 },
            itemStyle: { color: '#f9e2af' },
            areaStyle: this.maChartType === 'area'
              ? { color: 'rgba(249, 226, 175, 0.1)' }
              : null
          },
          {
            name: 'MA10',
            type: 'line',
            smooth: true,
            data: [],
            lineStyle: { color: '#89b4fa', width: 2 },
            itemStyle: { color: '#89b4fa' },
            areaStyle: this.maChartType === 'area'
              ? { color: 'rgba(137, 180, 250, 0.1)' }
              : null
          },
          {
            name: 'MA20',
            type: 'line',
            smooth: true,
            data: [],
            lineStyle: { color: '#cba6f7', width: 2 },
            itemStyle: { color: '#cba6f7' },
            areaStyle: this.maChartType === 'area'
              ? { color: 'rgba(203, 166, 247, 0.1)' }
              : null
          }
        ]
      }
      chart.setOption(option)
    },

    initRsiChart() {
      if (!this.$refs.rsiChart) return
      const chart = this.$echarts.init(this.$refs.rsiChart, 'dark')
      this.charts.rsi = chart

      const option = {
        backgroundColor: 'transparent',
        series: [{
          type: 'gauge',
          startAngle: 180,
          endAngle: 0,
          min: 0,
          max: 100,
          splitNumber: 10,
          radius: '90%',
          center: ['50%', '70%'],
          axisLine: {
            lineStyle: {
              width: 20,
              color: [
                [0.3, '#a6e3a1'],  // 超卖区 (绿色)
                [0.7, '#89b4fa'],  // 中性区 (蓝色)
                [1, '#f38ba8']     // 超买区 (红色)
              ]
            }
          },
          pointer: {
            icon: 'path://M2090.36389,615.30999 L2## ...',
            length: '70%',
            width: 8,
            offsetCenter: [0, '-10%'],
            itemStyle: { color: '#cdd6f4' }
          },
          axisTick: {
            length: 8,
            lineStyle: { color: 'auto', width: 1 }
          },
          splitLine: {
            length: 15,
            lineStyle: { color: 'auto', width: 2 }
          },
          axisLabel: {
            color: '#a6adc8',
            fontSize: 11,
            distance: -40,
            formatter: v => {
              if (v === 30) return '超卖'
              if (v === 70) return '超买'
              if (v === 50) return '中性'
              return ''
            }
          },
          title: {
            offsetCenter: [0, '20%'],
            fontSize: 14,
            color: '#a6adc8'
          },
          detail: {
            fontSize: 28,
            offsetCenter: [0, '-10%'],
            valueAnimation: true,
            formatter: v => v.toFixed(1),
            color: '#cdd6f4'
          },
          data: [{ value: 50, name: 'RSI14' }]
        }]
      }
      chart.setOption(option)
    },

    initMacdChart() {
      if (!this.$refs.macdChart) return
      const chart = this.$echarts.init(this.$refs.macdChart, 'dark')
      this.charts.macd = chart

      const option = {
        backgroundColor: 'transparent',
        grid: {
          left: '3%',
          right: '4%',
          bottom: '10%',
          top: '10%',
          containLabel: true
        },
        tooltip: {
          trigger: 'axis',
          backgroundColor: 'rgba(30, 30, 46, 0.9)',
          borderColor: '#3d3d5c',
          textStyle: { color: '#cdd6f4' }
        },
        xAxis: {
          type: 'category',
          data: [],
          axisLine: { lineStyle: { color: '#45475a' } },
          axisLabel: { color: '#a6adc8' }
        },
        yAxis: {
          type: 'value',
          axisLine: { show: false },
          splitLine: { lineStyle: { color: '#313244', type: 'dashed' } },
          axisLabel: { color: '#a6adc8' }
        },
        series: [
          {
            name: 'DIF',
            type: 'line',
            data: [],
            lineStyle: { color: '#f9e2af', width: 2 },
            itemStyle: { color: '#f9e2af' }
          },
          {
            name: 'DEA',
            type: 'line',
            data: [],
            lineStyle: { color: '#89b4fa', width: 2 },
            itemStyle: { color: '#89b4fa' }
          },
          {
            name: 'MACD',
            type: 'bar',
            data: [],
            itemStyle: {
              color: params => params.value >= 0 ? '#a6e3a1' : '#f38ba8'
            }
          }
        ]
      }
      chart.setOption(option)
    },

    initHeatmapChart() {
      if (!this.$refs.heatmapChart) return
      const chart = this.$echarts.init(this.$refs.heatmapChart, 'dark')
      this.charts.heatmap = chart

      const factorNames = ['MA5', 'MA10', 'MA20', 'RSI14', 'DIF', 'DEA']

      const option = {
        backgroundColor: 'transparent',
        tooltip: {
          position: 'top',
          backgroundColor: 'rgba(30, 30, 46, 0.9)',
          borderColor: '#3d3d5c',
          textStyle: { color: '#cdd6f4' }
        },
        grid: {
          left: '15%',
          right: '5%',
          top: '10%',
          bottom: '15%'
        },
        xAxis: {
          type: 'category',
          data: factorNames,
          axisLine: { lineStyle: { color: '#45475a' } },
          axisLabel: { color: '#a6adc8' }
        },
        yAxis: {
          type: 'category',
          data: [],
          axisLine: { lineStyle: { color: '#45475a' } },
          axisLabel: { color: '#a6adc8' }
        },
        visualMap: {
          min: -1,
          max: 1,
          calculable: true,
          orient: 'horizontal',
          left: 'center',
          bottom: '0%',
          inRange: {
            color: ['#f38ba8', '#313244', '#a6e3a1']
          },
          textStyle: { color: '#a6adc8' }
        },
        series: [{
          name: '因子强度',
          type: 'heatmap',
          data: [],
          label: { show: false },
          emphasis: {
            itemStyle: { shadowBlur: 10, shadowColor: 'rgba(0, 0, 0, 0.5)' }
          }
        }]
      }
      chart.setOption(option)
    },

    updateCharts() {
      this.updateMaChart()
      this.updateRsiChart()
      this.updateMacdChart()
      this.updateHeatmapChart()
    },

    updateMaChart() {
      if (!this.charts.ma || !this.activeInstrument) return

      const history = this.historyData[this.activeInstrument] || []
      const times = history.map(h => h.time)
      const ma5Data = history.map(h => h.ma5)
      const ma10Data = history.map(h => h.ma10)
      const ma20Data = history.map(h => h.ma20)

      this.charts.ma.setOption({
        xAxis: { data: times },
        series: [
          { data: ma5Data },
          { data: ma10Data },
          { data: ma20Data }
        ]
      })
    },

    updateRsiChart() {
      if (!this.charts.rsi || !this.activeInstrument) return

      const values = this.factorValues(this.activeInstrument) || {}
      const rsiValue = values.rsi14 || 50

      this.charts.rsi.setOption({
        series: [{
          data: [{ value: rsiValue, name: this.activeInstrument }]
        }]
      })
    },

    updateMacdChart() {
      if (!this.charts.macd || !this.activeInstrument) return

      const history = this.historyData[this.activeInstrument] || []
      const times = history.map(h => h.time)
      const difData = history.map(h => h.macd_dif)
      const deaData = history.map(h => h.macd_dea)
      const histData = history.map(h => h.macd_hist)

      this.charts.macd.setOption({
        xAxis: { data: times },
        series: [
          { data: difData },
          { data: deaData },
          { data: histData }
        ]
      })
    },

    updateHeatmapChart() {
      if (!this.charts.heatmap) return

      const instruments = this.selectedInstruments
      const factorNames = ['MA5', 'MA10', 'MA20', 'RSI14', 'DIF', 'DEA']
      const heatmapData = []

      instruments.forEach((ins, yIndex) => {
        const values = this.factorValues(ins) || {}
        const ma5 = values.ma5 || 0
        const ma10 = values.ma10 || 0
        const ma20 = values.ma20 || 0

        // 归一化计算因子强度
        const factors = [
          ma5 > ma10 ? 1 : (ma5 < ma10 ? -1 : 0),
          ma10 > ma20 ? 1 : (ma10 < ma20 ? -1 : 0),
          ma5 > ma20 ? 1 : (ma5 < ma20 ? -1 : 0),
          values.rsi14 ? (values.rsi14 - 50) / 50 : 0,
          values.macd_dif && values.macd_dea
            ? (values.macd_dif > values.macd_dea ? 1 : -1)
            : 0,
          values.macd_hist ? (values.macd_hist > 0 ? 1 : -1) : 0
        ]

        factors.forEach((v, xIndex) => {
          heatmapData.push([xIndex, yIndex, v])
        })
      })

      this.charts.heatmap.setOption({
        yAxis: { data: instruments },
        series: [{ data: heatmapData }]
      })
    },

    updateHistory() {
      const now = new Date().toLocaleTimeString()

      Object.keys(this.factors).forEach(ins => {
        if (!this.historyData[ins]) {
          // Vue 2 里给对象**新增属性**不会触发响应式,必须用 $set。
          // 今天无害(updateCharts 是命令式读它、没走模板/computed),
          // 但谁哪天把 historyData 绑到模板或 computed 上就会踩 ——
          // 那时表现是「数据在、界面不动」,又一个静默失效。@yutiansut @quantaxis
          this.$set(this.historyData, ins, [])
        }

        const values = this.factorValues(ins) || {}
        this.historyData[ins].push({
          time: now,
          ma5: values.ma5,
          ma10: values.ma10,
          ma20: values.ma20,
          macd_dif: values.macd_dif,
          macd_dea: values.macd_dea,
          macd_hist: values.macd_hist
        })

        // 保持历史长度
        if (this.historyData[ins].length > this.maxHistoryLength) {
          this.historyData[ins].shift()
        }
      })
    },

    handleResize() {
      Object.values(this.charts).forEach(chart => chart && chart.resize())
    },

    onInstrumentChange() {
      if (this.selectedInstruments.length > 0 && !this.selectedInstruments.includes(this.activeInstrument)) {
        this.activeInstrument = this.selectedInstruments[0]
      }
      this.$nextTick(() => {
        this.updateCharts()
      })
    },

    // 辅助方法
    getFactorDisplay(ins, key) {
      const values = this.factorValues(ins) || {}
      const v = values[key]
      return v !== null && v !== undefined ? v.toFixed(2) : '--'
    },

    getTrendClass(ins) {
      const values = this.factorValues(ins) || {}
      if (values.ma5 > values.ma10 && values.ma10 > values.ma20) return 'bullish'
      if (values.ma5 < values.ma10 && values.ma10 < values.ma20) return 'bearish'
      return 'neutral'
    },

    getTrendLabel(ins) {
      const trendClass = this.getTrendClass(ins)
      if (trendClass === 'bullish') return '多头'
      if (trendClass === 'bearish') return '空头'
      return '震荡'
    },

    getRsiClass(ins) {
      const values = this.factorValues(ins) || {}
      const rsi = values.rsi14
      if (rsi > 70) return 'overbought'
      if (rsi < 30) return 'oversold'
      return ''
    },

    getMacdClass(ins) {
      const values = this.factorValues(ins) || {}
      if (values.macd_dif > values.macd_dea) return 'positive'
      return 'negative'
    },

    getMacdSignal(ins) {
      const values = this.factorValues(ins) || {}
      if (!values.macd_dif || !values.macd_dea) return '--'
      return values.macd_dif > values.macd_dea ? '金叉' : '死叉'
    },

    formatValue(v, decimals = 2) {
      if (v === null || v === undefined) return '--'
      return v.toFixed(decimals)
    },

    getValueClass(v1, v2) {
      if (v1 > v2) return 'positive'
      if (v1 < v2) return 'negative'
      return ''
    },

    getRsiValueClass(rsi) {
      if (rsi > 70) return 'overbought'
      if (rsi < 30) return 'oversold'
      return ''
    },

    getDifClass(row) {
      if (row.macd_dif > row.macd_dea) return 'positive'
      return 'negative'
    },

    getSignalType(row) {
      if (row.rsi14 > 70) return 'danger'
      if (row.rsi14 < 30) return 'success'
      if (row.macd_dif > row.macd_dea) return 'primary'
      return 'info'
    },

    getSignalText(row) {
      if (row.rsi14 > 70) return '超买'
      if (row.rsi14 < 30) return '超卖'
      if (row.macd_dif > row.macd_dea) return '看涨'
      return '看跌'
    },

    exportData() {
      const data = JSON.stringify(this.factorTableData, null, 2)
      const blob = new Blob([data], { type: 'application/json' })
      const url = URL.createObjectURL(blob)
      const a = document.createElement('a')
      a.href = url
      a.download = `factor_data_${Date.now()}.json`
      a.click()
      URL.revokeObjectURL(url)
    }
  }
}
</script>

<style lang="scss" scoped>
// @yutiansut @quantaxis - 因子仪表盘样式 (Catppuccin Mocha 主题)
$base: #1e1e2e;
$surface0: #313244;
$surface1: #45475a;
$surface2: #585b70;
$text: #cdd6f4;
$subtext: #a6adc8;
$overlay: #6c7086;

$red: #f38ba8;
$green: #a6e3a1;
$yellow: #f9e2af;
$blue: #89b4fa;
$mauve: #cba6f7;
$teal: #94e2d5;

.factor-dashboard {
  padding: 20px;
  background: $base;
  min-height: 100vh;
  color: $text;
}

// 头部
.dashboard-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 20px;
  padding-bottom: 20px;
  border-bottom: 1px solid $surface1;

  .header-left {
    .dashboard-title {
      margin: 0;
      font-size: 24px;
      font-weight: 600;
      color: $text;
      display: flex;
      align-items: center;
      gap: 10px;

      i {
        color: $blue;
      }
    }

    .subtitle {
      font-size: 12px;
      color: $subtext;
      margin-left: 34px;
    }
  }

  .header-right {
    display: flex;
    align-items: center;
    gap: 16px;
  }

  .instrument-select {
    width: 300px;

    ::v-deep .el-input__inner {
      background: $surface0;
      border-color: $surface1;
      color: $text;
    }
  }

  .view-toggle {
    ::v-deep .el-button {
      background: $surface0;
      border-color: $surface1;
      color: $subtext;

      &.el-button--primary {
        background: $blue;
        border-color: $blue;
        color: $base;
      }
    }
  }
}

// 状态栏
.status-bar {
  display: flex;
  gap: 24px;
  margin-bottom: 20px;
  padding: 12px 16px;
  background: $surface0;
  border-radius: 8px;

  .status-item {
    display: flex;
    align-items: center;
    gap: 8px;
    color: $subtext;

    .status-dot {
      width: 8px;
      height: 8px;
      border-radius: 50%;
      background: $overlay;
    }

    &.active .status-dot {
      background: $green;
      box-shadow: 0 0 8px $green;
    }

    &.info {
      margin-left: auto;
    }
  }
}

// 概览卡片
.overview-cards {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
  gap: 16px;
  margin-bottom: 24px;
}

.overview-card {
  background: $surface0;
  border-radius: 12px;
  padding: 16px;
  cursor: pointer;
  border: 2px solid transparent;
  transition: all 0.3s ease;

  &:hover {
    border-color: $surface2;
    transform: translateY(-2px);
  }

  &.active {
    border-color: $blue;
    background: rgba($blue, 0.1);
  }

  .card-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 12px;

    .instrument-name {
      font-size: 16px;
      font-weight: 600;
      color: $text;
    }

    .trend-badge {
      padding: 4px 10px;
      border-radius: 12px;
      font-size: 12px;
      font-weight: 500;

      &.bullish {
        background: rgba($green, 0.2);
        color: $green;
      }

      &.bearish {
        background: rgba($red, 0.2);
        color: $red;
      }

      &.neutral {
        background: rgba($yellow, 0.2);
        color: $yellow;
      }
    }
  }

  .card-body {
    .mini-metrics {
      display: grid;
      grid-template-columns: repeat(3, 1fr);
      gap: 12px;
    }

    .metric {
      text-align: center;

      .label {
        display: block;
        font-size: 11px;
        color: $subtext;
        margin-bottom: 4px;
      }

      .value {
        font-size: 14px;
        font-weight: 600;
        color: $text;

        &.positive { color: $green; }
        &.negative { color: $red; }
        &.overbought { color: $red; }
        &.oversold { color: $green; }
      }
    }
  }
}

// 图表区域
.charts-section {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: 20px;
  margin-bottom: 24px;

  &.list {
    grid-template-columns: 1fr;
  }
}

.chart-panel {
  background: $surface0;
  border-radius: 12px;
  overflow: hidden;

  &.wide {
    grid-column: 1 / -1;
  }

  .panel-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 16px 20px;
    border-bottom: 1px solid $surface1;

    h3 {
      margin: 0;
      font-size: 14px;
      font-weight: 600;
      color: $text;
      display: flex;
      align-items: center;
      gap: 8px;

      i {
        color: $blue;
      }
    }

    .rsi-zones {
      display: flex;
      gap: 12px;

      .zone {
        font-size: 11px;
        padding: 2px 8px;
        border-radius: 4px;

        &.overbought {
          background: rgba($red, 0.2);
          color: $red;
        }
        &.neutral {
          background: rgba($blue, 0.2);
          color: $blue;
        }
        &.oversold {
          background: rgba($green, 0.2);
          color: $green;
        }
      }
    }

    .macd-signals {
      display: flex;
      gap: 12px;

      .signal {
        font-size: 12px;
        color: $subtext;
        opacity: 0.5;

        &.active {
          opacity: 1;
          color: $yellow;
        }
      }
    }

    ::v-deep .el-radio-group {
      .el-radio-button__inner {
        background: $surface1;
        border-color: $surface2;
        color: $subtext;
      }

      .el-radio-button__orig-radio:checked + .el-radio-button__inner {
        background: $blue;
        border-color: $blue;
        color: $base;
      }
    }
  }

  .panel-body {
    padding: 16px;

    .chart-container {
      height: 280px;

      &.tall {
        height: 320px;
      }
    }
  }

  .panel-footer {
    padding: 12px 20px;
    border-top: 1px solid $surface1;

    .legend-items {
      display: flex;
      justify-content: center;
      gap: 24px;

      .legend-item {
        display: flex;
        align-items: center;
        gap: 6px;
        font-size: 12px;
        color: $subtext;

        i {
          width: 16px;
          height: 3px;
          border-radius: 1px;
        }

        &.ma5 i { background: $yellow; }
        &.ma10 i { background: $blue; }
        &.ma20 i { background: $mauve; }
        &.dif i { background: $yellow; }
        &.dea i { background: $blue; }
        &.hist i {
          width: 10px;
          height: 10px;
          background: linear-gradient(180deg, $green 50%, $red 50%);
        }
      }
    }
  }
}

// 数据表格区域
.data-table-section {
  background: $surface0;
  border-radius: 12px;
  padding: 20px;

  .section-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 16px;

    h3 {
      margin: 0;
      font-size: 14px;
      font-weight: 600;
      color: $text;
      display: flex;
      align-items: center;
      gap: 8px;

      i {
        color: $blue;
      }
    }
  }

  .factor-table {
    ::v-deep {
      background: transparent;

      th {
        background: $surface1 !important;
        color: $subtext !important;
        border-color: $surface2 !important;
      }

      td {
        background: $surface0 !important;
        color: $text !important;
        border-color: $surface1 !important;
      }

      tr:hover td {
        background: $surface1 !important;
      }

      .positive { color: $green !important; }
      .negative { color: $red !important; }
      .overbought { color: $red !important; }
      .oversold { color: $green !important; }
    }
  }
}

// 响应式
@media (max-width: 1200px) {
  .charts-section {
    grid-template-columns: 1fr;
  }

  .chart-panel.wide {
    grid-column: 1;
  }
}

@media (max-width: 768px) {
  .dashboard-header {
    flex-direction: column;
    gap: 16px;

    .header-right {
      width: 100%;
      justify-content: space-between;
    }

    .instrument-select {
      flex: 1;
    }
  }

  .overview-cards {
    grid-template-columns: 1fr;
  }
}
</style>
