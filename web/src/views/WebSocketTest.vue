<template>
  <div class="websocket-test">
    <el-container>
      <!-- 顶部状态栏 -->
      <el-header height="auto" class="header">
        <el-card>
          <el-row :gutter="20">
            <el-col :span="4">
              <div class="status-item">
                <div class="label">连接状态</div>
                <el-tag :type="stateTagType" size="large">{{ connectionState }}</el-tag>
              </div>
            </el-col>
            <el-col :span="5">
              <div class="status-item">
                <div class="label">账户余额</div>
                <div class="value">¥ {{ formatNumber(accountBalance) }}</div>
              </div>
            </el-col>
            <el-col :span="5">
              <div class="status-item">
                <div class="label">可用资金</div>
                <div class="value">¥ {{ formatNumber(accountAvailable) }}</div>
              </div>
            </el-col>
            <el-col :span="5">
              <div class="status-item">
                <div class="label">浮动盈亏</div>
                <div class="value" :class="profitClass">
                  {{ formatProfit(accountFloatProfit) }}
                </div>
              </div>
            </el-col>
            <el-col :span="5">
              <div class="status-item">
                <div class="label">风险率</div>
                <el-progress
                  :percentage="riskRatio"
                  :color="riskColors"
                  :show-text="true"
                  :format="formatRisk"
                />
              </div>
            </el-col>
          </el-row>

          <el-row :gutter="20" style="margin-top: 15px">
            <el-col :span="24">
              <el-space>
                <el-button
                  v-if="!isConnected"
                  type="primary"
                  icon="el-icon-connection"
                  @click="connect"
                >
                  连接
                </el-button>
                <el-button
                  v-if="isConnected"
                  type="danger"
                  icon="el-icon-close"
                  @click="disconnect"
                >
                  断开
                </el-button>
                <el-button
                  icon="el-icon-refresh"
                  @click="reconnect"
                >
                  重连
                </el-button>
                <el-button
                  icon="el-icon-tickets"
                  @click="showSubscribeDialog = true"
                >
                  订阅行情
                </el-button>
                <el-button
                  icon="el-icon-document"
                  @click="showSnapshotDialog = true"
                >
                  查看快照
                </el-button>
                <el-button
                  icon="el-icon-refresh-left"
                  @click="clearSnapshot"
                >
                  清空快照
                </el-button>
              </el-space>
            </el-col>
          </el-row>
        </el-card>
      </el-header>

      <!-- 主内容区 -->
      <el-container class="main-container">
        <!-- 左侧：行情和下单 -->
        <el-aside width="50%">
          <!-- 行情面板 -->
          <el-card class="panel quote-panel">
            <template #header>
              <div class="panel-header">
                <span>实时行情</span>
                <el-select
                  v-model="selectedInstrument"
                  filterable
                  placeholder="选择合约"
                  size="small"
                  style="width: 200px"
                  @change="handleInstrumentChange"
                >
                  <el-option
                    v-for="instrument in allInstruments"
                    :key="instrument"
                    :label="instrument"
                    :value="instrument"
                  />
                </el-select>
              </div>
            </template>

            <div v-if="currentQuote" class="quote-detail">
              <el-row :gutter="20">
                <el-col :span="12">
                  <div class="quote-item">
                    <div class="label">合约</div>
                    <div class="value">{{ currentQuote.instrument_id }}</div>
                  </div>
                </el-col>
                <el-col :span="12">
                  <div class="quote-item">
                    <div class="label">时间</div>
                    <div class="value small">{{ formatDateTime(currentQuote.datetime) }}</div>
                  </div>
                </el-col>
              </el-row>

              <div class="price-section">
                <div class="price-main">
                  <span class="price-value" :class="getPriceChangeClass(currentQuote)">
                    {{ formatNumber(currentQuote.last_price) }}
                  </span>
                  <span class="price-change" :class="getPriceChangeClass(currentQuote)">
                    {{ formatPriceChange(currentQuote) }}
                  </span>
                  <span class="price-change-percent" :class="getPriceChangeClass(currentQuote)">
                    ({{ formatPriceChangePercent(currentQuote) }})
                  </span>
                </div>
              </div>

              <!-- 买卖五档 -->
              <div class="depth-panel">
                <el-row class="depth-header">
                  <el-col :span="8"><div class="header-label">卖盘</div></el-col>
                  <el-col :span="8"><div class="header-label">价格</div></el-col>
                  <el-col :span="8"><div class="header-label">买盘</div></el-col>
                </el-row>

                <!-- 卖五到卖一（倒序显示） -->
                <el-row v-for="i in 5" :key="'ask' + i" class="depth-row">
                  <el-col :span="8">
                    <div class="depth-volume ask-side">
                      {{ formatVolume(currentQuote['ask_volume' + (6 - i)]) }}
                    </div>
                  </el-col>
                  <el-col :span="8">
                    <div class="depth-price ask">
                      {{ formatNumber(currentQuote['ask_price' + (6 - i)]) }}
                    </div>
                  </el-col>
                  <el-col :span="8">
                    <div class="depth-volume bid-side">-</div>
                  </el-col>
                </el-row>

                <!-- 分隔线 -->
                <el-divider style="margin: 5px 0" />

                <!-- 买一到买五 -->
                <el-row v-for="i in 5" :key="'bid' + i" class="depth-row">
                  <el-col :span="8">
                    <div class="depth-volume ask-side">-</div>
                  </el-col>
                  <el-col :span="8">
                    <div class="depth-price bid">
                      {{ formatNumber(currentQuote['bid_price' + i]) }}
                    </div>
                  </el-col>
                  <el-col :span="8">
                    <div class="depth-volume bid-side">
                      {{ formatVolume(currentQuote['bid_volume' + i]) }}
                    </div>
                  </el-col>
                </el-row>
              </div>

              <el-row :gutter="20" class="price-grid" style="margin-top: 15px">
                <el-col :span="12">
                  <div class="quote-item">
                    <div class="label">涨停价</div>
                    <div class="value">{{ formatNumber(currentQuote.upper_limit) }}</div>
                  </div>
                </el-col>
                <el-col :span="12">
                  <div class="quote-item">
                    <div class="label">跌停价</div>
                    <div class="value">{{ formatNumber(currentQuote.lower_limit) }}</div>
                  </div>
                </el-col>
                <el-col :span="12">
                  <div class="quote-item">
                    <div class="label">成交量</div>
                    <div class="value">{{ currentQuote.volume }}</div>
                  </div>
                </el-col>
                <el-col :span="12">
                  <div class="quote-item">
                    <div class="label">持仓量</div>
                    <div class="value">{{ currentQuote.open_interest }}</div>
                  </div>
                </el-col>
              </el-row>
            </div>
            <el-empty v-else description="请订阅行情" />
          </el-card>

          <!-- K线图面板 -->
          <el-card class="panel kline-panel">
            <template #header>
              <div class="panel-header">
                <span>K线图</span>
                <el-space>
                  <el-select
                    v-model="klinePeriod"
                    size="small"
                    style="width: 100px"
                    @change="handlePeriodChange"
                  >
                    <el-option label="1分钟" :value="4" />
                    <el-option label="5分钟" :value="5" />
                    <el-option label="15分钟" :value="6" />
                    <el-option label="30分钟" :value="7" />
                    <el-option label="60分钟" :value="8" />
                    <el-option label="日线" :value="0" />
                  </el-select>
                </el-space>
              </div>
            </template>

            <div class="kline-container">
              <KLineChart
                ref="klineChart"
                :symbol="selectedInstrument"
                :period="klinePeriod"
                :kline-data="klineDataList"
              />
            </div>
          </el-card>

          <!-- 下单面板 -->
          <el-card class="panel order-form-panel">
            <template #header>
              <span>下单</span>
            </template>

            <el-form :model="orderForm" label-width="100px" size="small">
              <el-form-item label="合约">
                <el-input v-model="orderForm.instrument_id" placeholder="如: SHFE.cu2501" />
              </el-form-item>

              <el-form-item label="方向/开平">
                <el-radio-group v-model="orderForm.direction" size="small">
                  <el-radio-button label="BUY">买入</el-radio-button>
                  <el-radio-button label="SELL">卖出</el-radio-button>
                </el-radio-group>
                <el-radio-group v-model="orderForm.offset" size="small" style="margin-left: 10px">
                  <el-radio-button label="OPEN">开仓</el-radio-button>
                  <el-radio-button label="CLOSE">平仓</el-radio-button>
                </el-radio-group>
              </el-form-item>

              <el-form-item label="价格类型">
                <el-radio-group v-model="orderForm.price_type" size="small">
                  <el-radio-button label="LIMIT">限价</el-radio-button>
                  <el-radio-button label="MARKET">市价</el-radio-button>
                </el-radio-group>
              </el-form-item>

              <el-form-item v-if="orderForm.price_type === 'LIMIT'" label="委托价格">
                <el-input-number
                  v-model="orderForm.limit_price"
                  :step="10"
                  :min="0"
                  controls-position="right"
                  style="width: 100%"
                />
              </el-form-item>

              <el-form-item label="委托量">
                <el-input-number
                  v-model="orderForm.volume"
                  :step="1"
                  :min="1"
                  :max="100"
                  controls-position="right"
                  style="width: 100%"
                />
              </el-form-item>

              <el-form-item>
                <el-button
                  type="primary"
                  :disabled="!isConnected"
                  @click="submitOrder"
                  style="width: 100%"
                >
                  提交订单
                </el-button>
              </el-form-item>
            </el-form>
          </el-card>
        </el-aside>

        <!-- 右侧：持仓和订单 -->
        <el-main>
          <!-- 持仓列表 -->
          <el-card class="panel position-panel">
            <template #header>
              <div class="panel-header">
                <span>持仓 ({{ positionList.length }})</span>
              </div>
            </template>

            <el-table :data="positionList" stripe size="small" max-height="250">
              <el-table-column prop="instrument_id" label="合约" width="120" />
              <el-table-column label="多头" width="180">
                <template #default="{ row }">
                  <div v-if="row.volume_long > 0">
                    {{ row.volume_long }} @ {{ formatNumber(row.open_price_long) }}
                  </div>
                  <div v-else>-</div>
                </template>
              </el-table-column>
              <el-table-column label="空头" width="180">
                <template #default="{ row }">
                  <div v-if="row.volume_short > 0">
                    {{ row.volume_short }} @ {{ formatNumber(row.open_price_short) }}
                  </div>
                  <div v-else>-</div>
                </template>
              </el-table-column>
              <el-table-column label="浮动盈亏" width="120">
                <template #default="{ row }">
                  <span :class="row.float_profit >= 0 ? 'profit' : 'loss'">
                    {{ formatProfit(row.float_profit) }}
                  </span>
                </template>
              </el-table-column>
              <el-table-column label="操作" fixed="right">
                <template #default="{ row }">
                  <el-button
                    v-if="row.volume_long > 0"
                    type="danger"
                    size="mini"
                    @click="closePosition(row, 'LONG')"
                  >
                    平多
                  </el-button>
                  <el-button
                    v-if="row.volume_short > 0"
                    type="success"
                    size="mini"
                    @click="closePosition(row, 'SHORT')"
                  >
                    平空
                  </el-button>
                </template>
              </el-table-column>
            </el-table>
          </el-card>

          <!-- 订单列表 -->
          <el-card class="panel order-list-panel">
            <template #header>
              <div class="panel-header">
                <span>订单 ({{ activeOrdersList.length }})</span>
                <el-button
                  size="mini"
                  type="danger"
                  @click="cancelAllOrders"
                  :disabled="activeOrdersList.length === 0"
                >
                  全部撤单
                </el-button>
              </div>
            </template>

            <el-table :data="activeOrdersList" stripe size="small" max-height="250">
              <el-table-column prop="order_id" label="订单号" width="150" show-overflow-tooltip />
              <el-table-column prop="instrument_id" label="合约" width="120" />
              <el-table-column label="方向" width="100">
                <template #default="{ row }">
                  <el-tag :type="row.direction === 'BUY' ? 'danger' : 'success'" size="mini">
                    {{ row.direction === 'BUY' ? '买' : '卖' }}
                  </el-tag>
                  <el-tag type="info" size="mini" style="margin-left: 5px">
                    {{ row.offset === 'OPEN' ? '开' : '平' }}
                  </el-tag>
                </template>
              </el-table-column>
              <el-table-column label="价格" width="100">
                <template #default="{ row }">
                  {{ row.price_type === 'MARKET' ? '市价' : formatNumber(row.limit_price) }}
                </template>
              </el-table-column>
              <el-table-column label="数量" width="100">
                <template #default="{ row }">
                  {{ row.volume_left }} / {{ row.volume_orign }}
                </template>
              </el-table-column>
              <el-table-column label="状态" width="100">
                <template #default="{ row }">
                  <el-tag :type="getOrderStatusType(row.status)" size="mini">
                    {{ getOrderStatusText(row.status) }}
                  </el-tag>
                </template>
              </el-table-column>
              <el-table-column label="操作" width="80" fixed="right">
                <template #default="{ row }">
                  <el-button
                    v-if="canCancelOrder(row.status)"
                    type="danger"
                    size="mini"
                    @click="cancelOrder(row.order_id)"
                  >
                    撤单
                  </el-button>
                </template>
              </el-table-column>
            </el-table>
          </el-card>
        </el-main>
      </el-container>
    </el-container>

    <!-- 订阅对话框 -->
    <el-dialog v-model="showSubscribeDialog" title="订阅行情" width="500px">
      <el-form label-width="100px">
        <el-form-item label="合约代码">
          <el-select
            v-model="subscribeInstruments"
            multiple
            filterable
            allow-create
            placeholder="选择或输入合约代码"
            style="width: 100%"
          >
            <el-option label="SHFE.cu2501" value="SHFE.cu2501" />
            <el-option label="SHFE.ag2506" value="SHFE.ag2506" />
            <el-option label="DCE.i2505" value="DCE.i2505" />
            <el-option label="CZCE.RM505" value="CZCE.RM505" />
            <el-option label="CFFEX.IF2501" value="CFFEX.IF2501" />
          </el-select>
        </el-form-item>
      </el-form>

      <template #footer>
        <el-button @click="showSubscribeDialog = false">取消</el-button>
        <el-button type="primary" @click="subscribe">订阅</el-button>
      </template>
    </el-dialog>

    <!-- 快照对话框 -->
    <el-dialog v-model="showSnapshotDialog" title="业务快照" width="800px">
      <pre class="snapshot-json">{{ JSON.stringify(snapshot, null, 2) }}</pre>
    </el-dialog>
  </div>
