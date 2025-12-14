<template>
  <div class="chart-page">
    <el-card class="header-card">
      <el-row :gutter="20" align="middle">
        <el-col :span="8">
          <h2>K线图表</h2>
        </el-col>
        <el-col :span="16">
          <div class="controls">
            <el-select
              v-model="selectedInstrument"
              placeholder="选择合约"
              style="width: 200px"
              @change="onInstrumentChange"
            >
              <el-option
                v-for="inst in availableInstruments"
                :key="inst"
                :label="inst"
                :value="inst"
              />
            </el-select>

            <el-select
              v-model="klinePeriod"
              placeholder="时间周期"
              style="width: 120px; margin-left: 10px"
            >
              <el-option label="1分钟" :value="4" />
              <el-option label="5分钟" :value="5" />
              <el-option label="15分钟" :value="6" />
              <el-option label="30分钟" :value="7" />
              <el-option label="60分钟" :value="8" />
              <el-option label="日线" :value="0" />
            </el-select>

            <el-tag
              :type="isConnected ? 'success' : 'danger'"
              style="margin-left: 10px"
            >
              {{ isConnected ? 'WebSocket 已连接' : 'WebSocket 未连接' }}
            </el-tag>

            <el-button
              v-if="!isConnected"
              type="primary"
              size="small"
              icon="el-icon-connection"
              style="margin-left: 10px"
              @click="connect"
            >
              连接
            </el-button>

            <span class="info-text" style="margin-left: 15px">
              K线数量: {{ klineDataList.length }} 条
            </span>
          </div>
        </el-col>
      </el-row>
    </el-card>

    <el-card class="chart-card" :body-style="{ height: '100%', padding: '10px' }">
      <div class="chart-container" style="height: calc(100vh - 250px); min-height: 500px;">
        <KLineChart
          ref="klineChart"
          :symbol="selectedInstrument"
          :period="klinePeriod"
          :kline-data="klineDataList"
        />
      </div>
    </el-card>
  </div>
</template>

<script>
import { mapGetters, mapActions } from 'vuex'
import KLineChart from '@/components/KLineChart.vue'

