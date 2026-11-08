<template>
  <div class="chart-page">
    <el-card class="header-card">
      <el-row :gutter="20" align="middle">
        <el-col :xs="24" :lg="8">
          <h2>K线图表</h2>
        </el-col>
        <el-col :xs="24" :lg="16">
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
      <!-- ✨ 去掉行内 height:calc(100vh - 250px) 与 min-height:500px @yutiansut @quantaxis
           这两个值与 .chart-page(100vh-100px)、.chart-card(100vh-220px) 三条高度链
           互不相加, 实测 1366x600: .chart-card 只有 368px 且 overflow:hidden,
           而 .chart-container 被 min-height:500px 撑到 500px ->
           198px 被卡片裁掉且无滚动条, canvas 底边还落在视口外 99px。
           改为纯 flex 填充: 父级给多少就占多少, 不再有魔数。 -->
      <div class="chart-container">
        <KLineChart
          ref="klineChart"
          :symbol="selectedInstrument"
          :period="klinePeriod"
          :kline-data="klineDataList"
          :loaded="historyLoaded"
          :load-error="historyError"
        />
      </div>
    </el-card>
  </div>
</template>

<script>
import { mapGetters, mapActions } from 'vuex'
import KLineChart from '@/components/KLineChart.vue'
import { getInstruments, getKlineData } from '@/api'

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
      historyLoaded: false,   // HTTP 历史是否已回来
      historyError: '',       // 历史加载失败原因(供页面提示)

        // 可用合约列表 —— 启动时从 /api/market/instruments 动态填充。
        // ✨ 原先硬编码,与后端注册表脱节:新增/下市合约都要改代码,
        //    且与 VUE_APP_DEFAULT_INSTRUMENTS 是两份各自维护的清单。
        //    下面的值仅作为**接口失败时的兜底**。 @yutiansut @quantaxis
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
    // 当选中合约变化时，清除旧数据并订阅新K线数据
    selectedInstrument(newVal, oldVal) {
      if (newVal && newVal !== oldVal) {
        this.klineDataList = []
        this.loadHistory()                    // 先走 HTTP,不等 WS
        if (this.isConnected) this.subscribeKLine()
      }
    },

    // 监听K线数据更新
    'snapshot.klines': {
      immediate: true,
      handler(newKlines) {
        if (!newKlines || !this.selectedInstrument) return

        const instrumentKlines = newKlines[this.selectedInstrument]
        if (!instrumentKlines) return

        const durationNs = this.periodToNs(this.klinePeriod).toString()
        const periodKlines = instrumentKlines[durationNs]
        if (!periodKlines || !periodKlines.data) return

        // 转换K线数据格式 @yutiansut @quantaxis
        // DIFF协议使用纳秒时间戳（字符串格式避免JavaScript精度丢失）
        // 需要将纳秒字符串转换为毫秒数字用于Date对象
        const klineArray = Object.values(periodKlines.data).map(k => {
          // 处理纳秒时间戳：可能是字符串或数字
          let datetimeMs
          if (typeof k.datetime === 'string') {
            // 字符串格式：使用BigInt精确转换后除以1000000
            datetimeMs = Number(BigInt(k.datetime) / BigInt(1000000))
          } else {
            // 数字格式：直接除以1000000（可能有精度损失但仍可用）
            datetimeMs = Math.floor(k.datetime / 1000000)
          }
          return {
            datetime: datetimeMs,
            open: k.open,
            high: k.high,
            low: k.low,
            close: k.close,
            volume: k.volume,
            amount: k.amount || (k.volume * k.close)
          }
        })

        // ⚠️ 这里**不能**直接 `this.klineDataList = klineArray`。
        // HTTP 历史(loadHistory)已经放了最多 500 根,而 WS 推的往往只是
        // 最近几根 + 正在形成的那根 —— 整体替换会把历史抹掉,图表跳成几根。
        // 按 datetime 合并,WS 的同一时刻数据覆盖历史(它更新)。
        // @yutiansut @quantaxis
        const merged = new Map()
        for (const b of this.klineDataList) merged.set(b.datetime, b)
        for (const b of klineArray) merged.set(b.datetime, b)
        this.klineDataList = [...merged.values()].sort((a, b) => a.datetime - b.datetime)
      },
      deep: true
    },

    // 当K线周期变化时，清除旧数据并重新订阅
    klinePeriod(newVal, oldVal) {
      if (newVal !== oldVal) {
        this.klineDataList = []
        this.loadHistory()                    // 先走 HTTP,不等 WS
        if (this.selectedInstrument && this.isConnected) this.subscribeKLine()
      }
    }
  },

  mounted() {
    // 合约表从后端动态取,失败则沿用 data 里的兜底值 @yutiansut @quantaxis
    this.loadInstrumentList()

    // 先用 HTTP 把历史 K 线拉回来 —— 不依赖 WebSocket。
    // 改之前本页**只**监听 DIFF 的 snapshot.klines,WS 没推就永远停在
    // 「加载中」转圈。后端 /api/market/kline/{id} 一直是好的,
    // 前端 getKlineData() 也一直在,只是两头从没接上。@yutiansut @quantaxis
    this.loadHistory()

    // 自动连接 WebSocket（如果未连接）
    if (!this.isConnected) {
      this.connect()
    } else {
      // ✨ 已连接时也必须订阅**行情**,不能只订 K线 @yutiansut @quantaxis
      //
      // 原先这个分支只调 subscribeKLine()。而 WS 通常已被 App 级流程连上,
      // 用户进本页时 connect() 根本不会跑 —— 此时生效的是 store 启动时下的
      // 默认订阅 `VUE_APP_DEFAULT_INSTRUMENTS`
      //   = CFFEX.IF2501, SHFE.cu2501, SHFE.ag2506, CFFEX.IH2501
      // 与本页下拉 ['IF2501','IF2502','IH2501','IC2501'] 三重错配:
      //   · 带 CFFEX./SHFE. 前缀,而后端实际 ID 不带前缀
      //   · SHFE.cu2501 / SHFE.ag2506 在本部署根本不存在
      //   · IC2501 / IF2502 不在默认订阅里
      // 后果:选 IC2501 或 IF2502 时 K线图**只有 HTTP 历史、之后完全静止**
      // (实测 75 秒收到 0 根增量,同期 IF2501 收 45 根)。
      this.ensureQuoteSubscription()
      this.subscribeKLine()
    }
  },

  methods: {
    ...mapActions('websocket', [
      'connectWebSocket',
      'subscribeQuote',
      'setChart'
    ]),

    // 从 /api/market/instruments 动态取合约表 @yutiansut @quantaxis
    async loadInstrumentList() {
      try {
        const res = await getInstruments()
        const list = Array.isArray(res) ? res : (res && res.data) || []
        const ids = list.map(x => x.instrument_id).filter(Boolean)
        if (ids.length) {
          this.availableInstruments = ids
          if (!ids.includes(this.selectedInstrument)) this.selectedInstrument = ids[0]
        }
      } catch (e) {
        console.warn('[chart] 合约表加载失败,沿用兜底列表:', e && e.message)
      }
    },

    // ✨ 把本页合约**并入**既有订阅集 @yutiansut @quantaxis
    //
    // ⚠️ store 的 subscribeQuote 是**整体替换**不是累加
    //    (store/modules/websocket.js:313 直接 SET_SUBSCRIBED_INSTRUMENTS),
    //    必须传并集,否则会顶掉其他页面正在用的行情订阅。
    ensureQuoteSubscription() {
      const st = this.$store.state.websocket || {}
      const cur = st.subscribedInstruments || []
      // 后端 instrument_id 不带交易所前缀,统一去前缀后合并去重
      const strip = (x) => String(x).includes('.') ? String(x).split('.').pop() : String(x)
      const merged = Array.from(new Set([...cur.map(strip), ...this.availableInstruments]))
      this.subscribeQuote(merged)
    },

    async connect() {
      try {
        await this.connectWebSocket()
        this.$message.success('WebSocket 连接成功')

        // 连接成功后订阅行情和K线
        this.ensureQuoteSubscription()
        this.subscribeKLine()
      } catch (error) {
        this.$message.error('WebSocket 连接失败: ' + error.message)
      }
    },

    // 拉 HTTP 历史 K 线(与 WS 增量互补)
    // 后端参数:period 是**整数**(见 api/index.js 的说明),count 不是 limit。
    async loadHistory() {
      if (!this.selectedInstrument) return
      this.historyError = ''
      // ✨ 每次拉取都要复位 —— 否则切合约/切周期期间遮罩会显示
      //    「暂无K线数据」而不是「正在加载」。原先它只在 :272/:275 置 true,
      //    首次加载完成后**永不复位**。 @yutiansut @quantaxis
      this.historyLoaded = false
      const inst = this.selectedInstrument
      const per = this.klinePeriod
      try {
        const res = await getKlineData(inst, { period: per, count: 500 })
        // 本接口不走 {success,data,error} 信封,拦截器不拆包
        const bars = (res && res.data && res.data.klines) || []
        // 请求期间用户可能已切合约/切周期,结果作废
        if (inst !== this.selectedInstrument || per !== this.klinePeriod) return
        const merged = new Map()
        for (const b of bars) merged.set(b.datetime, b)
        for (const b of this.klineDataList) merged.set(b.datetime, b)  // WS 已到的更新
        this.klineDataList = [...merged.values()].sort((a, b) => a.datetime - b.datetime)
        this.historyLoaded = true
      } catch (e) {
        this.historyError = (e && e.message) || String(e)
        this.historyLoaded = true    // 别再转圈,让页面说清楚失败了
      }
    },

    // 订阅K线数据
    subscribeKLine() {
      if (!this.selectedInstrument || !this.isConnected) return

      this.setChart({
        chart_id: 'chart_page',
        instrument_id: this.selectedInstrument,
        period: this.klinePeriod,
        count: 500
      })
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
      // 合约切换由 watcher 处理
    }
  }
}
</script>

