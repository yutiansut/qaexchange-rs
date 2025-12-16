<template>
  <div class="factor-indicator" :class="{ dark: darkMode }">
    <!-- 合约选择器 -->
    <div class="indicator-header">
      <div class="instrument-info">
        <span class="instrument-name">{{ instrumentId }}</span>
        <span class="update-time" v-if="updateTime">
          <i class="el-icon-time"></i>
          {{ updateTime }}
        </span>
      </div>
      <div class="indicator-status" :class="statusClass">
        <span class="status-dot"></span>
        {{ statusText }}
      </div>
    </div>

    <!-- 因子卡片网格 -->
    <div class="factor-grid">
      <!-- MA 系列 -->
      <div class="factor-card ma">
        <div class="factor-header">
          <span class="factor-label">均线 MA</span>
          <span class="factor-badge trend">趋势</span>
        </div>
        <div class="factor-values">
          <div class="factor-item" v-for="ma in maFactors" :key="ma.id">
            <span class="factor-name" :style="{ color: ma.color }">{{ ma.label }}</span>
            <span class="factor-value">{{ formatValue(factorValues[ma.id]) }}</span>
          </div>
        </div>
        <div class="factor-trend" v-if="maTrend">
          <i :class="maTrend.icon"></i>
          <span :class="maTrend.class">{{ maTrend.text }}</span>
        </div>
      </div>

      <!-- EMA 系列 -->
      <div class="factor-card ema">
        <div class="factor-header">
          <span class="factor-label">指数均线 EMA</span>
          <span class="factor-badge momentum">动量</span>
        </div>
        <div class="factor-values">
          <div class="factor-item" v-for="ema in emaFactors" :key="ema.id">
            <span class="factor-name" :style="{ color: ema.color }">{{ ema.label }}</span>
            <span class="factor-value">{{ formatValue(factorValues[ema.id]) }}</span>
          </div>
        </div>
      </div>

      <!-- RSI 指标 -->
      <div class="factor-card rsi">
        <div class="factor-header">
          <span class="factor-label">相对强弱 RSI</span>
          <span class="factor-badge" :class="rsiZone">{{ rsiZoneText }}</span>
        </div>
        <div class="rsi-gauge">
          <div class="gauge-track">
            <div class="gauge-zone oversold"></div>
            <div class="gauge-zone neutral"></div>
            <div class="gauge-zone overbought"></div>
          </div>
          <div class="gauge-pointer" :style="{ left: rsiPosition + '%' }">
            <span class="gauge-value">{{ formatValue(factorValues.rsi14, 1) }}</span>
          </div>
          <div class="gauge-labels">
            <span>0</span>
            <span>30</span>
            <span>50</span>
            <span>70</span>
            <span>100</span>
          </div>
        </div>
      </div>

      <!-- MACD 指标 -->
      <div class="factor-card macd">
        <div class="factor-header">
          <span class="factor-label">MACD</span>
          <span class="factor-badge" :class="macdSignal">{{ macdSignalText }}</span>
        </div>
        <div class="macd-display">
          <div class="macd-item dif">
            <span class="macd-label">DIF</span>
            <span class="macd-value" :class="{ positive: factorValues.macd_dif > 0, negative: factorValues.macd_dif < 0 }">
              {{ formatValue(factorValues.macd_dif, 4) }}
            </span>
          </div>
          <div class="macd-item dea">
            <span class="macd-label">DEA</span>
            <span class="macd-value" :class="{ positive: factorValues.macd_dea > 0, negative: factorValues.macd_dea < 0 }">
              {{ formatValue(factorValues.macd_dea, 4) }}
            </span>
          </div>
          <div class="macd-item hist">
            <span class="macd-label">MACD柱</span>
            <span class="macd-value" :class="{ positive: factorValues.macd_hist > 0, negative: factorValues.macd_hist < 0 }">
              {{ formatValue(factorValues.macd_hist, 4) }}
            </span>
          </div>
        </div>
        <div class="macd-bar">
          <div class="bar-track"></div>
          <div class="bar-value" :class="{ positive: factorValues.macd_hist > 0 }" :style="{ width: macdBarWidth + '%' }"></div>
        </div>
      </div>
    </div>
  </div>
</template>

<script>
/**
 * 因子指标实时显示组件
 *
 * 显示 MA, EMA, RSI, MACD 等技术指标的实时值
 *
 * @yutiansut @quantaxis
 */
import { mapGetters } from 'vuex'

