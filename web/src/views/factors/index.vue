<template>
  <div class="factors-page">
    <!-- 页面头部 -->
    <div class="page-header">
      <div class="header-content">
        <h1 class="page-title">
          <i class="el-icon-data-analysis"></i>
          因子分析系统
        </h1>
        <p class="page-subtitle">Factor Analysis System - 实时因子计算与可视化</p>
      </div>
      <div class="header-actions">
        <el-tag :type="connectionStatus.type" size="medium">
          <i :class="connectionStatus.icon"></i>
          {{ connectionStatus.text }}
        </el-tag>
        <el-dropdown trigger="click" @command="handleLayoutChange">
          <el-button size="small" icon="el-icon-setting">
            布局 <i class="el-icon-arrow-down el-icon--right"></i>
          </el-button>
          <el-dropdown-menu slot="dropdown">
            <el-dropdown-item command="dashboard">仪表盘模式</el-dropdown-item>
            <el-dropdown-item command="chart">图表模式</el-dropdown-item>
            <el-dropdown-item command="split">分屏模式</el-dropdown-item>
          </el-dropdown-menu>
        </el-dropdown>
      </div>
    </div>

    <!-- 快捷操作栏 -->
    <div class="quick-actions">
      <el-select
        v-model="selectedInstrument"
        placeholder="选择合约"
        filterable
        clearable
        class="instrument-select"
        @change="onInstrumentSelect"
      >
        <el-option
          v-for="ins in availableInstruments"
          :key="ins"
          :label="ins"
          :value="ins"
        />
      </el-select>

      <el-checkbox-group v-model="enabledFactors" class="factor-toggles">
        <el-checkbox-button label="ma5">MA5</el-checkbox-button>
        <el-checkbox-button label="ma10">MA10</el-checkbox-button>
        <el-checkbox-button label="ma20">MA20</el-checkbox-button>
        <el-checkbox-button label="ema12">EMA12</el-checkbox-button>
        <el-checkbox-button label="ema26">EMA26</el-checkbox-button>
      </el-checkbox-group>

      <el-button-group class="period-selector">
        <el-button
          v-for="p in periods"
          :key="p.value"
          :type="period === p.value ? 'primary' : 'default'"
          size="small"
          @click="period = p.value"
        >
          {{ p.label }}
        </el-button>
      </el-button-group>
    </div>

    <!-- 主内容区域 -->
    <div class="main-content" :class="layoutMode">
      <!-- 仪表盘模式 -->
      <template v-if="layoutMode === 'dashboard'">
        <FactorDashboard ref="factorDashboard" />
      </template>

      <!-- 图表模式 -->
      <template v-else-if="layoutMode === 'chart'">
        <div class="chart-container">
          <div class="chart-toolbar">
            <span class="current-instrument">
              <i class="el-icon-s-data"></i>
              {{ selectedInstrument || '请选择合约' }}
            </span>
            <el-switch
              v-model="showFactorOverlay"
              active-text="因子叠加"
              inactive-text=""
            />
          </div>
          <div class="kline-wrapper">
            <KLineChart
              ref="klineChart"
              :symbol="selectedInstrument"
              :period="period"
              :kline-data="klineData"
              :factor-data="currentFactorData"
              :show-factor-overlay="showFactorOverlay"
              :enabled-factors="enabledFactors"
            />
          </div>
          <!-- 因子指标条 -->
          <FactorIndicator
            v-if="selectedInstrument"
            :instrument-id="selectedInstrument"
            class="factor-strip"
          />
        </div>
      </template>

      <!-- 分屏模式 -->
      <template v-else-if="layoutMode === 'split'">
        <div class="split-view">
          <div class="split-left">
            <div class="panel-header">
              <h3>K线图表</h3>
            </div>
            <div class="kline-wrapper">
              <KLineChart
                ref="klineChartSplit"
                :symbol="selectedInstrument"
                :period="period"
                :kline-data="klineData"
                :factor-data="currentFactorData"
                :show-factor-overlay="showFactorOverlay"
                :enabled-factors="enabledFactors"
              />
            </div>
          </div>
          <div class="split-right">
            <div class="panel-header">
              <h3>因子详情</h3>
            </div>
            <FactorIndicator
              v-if="selectedInstrument"
              :instrument-id="selectedInstrument"
              class="factor-detail"
            />
            <!-- 因子历史表 -->
            <div class="factor-history">
              <h4>因子历史记录</h4>
              <el-table
                :data="factorHistoryData"
                size="mini"
                stripe
                max-height="300"
              >
                <el-table-column prop="time" label="时间" width="100" />
                <el-table-column prop="ma5" label="MA5" width="80">
                  <template slot-scope="{ row }">
                    {{ formatValue(row.ma5) }}
                  </template>
                </el-table-column>
                <el-table-column prop="ma10" label="MA10" width="80">
                  <template slot-scope="{ row }">
                    {{ formatValue(row.ma10) }}
                  </template>
                </el-table-column>
                <el-table-column prop="rsi14" label="RSI" width="60">
                  <template slot-scope="{ row }">
                    <span :class="getRsiClass(row.rsi14)">
                      {{ formatValue(row.rsi14, 1) }}
                    </span>
                  </template>
                </el-table-column>
              </el-table>
            </div>
          </div>
        </div>
      </template>
    </div>

    <!-- 底部状态栏 -->
    <div class="status-bar">
      <div class="status-item">
        <span class="label">因子数据更新:</span>
        <span class="value">{{ lastUpdateTime }}</span>
      </div>
      <div class="status-item">
        <span class="label">活跃合约:</span>
        <span class="value">{{ availableInstruments.length }}</span>
      </div>
      <div class="status-item">
        <span class="label">因子类型:</span>
        <span class="value">{{ enabledFactors.length }} / 5</span>
      </div>
    </div>
  </div>