export default {
  name: 'ChartPage',

  components: {
    KLineChart
  },

  data() {
    return {
      // ✨ 修改默认合约为有K线数据的合约 @yutiansut @quantaxis
      // 注意: 合约ID不带交易所前缀（后端注册的就是 IF2501 格式）
      selectedInstrument: 'IF2501',
      klinePeriod: 5,  // 默认5分钟
      klineDataList: [],

      // 可用合约列表（与后端 instrument_id 一致，不带交易所前缀）
      availableInstruments: [
        'IF2501',    // ✅ 默认有K线数据
        'IF2502',
        'IH2501',
        'IC2501'
      ]
    }
  },

  computed: {
    ...mapGetters('websocket', [
      'isConnected',
      'snapshot'
    ])
  },

  watch: {
    // 当选中合约变化时，订阅K线数据
    selectedInstrument(newVal) {
      if (newVal && this.isConnected) {
        this.subscribeKLine()
      }
    },

    // ✨ 监听整个 snapshot 变化，调试 klines 数据 @yutiansut @quantaxis
    snapshot: {
      handler(newSnapshot) {
        // 调试：打印完整快照的 klines 结构
        if (newSnapshot && newSnapshot.klines) {
          console.log('[ChartPage] 📊 snapshot.klines received:', JSON.stringify(newSnapshot.klines, null, 2))
        }
      },
      deep: true
    },

    // 监听K线数据更新（WebSocket实时推送）@yutiansut @quantaxis
    // ✨ 添加 immediate: true 确保数据到达时立即触发
    'snapshot.klines': {
      immediate: true,
      handler(newKlines) {
        console.log('[ChartPage] 📊 snapshot.klines watcher triggered:', {
          hasKlines: !!newKlines,
          instrument: this.selectedInstrument,
          period: this.klinePeriod
        })

        if (!newKlines || !this.selectedInstrument) {
          console.log('[ChartPage] ⚠️ No klines or no instrument selected')
          return
        }

        console.log('[ChartPage] 📊 Available instruments in klines:', Object.keys(newKlines))

        const instrumentKlines = newKlines[this.selectedInstrument]
        if (!instrumentKlines) {
          console.log('[ChartPage] ⚠️ No klines for instrument:', this.selectedInstrument)
          return
        }

        const durationNs = this.periodToNs(this.klinePeriod).toString()
        console.log('[ChartPage] 📊 Looking for duration:', durationNs, 'Available:', Object.keys(instrumentKlines))

        const periodKlines = instrumentKlines[durationNs]
        if (!periodKlines || !periodKlines.data) {
          console.log('[ChartPage] ⚠️ No period klines for duration:', durationNs)
          return
        }

        // 转换为数组格式
        console.log('[ChartPage] 📊 Raw period klines data:', periodKlines.data)

        const klineArray = Object.values(periodKlines.data).map(k => {
          const converted = {
            datetime: k.datetime / 1000000,  // 纳秒转毫秒
            open: k.open,
            high: k.high,
            low: k.low,
            close: k.close,
            volume: k.volume,
            amount: k.amount || (k.volume * k.close)
          }
          console.log('[ChartPage] 📊 Converted K-line:', {
            original_datetime: k.datetime,
            converted_datetime: converted.datetime,
            readable_time: new Date(converted.datetime).toLocaleString(),
            ohlc: [k.open, k.high, k.low, k.close]
          })
          return converted
        })

        // 按时间排序
        klineArray.sort((a, b) => a.datetime - b.datetime)

        this.klineDataList = klineArray
        console.log('[ChartPage] ✅ K-line data updated:', klineArray.length, 'bars')
        console.log('[ChartPage] 📊 Sample kline data:', klineArray[0])
      },
      deep: true
    },

    // 当K线周期变化时，重新订阅
    klinePeriod() {
      if (this.selectedInstrument && this.isConnected) {
        this.subscribeKLine()
      }
    }
  },

  mounted() {
    // 自动连接 WebSocket（如果未连接）
    if (!this.isConnected) {
      this.connect()
    } else {
      // 已连接，直接订阅
      this.subscribeKLine()
    }
  },

  methods: {
    ...mapActions('websocket', [
      'connectWebSocket',
      'subscribeQuote',
      'setChart'
    ]),

    async connect() {
      try {
        await this.connectWebSocket()
        this.$message.success('WebSocket 连接成功')

        // 连接成功后订阅行情和K线
        this.subscribeQuote(this.availableInstruments)
        this.subscribeKLine()
      } catch (error) {
        this.$message.error('WebSocket 连接失败: ' + error.message)
      }
    },

    // ✨ 订阅K线数据（增强调试）@yutiansut @quantaxis
    subscribeKLine() {
      console.log('[ChartPage] 📊 subscribeKLine called:', {
        instrument: this.selectedInstrument,
        isConnected: this.isConnected,
        period: this.klinePeriod
      })

      if (!this.selectedInstrument || !this.isConnected) {
        console.warn('[ChartPage] ⚠️ Cannot subscribe K-line: not connected or no instrument selected')
        return
      }

      const durationNs = this.periodToNs(this.klinePeriod)
      const chartConfig = {
        chart_id: 'chart_page',
        instrument_id: this.selectedInstrument,
        period: this.klinePeriod,
        count: 500
      }

      console.log('[ChartPage] 📊 Calling setChart with config:', chartConfig)
      console.log('[ChartPage] 📊 Duration in ns:', durationNs)

      this.setChart(chartConfig)

      console.log('[ChartPage] ✅ setChart called successfully')
    },

    // 转换周期为纳秒
    periodToNs(period) {
      switch (period) {
        case 0: return 86400000000000
        case 3: return 3000000000
        case 4: return 60000000000
        case 5: return 300000000000
        case 6: return 900000000000
        case 7: return 1800000000000
        case 8: return 3600000000000
        default: return 300000000000
      }
    },

    onInstrumentChange(value) {
      console.log('[ChartPage] Instrument changed to:', value)
    }
  }
}
</script>

<style scoped lang="scss">
.chart-page {
  padding: 20px;
  height: calc(100vh - 100px);
  display: flex;
  flex-direction: column;

  .header-card {
    margin-bottom: 20px;

    h2 {
      margin: 0;
      color: #303133;
    }

    .controls {
      display: flex;
      align-items: center;
      justify-content: flex-end;

      .info-text {
        font-size: 14px;
        color: #606266;
      }
    }
  }

  .chart-card {
    // ✨ 修复: 使用显式高度而非 flex (flex 在某些情况下会计算为 0) @yutiansut @quantaxis
    height: calc(100vh - 220px); // 页面高度 - padding - header
    display: flex;
    flex-direction: column;

    .chart-container {
      flex: 1;
      min-height: 500px;
    }
  }
}
</style>