</template>

<script>
import { mapGetters, mapActions } from 'vuex'
import KLineChart from '@/components/KLineChart.vue'

export default {
  name: 'WebSocketTest',

  components: {
    KLineChart
  },

  data() {
    return {
      // 选中的合约
      selectedInstrument: '',

      // K线周期
      klinePeriod: 5,  // 默认5分钟

      // K线数据列表
      klineDataList: [],

      // 下单表单
      orderForm: {
        instrument_id: 'SHFE.cu2501',
        direction: 'BUY',
        offset: 'OPEN',
        price_type: 'LIMIT',
        limit_price: 50000,
        volume: 1
      },

      // 订阅对话框
      showSubscribeDialog: false,
      subscribeInstruments: ['SHFE.cu2501', 'SHFE.ag2506'],

      // 快照对话框
      showSnapshotDialog: false,

      // 风险率颜色
      riskColors: [
        { color: '#67C23A', percentage: 50 },
        { color: '#E6A23C', percentage: 80 },
        { color: '#F56C6C', percentage: 100 }
      ]
    }
  },

  computed: {
    ...mapGetters('websocket', [
      'connectionState',
      'isConnected',
      'snapshot',
      'account',
      'positions',
      'quotes',
      'quote',
      'activeOrders',
      'subscribedInstruments'
    ]),

    stateTagType() {
      const typeMap = {
        CONNECTED: 'success',
        CONNECTING: 'warning',
        RECONNECTING: 'warning',
        DISCONNECTED: 'info',
        CLOSING: 'danger'
      }
      return typeMap[this.connectionState] || 'info'
    },

    currentQuote() {
      return this.quote(this.selectedInstrument)
    },

    positionList() {
      return Object.values(this.positions).filter(
        p => p.volume_long > 0 || p.volume_short > 0
      )
    },

    activeOrdersList() {
      return this.activeOrders
    },

    allInstruments() {
      // 合并订阅的合约和行情中的合约
      const instruments = new Set([
        ...this.subscribedInstruments,
        ...Object.keys(this.quotes)
      ])
      return Array.from(instruments)
    },

    riskRatio() {
      if (!this.account) return 0
      return Math.min(100, (this.account.risk_ratio || 0) * 100)
    },

    profitClass() {
      const profit = this.accountFloatProfit
      return profit >= 0 ? 'profit' : 'loss'
    },

    accountBalance() {
      return this.account && this.account.balance || 0
    },

    accountAvailable() {
      return this.account && this.account.available || 0
    },

    accountFloatProfit() {
      return this.account && this.account.float_profit || 0
    }
  },

  watch: {
    // 当订阅的合约变化时，自动选择第一个
    subscribedInstruments: {
      handler(newValue) {
        if (newValue.length > 0 && !this.selectedInstrument) {
          this.selectedInstrument = newValue[0]
        }
      },
      immediate: true
    },

    // 当选中合约变化时，订阅K线数据（通过 WebSocket）
    selectedInstrument(newVal) {
      if (newVal && this.isConnected) {
        this.subscribeKLine()
      }
    },

    // 监听K线数据更新（WebSocket实时推送）
    'snapshot.klines': {
      handler(newKlines) {
        if (!newKlines || !this.selectedInstrument) return

        // 从 snapshot.klines 中提取当前合约的K线数据
        const instrumentKlines = newKlines[this.selectedInstrument]
        if (!instrumentKlines) return

        // 转换周期为纳秒字符串
        const durationNs = this.periodToNs(this.klinePeriod).toString()
        const periodKlines = instrumentKlines[durationNs]
        if (!periodKlines || !periodKlines.data) return

        // 将 K线数据转换为数组格式（HQChart 需要）
        const klineArray = Object.values(periodKlines.data).map(k => ({
          datetime: k.datetime / 1000000,  // 纳秒转毫秒
          open: k.open,
          high: k.high,
          low: k.low,
          close: k.close,
          volume: k.volume,
          amount: k.amount || (k.volume * k.close)
        }))

        // 按时间排序
        klineArray.sort((a, b) => a.datetime - b.datetime)

        // 更新 K线数据列表
        this.klineDataList = klineArray
        console.log('[WebSocketTest] K-line data updated from WebSocket:', klineArray.length, 'bars')
      },
      deep: true
    },

    // 当K线周期变化时，重新订阅
    klinePeriod(newPeriod) {
      if (this.selectedInstrument && this.isConnected) {
        this.subscribeKLine()
      }
    }
  },

  methods: {
    ...mapActions('websocket', [
      'connectWebSocket',
      'disconnectWebSocket',
      'subscribeQuote',
      'insertOrder',
      'cancelOrder',
      'setChart'  // ✨ 新增：K线订阅
    ]),

    async connect() {
      try {
        await this.connectWebSocket()
        this.$message.success('WebSocket 连接成功')
      } catch (error) {
        this.$message.error('WebSocket 连接失败: ' + error.message)
      }
    },

    disconnect() {
      this.disconnectWebSocket()
      this.$message.info('WebSocket 已断开')
    },

    reconnect() {
      this.disconnect()
      setTimeout(() => {
        this.connect()
      }, 500)
    },

    subscribe() {
      if (this.subscribeInstruments.length === 0) {
        this.$message.warning('请选择合约')
        return
      }

      try {
        this.subscribeQuote(this.subscribeInstruments)
        this.$message.success('订阅成功: ' + this.subscribeInstruments.join(', '))
        this.showSubscribeDialog = false

        // 自动选择第一个订阅的合约
        if (this.subscribeInstruments.length > 0) {
          this.selectedInstrument = this.subscribeInstruments[0]
          this.orderForm.instrument_id = this.subscribeInstruments[0]
        }
      } catch (error) {
        this.$message.error('订阅失败: ' + error.message)
      }
    },

    handleInstrumentChange(instrumentId) {
      this.orderForm.instrument_id = instrumentId

      // 如果有行情，自动填充价格
      const quote = this.quote(instrumentId)
      if (quote) {
        this.orderForm.limit_price = quote.last_price
      }
    },

    submitOrder() {
      if (!this.isConnected) {
        this.$message.error('WebSocket 未连接')
        return
      }

      try {
        const orderId = this.insertOrder({
          instrument_id: this.orderForm.instrument_id,
          direction: this.orderForm.direction,
          offset: this.orderForm.offset,
          volume: this.orderForm.volume,
          price_type: this.orderForm.price_type,
          limit_price: this.orderForm.limit_price
        })

        this.$message.success('订单已提交: ' + orderId)
      } catch (error) {
        this.$message.error('下单失败: ' + error.message)
      }
    },

    cancelOrder(orderId) {
      this.$confirm('确认撤单?', '提示', {
        confirmButtonText: '确定',
        cancelButtonText: '取消',
        type: 'warning'
      }).then(() => {
        this.$store.dispatch('websocket/cancelOrder', orderId)
        this.$message.success('撤单指令已发送')
      }).catch(() => {})
    },

    cancelAllOrders() {
      const count = this.activeOrdersList.length
      if (count === 0) {
        this.$message.info('没有可撤销的订单')
        return
      }

      this.$confirm(`确认撤销全部 ${count} 个订单?`, '提示', {
        confirmButtonText: '确定',
        cancelButtonText: '取消',
        type: 'warning'
      }).then(() => {
        this.activeOrdersList.forEach(order => {
          this.$store.dispatch('websocket/cancelOrder', order.order_id)
        })
        this.$message.success('撤单指令已发送')
      }).catch(() => {})
    },

    closePosition(position, side) {
      const volume = side === 'LONG' ? position.volume_long : position.volume_short
      const direction = side === 'LONG' ? 'SELL' : 'BUY'

      this.$confirm(
        `确认平仓 ${position.instrument_id} ${side === 'LONG' ? '多头' : '空头'} ${volume} 手?`,
        '提示',
        {
          confirmButtonText: '确定',
          cancelButtonText: '取消',
          type: 'warning'
        }
      ).then(() => {
        this.insertOrder({
          instrument_id: position.instrument_id,
          direction: direction,
          offset: 'CLOSE',
          volume: volume,
          price_type: 'MARKET'
        })
        this.$message.success('平仓指令已发送')
      }).catch(() => {})
    },

    clearSnapshot() {
      // 这只是清空本地显示，不影响实际数据
      this.$confirm('确认清空快照显示?', '提示', {
        confirmButtonText: '确定',
        cancelButtonText: '取消',
        type: 'warning'
      }).then(() => {
        this.$message.info('已清空快照显示')
      }).catch(() => {})
    },

    canCancelOrder(status) {
      return status === 'ACCEPTED' || status === 'PENDING' || status === 'PARTIAL_FILLED'
    },

    getOrderStatusType(status) {
      const typeMap = {
        PENDING: 'info',
        ACCEPTED: 'warning',
        FILLED: 'success',
        CANCELLED: 'info',
        REJECTED: 'danger',
        PARTIAL_FILLED: 'warning'
      }
      return typeMap[status] || 'info'
    },

    getOrderStatusText(status) {
      const textMap = {
        PENDING: '待提交',
        ACCEPTED: '已接受',
        FILLED: '已成交',
        CANCELLED: '已撤单',
        REJECTED: '已拒绝',
        PARTIAL_FILLED: '部分成交'
      }
      return textMap[status] || status
    },

    formatNumber(value) {
      if (value === undefined || value === null) return '-'
      return Number(value).toLocaleString('zh-CN', {
        minimumFractionDigits: 2,
        maximumFractionDigits: 2
      })
    },

    formatVolume(value) {
      if (value === undefined || value === null || value === 0) return '-'
      return Number(value).toLocaleString('zh-CN')
    },

    formatProfit(value) {
      if (value === undefined || value === null) return '-'
      const formatted = this.formatNumber(Math.abs(value))
      return value >= 0 ? `+¥${formatted}` : `-¥${formatted}`
    },

    formatDateTime(value) {
      if (!value) return '-'
      return new Date(value).toLocaleString('zh-CN')
    },

    formatPriceChange(row) {
      if (!row.last_price || !row.pre_close) return '-'
      const change = row.last_price - row.pre_close
      return (change >= 0 ? '+' : '') + this.formatNumber(change)
    },

    formatPriceChangePercent(row) {
      if (!row.last_price || !row.pre_close) return '-'
      const change = row.last_price - row.pre_close
      const percent = (change / row.pre_close) * 100
      return (percent >= 0 ? '+' : '') + percent.toFixed(2) + '%'
    },

    getPriceChangeClass(row) {
      if (!row.last_price || !row.pre_close) return ''
      const change = row.last_price - row.pre_close
      return change > 0 ? 'profit' : change < 0 ? 'loss' : ''
    },

    formatRisk(percentage) {
      return percentage.toFixed(1) + '%'
    },

    // 订阅K线数据（WebSocket DIFF协议）
    subscribeKLine() {
      if (!this.selectedInstrument || !this.isConnected) {
        console.warn('[WebSocketTest] Cannot subscribe K-line: not connected or no instrument selected')
        return
      }

      console.log('[WebSocketTest] Subscribing K-line:', this.selectedInstrument, 'period:', this.klinePeriod)

      this.setChart({
        chart_id: 'main_chart',
        instrument_id: this.selectedInstrument,
        period: this.klinePeriod,
        count: 500
      })
    },

    // 转换周期为纳秒（DIFF协议要求）
    periodToNs(period) {
      switch (period) {
        case 0: return 86400000000000  // 日线
        case 3: return 3000000000      // 3秒
        case 4: return 60000000000     // 1分钟
        case 5: return 300000000000    // 5分钟
        case 6: return 900000000000    // 15分钟
        case 7: return 1800000000000   // 30分钟
        case 8: return 3600000000000   // 60分钟
        default: return 300000000000   // 默认5分钟
      }
    },

    handlePeriodChange(period) {
      console.log('[WebSocketTest] K-line period changed to:', period)
      // 周期变化时，通过 WebSocket 重新订阅（watch 会自动触发）
    },

    async fetchKLineData() {
      console.log('[WebSocketTest] Fetching K-line data for:', this.selectedInstrument, 'Period:', this.klinePeriod)

      try {
        // 调用后端 K线 API @yutiansut @quantaxis
        // 使用相对路径，通过 vue.config.js 代理访问
        const response = await this.$axios.get(
          `/api/market/kline/${this.selectedInstrument}`,
          {
            params: {
              period: this.klinePeriod,
              count: 500
            }
          }
        )

        if (response.data.code === 0 && response.data.data) {
          this.klineDataList = response.data.data.klines
          console.log('[WebSocketTest] Loaded', this.klineDataList.length, 'K-line bars')
        } else {
          console.warn('[WebSocketTest] Empty K-line data, using mock data')
          this.klineDataList = this.generateMockKLineData()
        }
      } catch (error) {
        console.error('[WebSocketTest] Failed to fetch K-line data:', error)
        // 失败时使用模拟数据
        this.klineDataList = this.generateMockKLineData()
        this.$message.warning('暂无K线数据，使用模拟数据')
      }
    },

    generateMockKLineData() {
      // 生成模拟K线数据用于测试
      const data = []
      const now = Date.now()
      const interval = this.klinePeriod === 0 ? 86400000 : 60000 * (this.klinePeriod === 4 ? 1 : this.klinePeriod === 5 ? 5 : 15)

      let basePrice = 3800
      for (let i = 100; i >= 0; i--) {
        const timestamp = now - i * interval
        const open = basePrice + (Math.random() - 0.5) * 20
        const close = basePrice + (Math.random() - 0.5) * 20
        const high = Math.max(open, close) + Math.random() * 10
        const low = Math.min(open, close) - Math.random() * 10
        const volume = Math.floor(Math.random() * 1000) + 100

        data.push({
          datetime: timestamp,
          open: parseFloat(open.toFixed(2)),
          high: parseFloat(high.toFixed(2)),
          low: parseFloat(low.toFixed(2)),
          close: parseFloat(close.toFixed(2)),
          volume: volume,
          amount: volume * ((open + close) / 2)
        })

        basePrice = close
      }

      return data
    }
  },

  mounted() {
    // 初始化时获取K线数据
    if (this.selectedInstrument) {
      this.fetchKLineData()
    }
  }
}
</script>