export default {
  name: 'FactorIndicator',

  props: {
    // 合约代码
    instrumentId: {
      type: String,
      required: true
    },
    // 深色模式
    darkMode: {
      type: Boolean,
      default: true
    }
  },

  data() {
    return {
      // MA 因子配置
      maFactors: [
        { id: 'ma5', label: 'MA5', color: '#f5a623' },
        { id: 'ma10', label: 'MA10', color: '#4a90d9' },
        { id: 'ma20', label: 'MA20', color: '#7ed321' }
      ],
      // EMA 因子配置
      emaFactors: [
        { id: 'ema12', label: 'EMA12', color: '#bd10e0' },
        { id: 'ema26', label: 'EMA26', color: '#50e3c2' }
      ]
    }
  },

  computed: {
    ...mapGetters('websocket', [
      'factorValues',
      'factorTimestamp',
      'isConnected'
    ]),

    // 获取当前合约的因子值
    factorValues() {
      return this.$store.getters['websocket/factorValues'](this.instrumentId)
    },

    // 更新时间显示
    updateTime() {
      const ts = this.$store.getters['websocket/factorTimestamp'](this.instrumentId)
      if (!ts) return null
      const date = new Date(ts)
      return date.toLocaleTimeString('zh-CN', { hour12: false })
    },

    // 连接状态
    statusClass() {
      if (!this.isConnected) return 'disconnected'
      if (Object.keys(this.factorValues).length === 0) return 'waiting'
      return 'live'
    },

    statusText() {
      if (!this.isConnected) return '未连接'
      if (Object.keys(this.factorValues).length === 0) return '等待数据'
      return '实时'
    },

    // MA 趋势判断
    maTrend() {
      const ma5 = this.factorValues.ma5
      const ma10 = this.factorValues.ma10
      const ma20 = this.factorValues.ma20

      if (!ma5 || !ma10 || !ma20) return null

      if (ma5 > ma10 && ma10 > ma20) {
        return { icon: 'el-icon-top', class: 'bullish', text: '多头排列' }
      } else if (ma5 < ma10 && ma10 < ma20) {
        return { icon: 'el-icon-bottom', class: 'bearish', text: '空头排列' }
      }
      return { icon: 'el-icon-minus', class: 'neutral', text: '震荡整理' }
    },

    // RSI 区域判断
    rsiZone() {
      const rsi = this.factorValues.rsi14
      if (!rsi) return 'neutral'
      if (rsi >= 70) return 'overbought'
      if (rsi <= 30) return 'oversold'
      return 'neutral'
    },

    rsiZoneText() {
      const rsi = this.factorValues.rsi14
      if (!rsi) return '等待'
      if (rsi >= 70) return '超买'
      if (rsi <= 30) return '超卖'
      return '中性'
    },

    // RSI 指针位置
    rsiPosition() {
      const rsi = this.factorValues.rsi14
      if (!rsi) return 50
      return Math.min(100, Math.max(0, rsi))
    },

    // MACD 信号
    macdSignal() {
      const dif = this.factorValues.macd_dif
      const dea = this.factorValues.macd_dea

      if (!dif || !dea) return 'neutral'
      if (dif > dea && dif > 0) return 'bullish'
      if (dif < dea && dif < 0) return 'bearish'
      return 'neutral'
    },

    macdSignalText() {
      const dif = this.factorValues.macd_dif
      const dea = this.factorValues.macd_dea

      if (!dif || !dea) return '等待'
      if (dif > dea) return '金叉'
      if (dif < dea) return '死叉'
      return '震荡'
    },

    // MACD 柱宽度
    macdBarWidth() {
      const hist = this.factorValues.macd_hist
      if (!hist) return 0
      // 标准化到 0-100
      return Math.min(100, Math.abs(hist) * 1000)
    }
  },

  methods: {
    formatValue(val, decimals = 2) {
      if (val === undefined || val === null) return '--'
      return Number(val).toFixed(decimals)
    }
  }
}
</script>

<style lang="scss" scoped>
// @yutiansut @quantaxis - 因子指标样式

// 颜色变量
$primary: #1890ff;
$success: #52c41a;
$warning: #faad14;
$danger: #f5222d;
$bg-dark: #1a1a2e;
$bg-card: #16213e;
$text-primary: #e4e6eb;
$text-secondary: #8b949e;
$border-color: #30363d;

.factor-indicator {
  background: $bg-dark;
  border-radius: 12px;
  padding: 20px;
  color: $text-primary;

  &:not(.dark) {
    background: #fff;
    color: #303133;
  }
}