<style scoped lang="scss">
// ✨ 深色主题配色变量 @yutiansut @quantaxis
$dark-bg-primary: #0d1117;
$dark-bg-secondary: #161b22;
$dark-bg-tertiary: #21262d;
$dark-border: #30363d;
$dark-text-primary: #f0f6fc;
$dark-text-secondary: #8b949e;

.chart-page {
  // ✨ 高度不再自己猜 @yutiansut @quantaxis
  // 原来是 calc(100vh - 100px), 而真实的 chrome 是 96px(顶栏 56 + content-wrapper
  // 上下 padding 各 20), 有公告条时是 136px —— 100px 三种情况都不对。
  // .content-wrapper 经 01-layout-scroll.patch 后已是确定高度的 flex 子项,
  // 这里直接 height:100% 即可, 公告条出现/消失都自动跟随。
  padding: 0;   // ✨ content-wrapper 已有 20px padding, 这里再加就是双重内边距
                //    (与 orders/trades/positions 等页的 padding:0 保持一致)
  height: 100%;
  min-height: 320px;   // 极短视口下的兜底
  display: flex;
  flex-direction: column;

  .header-card {
    flex-shrink: 0;      // ✨ 工具条高度固定, 剩余空间全给图表
    margin-bottom: 20px;
    background-color: $dark-bg-secondary;
    border-color: $dark-border;

    h2 {
      margin: 0;
      color: $dark-text-primary;
    }

    .controls {
      display: flex;
      align-items: center;
      justify-content: flex-end;
      flex-wrap: wrap;     // ✨ 工具条里有 200px + 120px 两个定宽 select 加 tag/button,
      gap: 8px 0;          //    最小宽度约 600px。窄屏必须换行, 否则被挤出可视区。

      .info-text {
        font-size: 14px;
        color: $dark-text-secondary;
      }
    }
  }

  .chart-card {
    // ✨ 由固定 calc(100vh - 220px) 改回 flex 填充 @yutiansut @quantaxis
    // 当年注释说「flex 在某些情况下会计算为 0」, 真正原因是 flex 子项默认
    // min-height:auto + 祖先没有确定高度, 而不是 flex 本身不可用。
    // 现在 .chart-page 有确定高度(100%), 再补上 min-height:0, flex 就可靠了。
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    background-color: $dark-bg-secondary;
    border-color: $dark-border;

    // el-card 的 body 是 .chart-container 的直接父级, 必须一起进 flex 链,
    // 否则 body 高度塌成 auto, 子元素的 flex:1 无从计算。
    ::v-deep .el-card__body {
      flex: 1;
      min-height: 0;
      display: flex;
      flex-direction: column;
    }

    .chart-container {
      flex: 1;
      min-height: 0;       // ✨ 原来是 min-height:500px, 正是被卡片裁掉的元凶
      background-color: $dark-bg-primary;
    }
  }
}

// ✨ 响应式: 该页原来一个 @media 都没有 @yutiansut @quantaxis
@media (max-width: 1199px) {
  .chart-page .header-card .controls {
    justify-content: flex-start;
  }
}

@media (max-width: 767px) {
  .chart-page {
    min-height: 260px;

    .header-card {
      margin-bottom: 12px;
    }
  }
}
</style>