</template>

<script>
// @yutiansut @quantaxis - 因子分析系统页面
import { mapGetters } from 'vuex'
import FactorDashboard from '@/components/FactorDashboard.vue'
import FactorIndicator from '@/components/FactorIndicator.vue'
import KLineChart from '@/components/KLineChart.vue'

export default {
  name: 'FactorsPage',

  components: {
    FactorDashboard,
    FactorIndicator,
    KLineChart
  },

  data() {
    return {
      layoutMode: 'dashboard',  // dashboard | chart | split
      selectedInstrument: '',
      enabledFactors: ['ma5', 'ma10', 'ma20'],
      showFactorOverlay: true,
      period: 5,  // 5分钟K线
      periods: [
        { label: '1分', value: 4 },
        { label: '5分', value: 5 },
        { label: '15分', value: 6 },
        { label: '30分', value: 7 },
        { label: '60分', value: 8 },
        { label: '日线', value: 0 }
      ],
      klineData: [],
      factorHistoryData: [],
      lastUpdateTime: '--:--:--',
      maxHistoryRows: 50
    }
  },

  computed: {
    ...mapGetters('websocket', [
      'factors',
      'factorValues',
      'hasFactors',
      'isConnected'
    ]),

    availableInstruments() {
      return Object.keys(this.factors)
    },

    currentFactorData() {
      if (!this.selectedInstrument) return {}
      return this.factorValues(this.selectedInstrument) || {}
    },

    connectionStatus() {
      if (this.isConnected && this.hasFactors) {
        return {
          type: 'success',
          icon: 'el-icon-success',
          text: '数据流正常'
        }
      } else if (this.isConnected) {
        return {
          type: 'warning',
          icon: 'el-icon-warning',
          text: '等待因子数据'
        }
      } else {
        return {
          type: 'danger',
          icon: 'el-icon-error',
          text: '连接断开'
        }
      }
    }
  },

  watch: {
    factors: {
      handler(newFactors) {
        // 自动选择第一个合约
        if (!this.selectedInstrument && Object.keys(newFactors).length > 0) {
          this.selectedInstrument = Object.keys(newFactors)[0]
        }

        // 更新历史记录
        if (this.selectedInstrument) {
          this.updateFactorHistory()
        }

        this.lastUpdateTime = new Date().toLocaleTimeString()
      },
      deep: true,
      immediate: true
    }
  },

  mounted() {
    // 确保WebSocket连接
    this.ensureConnection()
  },

  methods: {
    ensureConnection() {
      if (!this.isConnected) {
        this.$store.dispatch('websocket/connect')
      }
    },

    handleLayoutChange(layout) {
      this.layoutMode = layout
    },

    onInstrumentSelect(instrument) {
      this.factorHistoryData = []
      // 可以在这里加载该合约的K线数据
      this.loadKlineData(instrument)
    },

    loadKlineData(instrument) {
      // TODO: 从后端加载K线数据
      // 这里先用模拟数据
      console.log('[FactorsPage] Loading kline data for:', instrument)

      // 模拟K线数据
      const now = Date.now()
      const interval = 5 * 60 * 1000  // 5分钟
      this.klineData = Array.from({ length: 100 }, (_, i) => ({
        datetime: now - (100 - i) * interval,
        open: 4000 + Math.random() * 200,
        high: 4100 + Math.random() * 200,
        low: 3900 + Math.random() * 200,
        close: 4000 + Math.random() * 200,
        volume: Math.floor(Math.random() * 10000),
        amount: Math.floor(Math.random() * 100000000)
      }))
    },

    updateFactorHistory() {
      const values = this.currentFactorData
      if (!values || Object.keys(values).length === 0) return

      const record = {
        time: new Date().toLocaleTimeString(),
        ...values
      }

      this.factorHistoryData.unshift(record)

      // 限制历史记录数量
      if (this.factorHistoryData.length > this.maxHistoryRows) {
        this.factorHistoryData.pop()
      }
    },

    formatValue(v, decimals = 2) {
      if (v === null || v === undefined) return '--'
      return v.toFixed(decimals)
    },

    getRsiClass(rsi) {
      if (rsi > 70) return 'overbought'
      if (rsi < 30) return 'oversold'
      return ''
    }
  }
}
</script>