<style scoped lang="scss">
.websocket-test {
  width: 100%;
  height: 100vh;
  background-color: #f5f5f5;

  .el-container {
    height: 100%;
  }

  .header {
    padding: 20px;
    background-color: #fff;
  }

  .status-item {
    .label {
      font-size: 12px;
      color: #909399;
      margin-bottom: 8px;
    }

    .value {
      font-size: 20px;
      font-weight: bold;
      color: #303133;

      &.profit {
        color: #67C23A;
      }

      &.loss {
        color: #F56C6C;
      }
    }
  }

  .main-container {
    margin-top: 20px;
    padding: 0 20px 20px;
    overflow: hidden;
  }

  .el-aside {
    padding-right: 10px;
    overflow-y: auto;
  }

  .el-main {
    padding-left: 10px;
    overflow-y: auto;
  }

  .panel {
    margin-bottom: 20px;

    &:last-child {
      margin-bottom: 0;
    }
  }

  .kline-panel {
    .kline-container {
      height: 500px;
      background-color: #1a1a1a;
    }
  }

  .panel-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .quote-detail {
    .quote-item {
      margin-bottom: 15px;

      .label {
        font-size: 12px;
        color: #909399;
        margin-bottom: 5px;
      }

      .value {
        font-size: 16px;
        font-weight: bold;
        color: #303133;

        &.small {
          font-size: 12px;
          font-weight: normal;
        }

        &.bid {
          color: #F56C6C;
        }

        &.ask {
          color: #67C23A;
        }

        .volume {
          font-size: 12px;
          font-weight: normal;
          color: #909399;
          margin-left: 5px;
        }
      }
    }

    .price-section {
      margin: 20px 0;
      padding: 20px;
      background-color: #f5f7fa;
      border-radius: 4px;
      text-align: center;

      .price-main {
        .price-value {
          font-size: 36px;
          font-weight: bold;
          margin-right: 10px;
        }

        .price-change {
          font-size: 18px;
          font-weight: bold;
          margin-right: 5px;
        }

        .price-change-percent {
          font-size: 16px;
        }
      }
    }

    .price-grid {
      margin-top: 20px;
    }
  }

  .depth-panel {
    margin: 20px 0;
    background-color: #fafafa;
    border-radius: 4px;
    padding: 10px;

    .depth-header {
      margin-bottom: 8px;
      font-weight: bold;
      font-size: 12px;
      color: #606266;
      border-bottom: 1px solid #dcdfe6;
      padding-bottom: 5px;

      .header-label {
        text-align: center;
      }
    }

    .depth-row {
      margin: 3px 0;
      font-size: 13px;
      line-height: 24px;

      .depth-price {
        text-align: center;
        font-weight: 500;

        &.bid {
          color: #F56C6C;
        }

        &.ask {
          color: #67C23A;
        }
      }

      .depth-volume {
        text-align: center;
        font-size: 12px;
        color: #909399;

        &.bid-side {
          color: #F56C6C;
        }

        &.ask-side {
          color: #67C23A;
        }
      }
    }
  }

  .profit {
    color: #67C23A;
  }

  .loss {
    color: #F56C6C;
  }

  .snapshot-json {
    max-height: 500px;
    overflow-y: auto;
    background-color: #f5f5f5;
    padding: 10px;
    border-radius: 4px;
    font-size: 12px;
  }
}

:deep(.el-progress__text) {
  font-size: 12px !important;
}
</style>