.indicator-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 20px;
  padding-bottom: 16px;
  border-bottom: 1px solid $border-color;

  .instrument-info {
    display: flex;
    align-items: center;
    gap: 12px;

    .instrument-name {
      font-size: 18px;
      font-weight: 600;
      color: $text-primary;
    }

    .update-time {
      font-size: 12px;
      color: $text-secondary;
      display: flex;
      align-items: center;
      gap: 4px;

      i {
        font-size: 12px;
      }
    }
  }

  .indicator-status {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    padding: 4px 10px;
    border-radius: 12px;
    background: rgba($success, 0.1);
    color: $success;

    .status-dot {
      width: 6px;
      height: 6px;
      border-radius: 50%;
      background: $success;
      animation: pulse 2s infinite;
    }

    &.disconnected {
      background: rgba($danger, 0.1);
      color: $danger;

      .status-dot {
        background: $danger;
        animation: none;
      }
    }

    &.waiting {
      background: rgba($warning, 0.1);
      color: $warning;

      .status-dot {
        background: $warning;
      }
    }
  }
}

.factor-grid {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: 16px;

  @media (max-width: 768px) {
    grid-template-columns: 1fr;
  }
}

.factor-card {
  background: $bg-card;
  border-radius: 10px;
  padding: 16px;
  border: 1px solid $border-color;
  transition: all 0.3s ease;

  &:hover {
    border-color: $primary;
    transform: translateY(-2px);
  }

  .factor-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 12px;

    .factor-label {
      font-size: 14px;
      font-weight: 500;
      color: $text-primary;
    }

    .factor-badge {
      font-size: 11px;
      padding: 2px 8px;
      border-radius: 10px;
      background: rgba($primary, 0.1);
      color: $primary;

      &.trend { background: rgba(#f5a623, 0.1); color: #f5a623; }
      &.momentum { background: rgba(#bd10e0, 0.1); color: #bd10e0; }
      &.overbought { background: rgba($danger, 0.1); color: $danger; }
      &.oversold { background: rgba($success, 0.1); color: $success; }
      &.neutral { background: rgba($text-secondary, 0.1); color: $text-secondary; }
      &.bullish { background: rgba($success, 0.1); color: $success; }
      &.bearish { background: rgba($danger, 0.1); color: $danger; }
    }
  }
}

.factor-values {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.factor-item {
  display: flex;
  justify-content: space-between;
  align-items: center;

  .factor-name {
    font-size: 12px;
    font-weight: 500;
  }

  .factor-value {
    font-size: 14px;
    font-weight: 600;
    font-family: 'Monaco', 'Consolas', monospace;
  }
}

.factor-trend {
  margin-top: 12px;
  padding-top: 12px;
  border-top: 1px solid $border-color;
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;

  &.bullish { color: $success; }
  &.bearish { color: $danger; }
  &.neutral { color: $text-secondary; }

  i {
    font-size: 14px;
  }
}

// RSI 仪表盘
.rsi-gauge {
  position: relative;
  padding: 20px 0;

  .gauge-track {
    height: 8px;
    border-radius: 4px;
    display: flex;
    overflow: hidden;

    .gauge-zone {
      flex: 1;

      &.oversold { background: linear-gradient(90deg, $success, $success); }
      &.neutral { background: linear-gradient(90deg, #3d4450, #3d4450); }
      &.overbought { background: linear-gradient(90deg, $danger, $danger); }
    }
  }

  .gauge-pointer {
    position: absolute;
    top: 0;
    transform: translateX(-50%);
    display: flex;
    flex-direction: column;
    align-items: center;
    transition: left 0.5s ease;

    &::after {
      content: '';
      width: 0;
      height: 0;
      border-left: 6px solid transparent;
      border-right: 6px solid transparent;
      border-top: 8px solid $primary;
    }

    .gauge-value {
      font-size: 16px;
      font-weight: 700;
      color: $primary;
      margin-bottom: 4px;
    }
  }

  .gauge-labels {
    display: flex;
    justify-content: space-between;
    margin-top: 8px;
    font-size: 10px;
    color: $text-secondary;
  }
}

// MACD 显示
.macd-display {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 12px;
  margin-bottom: 12px;
}

.macd-item {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 4px;

  .macd-label {
    font-size: 11px;
    color: $text-secondary;
  }

  .macd-value {
    font-size: 13px;
    font-weight: 600;
    font-family: 'Monaco', 'Consolas', monospace;

    &.positive { color: $success; }
    &.negative { color: $danger; }
  }
}

.macd-bar {
  height: 6px;
  background: $border-color;
  border-radius: 3px;
  overflow: hidden;

  .bar-value {
    height: 100%;
    background: $danger;
    border-radius: 3px;
    transition: width 0.5s ease;

    &.positive {
      background: $success;
    }
  }
}

@keyframes pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.5; }
}
</style>
</script>