<style lang="scss" scoped>
// @yutiansut @quantaxis - 因子分析页面样式
$base: #1e1e2e;
$surface0: #313244;
$surface1: #45475a;
$text: #cdd6f4;
$subtext: #a6adc8;
$blue: #89b4fa;
$green: #a6e3a1;
$red: #f38ba8;
$yellow: #f9e2af;

.factors-page {
  height: 100%;
  display: flex;
  flex-direction: column;
  background: $base;
  color: $text;
}

// 页面头部
.page-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 16px 24px;
  background: $surface0;
  border-bottom: 1px solid $surface1;

  .header-content {
    .page-title {
      margin: 0;
      font-size: 20px;
      font-weight: 600;
      color: $text;
      display: flex;
      align-items: center;
      gap: 8px;

      i {
        color: $blue;
      }
    }

    .page-subtitle {
      margin: 4px 0 0 28px;
      font-size: 12px;
      color: $subtext;
    }
  }

  .header-actions {
    display: flex;
    align-items: center;
    gap: 12px;
  }
}

// 快捷操作栏
.quick-actions {
  display: flex;
  align-items: center;
  gap: 16px;
  padding: 12px 24px;
  background: rgba($surface0, 0.5);
  border-bottom: 1px solid $surface1;

  .instrument-select {
    width: 180px;

    ::v-deep .el-input__inner {
      background: $surface0;
      border-color: $surface1;
      color: $text;
    }
  }

  .factor-toggles {
    ::v-deep .el-checkbox-button__inner {
      background: $surface0;
      border-color: $surface1;
      color: $subtext;
    }

    ::v-deep .el-checkbox-button.is-checked .el-checkbox-button__inner {
      background: $blue;
      border-color: $blue;
      color: $base;
    }
  }

  .period-selector {
    margin-left: auto;

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

// 主内容区域
.main-content {
  flex: 1;
  overflow: hidden;
  padding: 16px;

  &.dashboard {
    overflow: auto;
  }
}

// 图表容器
.chart-container {
  height: 100%;
  display: flex;
  flex-direction: column;
  background: $surface0;
  border-radius: 8px;
  overflow: hidden;

  .chart-toolbar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 12px 16px;
    border-bottom: 1px solid $surface1;

    .current-instrument {
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

  .kline-wrapper {
    flex: 1;
    min-height: 400px;
  }

  .factor-strip {
    border-top: 1px solid $surface1;
  }
}

// 分屏视图
.split-view {
  height: 100%;
  display: grid;
  grid-template-columns: 1fr 400px;
  gap: 16px;

  .split-left,
  .split-right {
    background: $surface0;
    border-radius: 8px;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .panel-header {
    padding: 12px 16px;
    border-bottom: 1px solid $surface1;

    h3 {
      margin: 0;
      font-size: 14px;
      font-weight: 600;
      color: $text;
    }
  }

  .split-left {
    .kline-wrapper {
      flex: 1;
      min-height: 0;
    }
  }

  .split-right {
    .factor-detail {
      padding: 16px;
    }

    .factor-history {
      flex: 1;
      padding: 16px;
      border-top: 1px solid $surface1;
      overflow: auto;

      h4 {
        margin: 0 0 12px 0;
        font-size: 13px;
        color: $subtext;
      }

      ::v-deep .el-table {
        background: transparent;

        th {
          background: $surface1 !important;
          color: $subtext !important;
          border-color: $surface1 !important;
        }

        td {
          background: $surface0 !important;
          color: $text !important;
          border-color: $surface1 !important;
        }

        .overbought { color: $red !important; }
        .oversold { color: $green !important; }
      }
    }
  }
}

// 状态栏
.status-bar {
  display: flex;
  gap: 24px;
  padding: 8px 24px;
  background: $surface0;
  border-top: 1px solid $surface1;
  font-size: 12px;

  .status-item {
    display: flex;
    align-items: center;
    gap: 6px;

    .label {
      color: $subtext;
    }

    .value {
      color: $text;
      font-weight: 500;
    }
  }
}

// 响应式
@media (max-width: 1200px) {
  .split-view {
    grid-template-columns: 1fr;
    grid-template-rows: 1fr 1fr;
  }
}

@media (max-width: 768px) {
  .quick-actions {
    flex-wrap: wrap;

    .period-selector {
      margin-left: 0;
      width: 100%;
    }
  }
}
</style>
