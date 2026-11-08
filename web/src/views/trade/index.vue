<template>
  <div class="trade-page">
    <!-- ① 顶部条:合约选择 + 实时行情 + 连接状态 @yutiansut @quantaxis
         原来是两张独立 el-card(instrument-selector / market-info),
         各自带边框和 padding,把「选什么」和「现在多少钱」割成两块。
         合并成一条横向 header,跨整宽,一眼看完。 -->
    <div class="trade-topbar">
      <el-select
        v-model="selectedInstrument"
        @change="handleInstrumentChange"
        placeholder="选择合约"
        class="tb-inst"
        size="small"
      >
        <el-option
          v-for="inst in instruments"
          :key="inst.instrument_id"
          :label="`${inst.instrument_id} - ${inst.name}`"
          :value="inst.instrument_id"
        />
      </el-select>

      <div class="tb-price" :class="priceChangeClass">
        <span class="tb-last">{{ tickData.last_price ? tickData.last_price.toFixed(2) : '--' }}</span>
        <span class="tb-chg" v-if="priceChange !== 0">
          {{ priceChange > 0 ? '+' : '' }}{{ priceChange.toFixed(2) }}
          ({{ priceChangePercent > 0 ? '+' : '' }}{{ priceChangePercent.toFixed(2) }}%)
        </span>
      </div>

      <div class="tb-quotes">
        <div class="tb-q"><span>买一</span><b class="bid">{{ bestBidPrice ? bestBidPrice.toFixed(2) : '--' }}</b></div>
        <div class="tb-q"><span>卖一</span><b class="ask">{{ bestAskPrice ? bestAskPrice.toFixed(2) : '--' }}</b></div>
        <!-- 与左边买一/卖一同口径:未加载显示 `--`。@yutiansut @quantaxis
             原写法 `|| 0` 会把「行情还没到」渲染成「成交量 0」——
             在报价区这会被读成「今日无成交」,而旁边两个值已经是 `--`,
             同一行三个数字口径不一致。
             ⚠️ 0 是**合法真值**(确实没成交),所以判空用 `== null` 而不是 falsy ——
             `0 || x` 会把真的 0 也换掉。 -->
        <div class="tb-q"><span>成交量</span><b>{{ tickData.volume == null ? '--' : tickData.volume }}</b></div>
      </div>

      <el-tooltip :content="wsEnabled ? '点击切换到 HTTP 轮询' : '点击切换到 WebSocket'" placement="top">
        <el-tag
          :type="wsConnectionStatus.type"
          size="small"
          class="tb-ws"
          @click="toggleWebSocket"
        >
          <i :class="wsEnabled ? 'el-icon-connection' : 'el-icon-refresh'"></i>
          {{ wsConnectionStatus.text }}
        </el-tag>
      </el-tooltip>
    </div>

    <el-row :gutter="16">
      <el-col :xs="24" :lg="8">
        <!-- 订单簿 -->
        <el-card class="orderbook" shadow="never">
          <div slot="header" class="card-header">
            <span>订单簿</span>
            <el-button-group size="mini">
              <el-button :type="depth === 5 ? 'primary' : ''" @click="setDepth(5)">5档</el-button>
              <el-button :type="depth === 10 ? 'primary' : ''" @click="setDepth(10)">10档</el-button>
            </el-button-group>
          </div>

          <div class="orderbook-content">
            <!-- 卖盘（价格越高越上，远离最新价）-->
            <div class="asks">
              <div class="header-row">
                <span class="price">价格(卖)</span>
                <span class="volume">数量</span>
              </div>
              <div
                v-for="(ask, index) in reversedAsks"
                :key="'ask-' + index"
                class="order-row ask-row"
                @click="handlePriceClick(ask.price, 'SELL')"
              >
                <span class="price">{{ ask.price.toFixed(2) }}</span>
                <span class="volume">{{ ask.volume }}</span>
                <div
                  class="volume-bar ask-bar"
                  :style="{ width: (ask.volume / maxVolume * 100) + '%' }"
                />
              </div>
            </div>

            <!-- 最新价分隔线 -->
            <div class="last-price-separator">
              <span class="price" :class="priceChangeClass">
                {{ tickData.last_price ? tickData.last_price.toFixed(2) : '--' }}
              </span>
            </div>

            <!-- 买盘 -->
            <div class="bids">
              <div
                v-for="(bid, index) in orderbook.bids"
                :key="'bid-' + index"
                class="order-row bid-row"
                @click="handlePriceClick(bid.price, 'BUY')"
              >
                <span class="price">{{ bid.price.toFixed(2) }}</span>
                <span class="volume">{{ bid.volume }}</span>
                <div
                  class="volume-bar bid-bar"
                  :style="{ width: (bid.volume / maxVolume * 100) + '%' }"
                />
              </div>
              <div class="header-row">
                <span class="price">价格(买)</span>
                <span class="volume">数量</span>
              </div>
            </div>
          </div>
        </el-card>

        <!-- 最新成交 -->
        <el-card class="recent-trades" shadow="never">
          <div slot="header">最新成交</div>
          <div class="trades-list">
            <div class="header-row">
              <span class="time">时间</span>
              <span class="price">价格</span>
              <span class="volume">数量</span>
            </div>
            <div
              v-for="(trade, index) in recentTrades"
              :key="index"
              class="trade-row"
              :class="trade.direction === 'BUY' ? 'buy-trade' : 'sell-trade'"
            >
              <span class="time">{{ trade.time }}</span>
              <span class="price">{{ trade.price.toFixed(2) }}</span>
              <span class="volume">{{ trade.volume }}</span>
            </div>
          </div>
        </el-card>
      </el-col>

      <!-- 右侧：下单面板 -->
      <el-col :xs="24" :lg="16">
        <!-- ② 下单区:方向用按钮组切换,不用 tabs @yutiansut @quantaxis
             原来三个 el-tab-pane 各挂一份 order-form,切 tab 会整块重绘,
             且「买/卖/平」被拆成三个页面 —— 交易时最需要的恰是快速反手。
             改为顶部一排方向按钮,下方复用同一个表单实例。 -->
        <el-card class="order-panel" shadow="never">
          <div class="op-switch">
            <button
              type="button"
              :class="['op-btn', 'op-buy', activeTab === 'buy' ? 'is-on' : '']"
              @click="activeTab = 'buy'"
            >买入开仓</button>
            <button
              type="button"
              :class="['op-btn', 'op-sell', activeTab === 'sell' ? 'is-on' : '']"
              @click="activeTab = 'sell'"
            >卖出开仓</button>
            <button
              type="button"
              :class="['op-btn', 'op-close', activeTab === 'close' ? 'is-on' : '']"
              @click="activeTab = 'close'"
            >平仓</button>
          </div>

          <order-form
            v-if="activeTab !== 'close'"
            :key="activeTab"
            :instrument-id="selectedInstrument"
            :current-price="tickData.last_price || 0"
            :direction="activeTab === 'buy' ? 'BUY' : 'SELL'"
            offset="OPEN"
            @submit="handleOrderSubmit"
            @account-change="handleAccountChange"
          />
          <close-form
            v-else
            ref="closeForm"
            :instrument-id="selectedInstrument"
            :current-price="tickData.last_price || 0"
            @submit="handleOrderSubmit"
            @account-change="handleAccountChange"
          />
        </el-card>

        <!-- 当前委托 -->
        <el-card class="pending-orders" shadow="never">
          <div slot="header" class="card-header">
            <span>当前委托</span>
            <el-button size="mini" @click="loadPendingOrders">刷新</el-button>
          </div>
          <el-table
            :empty-text="$emptyText(loadError, '暂无挂单')"
            :data="pendingOrders"
            border
            stripe
            height="300"
            size="mini"
          >
            <el-table-column prop="order_id" label="订单号" min-width="150" show-overflow-tooltip/>
            <el-table-column prop="instrument_id" label="合约" min-width="100" show-overflow-tooltip/>
            <el-table-column prop="direction" label="方向" width="60" align="center">
              <template slot-scope="scope">
                <span :class="scope.row.direction === 'BUY' ? 'buy-text' : 'sell-text'">
                  {{ scope.row.direction === 'BUY' ? '买' : '卖' }}
                </span>
              </template>
            </el-table-column>
            <el-table-column prop="price" label="价格" width="80" align="right" show-overflow-tooltip>
              <template slot-scope="scope">
                {{ $num(scope.row.price, 2) }}
              </template>
            </el-table-column>
            <el-table-column prop="volume" label="数量" width="60" align="right" show-overflow-tooltip/>
            <el-table-column prop="filled_volume" label="成交" width="60" align="right" show-overflow-tooltip/>
            <el-table-column prop="status" label="状态" width="80" align="center">
              <template slot-scope="scope">
                <el-tag size="mini" :type="getStatusType(scope.row.status)">
                  {{ getStatusText(scope.row.status) }}
                </el-tag>
              </template>
            </el-table-column>
            <el-table-column label="操作" width="80" fixed="right">
              <template slot-scope="scope">
                <el-button
                  v-if="scope.row.status === 'Submitted' || scope.row.status === 'PartiallyFilled'"
                  type="text"
                  size="mini"
                  @click="handleCancelOrder(scope.row)"
                >
                  撤单
                </el-button>
              </template>
            </el-table-column>
          </el-table>
        </el-card>
      </el-col>
    </el-row>
  </div>
</template>

<script>
import { getInstruments, getOrderBook, getTick, getRecentTrades, submitOrder, cancelOrder, queryUserOrders } from '@/api'
import { extractErrorMessage } from '@/api/request'
import { mapGetters, mapActions } from 'vuex'
import OrderForm from './components/OrderForm.vue'
import CloseForm from './components/CloseForm.vue'
// ✨ P1-3: WebSocket 实时数据集成 @yutiansut @quantaxis
import WebSocketManager, { ConnectionState } from '@/websocket'

export default {
  name: 'Trade',
  components: {
    OrderForm,
    CloseForm
  },
  computed: {
    ...mapGetters(['currentUser']),
    maxVolume() {
      const allVolumes = [...this.orderbook.bids, ...this.orderbook.asks].map(o => o.volume)
      return Math.max(...allVolumes, 1)
    },
    priceChangeClass() {
      if (this.priceChange > 0) return 'price-up'
      if (this.priceChange < 0) return 'price-down'
      return ''
    },
    // 卖盘反序：价格高的在上面（远离最新价）
    reversedAsks() {
      return [...this.orderbook.asks].reverse()
    },
    // 从订单簿第一档获取买一价
    bestBidPrice() {
      return this.orderbook.bids && this.orderbook.bids.length > 0
        ? this.orderbook.bids[0].price
        : (this.tickData.bid_price || null)
    },
    // 从订单簿第一档获取卖一价
    bestAskPrice() {
      return this.orderbook.asks && this.orderbook.asks.length > 0
        ? this.orderbook.asks[0].price
        : (this.tickData.ask_price || null)
    },
    // ✨ WebSocket 连接状态显示
    wsConnectionStatus() {
      const statusMap = {
        [ConnectionState.CONNECTED]: { text: '已连接', type: 'success' },
        [ConnectionState.CONNECTING]: { text: '连接中', type: 'warning' },
        [ConnectionState.RECONNECTING]: { text: '重连中', type: 'warning' },
        [ConnectionState.DISCONNECTED]: { text: '未连接', type: 'danger' },
        [ConnectionState.CLOSING]: { text: '关闭中', type: 'info' }
      }
      return statusMap[this.wsState] || { text: '未知', type: 'info' }
    }
  },
  data() {
    return {
      // 挂单表的加载失败原因 —— 只由 loadPendingOrders 维护,
      // 不让下单/撤单的失败污染这张表 @yutiansut @quantaxis
      loadError: null,
      instruments: [],
      selectedInstrument: 'IF2501',
      selectedAccountId: null,  // 当前选中的账户ID（用于查询委托）
      activeTab: 'buy',
      depth: 5,
      orderbook: {
        bids: [],
        asks: []
      },
      // 初值用 null 而非 0 —— 首屏在行情到达前应显示 `--`,不是一个假的 0。
      // 与 updateTickFromSnapshot 里改用 `??` 是同一件事的两头。@yutiansut @quantaxis
      tickData: {
        last_price: null,
        bid_price: null,
        ask_price: null,
        volume: null
      },
      recentTrades: [],
      pendingOrders: [],
      prevPrice: 0,
      priceChange: 0,
      priceChangePercent: 0,
      refreshTimer: null,
      // ✨ P1-3: WebSocket 相关状态 @yutiansut @quantaxis
      ws: null,
      wsState: ConnectionState.DISCONNECTED,
      wsEnabled: true,  // 是否启用 WebSocket（可在 UI 中切换）
      snapshotUnsubscribe: null,  // 快照监听取消函数
      ordersRefreshTimer: null  // 委托刷新定时器（委托仍用 HTTP 轮询）
    }
  },
  mounted() {
    this.loadInstruments()
    // ✨ P1-3: 优先使用 WebSocket，回退到 HTTP 轮询
    if (this.wsEnabled) {
      this.initWebSocket()
    } else {
      this.startAutoRefresh()
    }
  },
  beforeDestroy() {
    this.stopAutoRefresh()
    this.destroyWebSocket()
  },
  methods: {
    // ✨ 映射 Vuex websocket 模块的 actions @yutiansut @quantaxis
    ...mapActions('websocket', ['fetchUserAccounts']),

    async loadInstruments() {
      try {
        this.instruments = await getInstruments()
        if (this.instruments.length > 0 && !this.selectedInstrument) {
          this.selectedInstrument = this.instruments[0].instrument_id
        }
      } catch (error) {
        this.$message.error('加载合约列表失败')
      }
    },

    async loadOrderBook() {
      if (!this.selectedInstrument) return

      try {
        const data = await getOrderBook(this.selectedInstrument, this.depth)
        // @yutiansut @quantaxis
        // 合并同价格的订单（将相同价格的volume累加）
        this.orderbook = {
          bids: this.mergePriceLevels(data.bids || [], 'desc'),  // 买盘：价格从高到低
          asks: this.mergePriceLevels(data.asks || [], 'asc')    // 卖盘：价格从低到高
        }
      } catch (error) {
        console.error('Failed to load orderbook:', error)
      }
    },

    // 合并同价格档位的订单
    // @yutiansut @quantaxis
    mergePriceLevels(orders, sortOrder = 'desc') {
      if (!orders || orders.length === 0) return []

      // 使用 Map 合并同价格订单
      const priceMap = new Map()
      for (const order of orders) {
        const price = order.price
        if (priceMap.has(price)) {
          priceMap.get(price).volume += order.volume
        } else {
          priceMap.set(price, { price: price, volume: order.volume })
        }
      }

      // 转为数组并排序
      const merged = Array.from(priceMap.values())
      merged.sort((a, b) => sortOrder === 'desc' ? b.price - a.price : a.price - b.price)

      // 只返回前 depth 档
      return merged.slice(0, this.depth)
    },

    async loadTick() {
      if (!this.selectedInstrument) return

      try {
        const data = await getTick(this.selectedInstrument)

        if (this.tickData.last_price && data.last_price) {
          this.prevPrice = this.tickData.last_price
          this.priceChange = data.last_price - this.prevPrice
          this.priceChangePercent = (this.priceChange / this.prevPrice) * 100
        }

        this.tickData = data
      } catch (error) {
        console.error('Failed to load tick:', error)
      }
    },

    handleAccountChange(accountId) {
      // 当OrderForm中的账户选择变化时更新
      const changed = this.selectedAccountId !== accountId
      this.selectedAccountId = accountId
      // 立即刷新该账户的委托
      this.loadPendingOrders()
      // ✨ 账户变了必须重连 WS —— 后端按 account_id 分发 orders/trades/accounts,
      //    不重连就还在收上一个账户(或压根没连)的流。@yutiansut @quantaxis
      if (changed && this.wsEnabled) {
        this.destroyWebSocket()
        this.initWebSocket()
      }
    },

    async loadPendingOrders() {
      this.loadError = null   // 每轮自动刷新先清掉上次的失败标记
      // Phase 10: 使用 account_id 而非 user_id 查询委托
      if (!this.selectedAccountId) return

      try {
        const data = await queryUserOrders(this.selectedAccountId)
        // 过滤活跃订单：排除已完成(Filled)、已撤销(Cancelled)、已拒绝(Rejected)
        // 包括：PendingRisk, PendingRoute, Submitted, PartiallyFilled
        this.pendingOrders = (data.orders || []).filter(o => {
          const status = o.status
          return status !== 'Filled' && status !== 'Cancelled' && status !== 'Rejected'
        })
      } catch (error) {
        console.error('Failed to load orders:', error)
        this.loadError = this.$errMsg(error, '加载失败')
      }
    },

    async loadRecentTrades() {
      if (!this.selectedInstrument) return

      try {
        const trades = await getRecentTrades(this.selectedInstrument, 20)

        // 转换后端数据格式为前端需要的格式
        this.recentTrades = (trades || []).map(trade => {
          // 将纳秒时间戳转换为 HH:MM:SS 格式
          const date = new Date(trade.timestamp / 1000000)  // 纳秒 → 毫秒
          const timeStr = date.toLocaleTimeString('zh-CN', {
            hour: '2-digit',
            minute: '2-digit',
            second: '2-digit'
          })

          return {
            time: timeStr,
            price: trade.price,
            volume: trade.volume,
            direction: trade.direction  // backend已经提供了direction字段
          }
        })
      } catch (error) {
        console.error('Failed to load recent trades:', error)
      }
    },

    handleInstrumentChange() {
      // ✨ P1-3: WebSocket 模式下重新订阅行情 @yutiansut @quantaxis
      if (this.ws && this.ws.isConnected()) {
        this.ws.subscribeQuote([this.selectedInstrument])
        console.log('[Trade] Subscribed to new instrument:', this.selectedInstrument)
      }
      // 同时加载订单簿和成交（HTTP 补充）
      this.loadOrderBook()
      this.loadTick()
      this.loadRecentTrades()
    },

    setDepth(depth) {
      this.depth = depth
      this.loadOrderBook()
    },

    handlePriceClick(price, direction) {
      this.activeTab = direction === 'BUY' ? 'buy' : 'sell'
      this.$nextTick(() => {
        // 触发价格填充到表单（通过 EventBus 或其他方式）
        this.$emit('price-selected', price)
      })
    },

    // ⚠️ `done` 是子组件传来的**复位回调** —— 必须在所有返回路径上调到,
    // 包括提前 return 的校验失败分支。@yutiansut @quantaxis
    // 漏调会让按钮一直转;但那是**可见**故障,好过静默重复下单
    // (原实现用固定 1 秒定时器复位,与请求无关,见 OrderForm 的说明)。
    async handleOrderSubmit(orderData, done) {
      const finish = () => { if (typeof done === 'function') done() }
      if (!this.currentUser) {
        this.$message.warning('请先登录')
        finish()
        return
      }

      // 检查子组件是否已经提供了 user_id（实际是 account_id）
      if (!orderData.user_id) {
        this.$message.warning('请选择交易账户')
        finish()
        return
      }

      try {
        await submitOrder({
          ...orderData,
          instrument_id: this.selectedInstrument
          // 注意：不再覆盖 user_id，使用子组件传来的 account_id
        })

        this.$message.success('订单提交成功')
        this.loadPendingOrders()

        // ✨ 平仓下单后刷新持仓数据，更新可平量显示 @yutiansut @quantaxis
        if (orderData.offset === 'CLOSE' && this.$refs.closeForm) {
          this.$refs.closeForm.refresh()
        }
      } catch (error) {
        this.$message.error('订单提交失败: ' + extractErrorMessage(error))
      } finally {
        // 无论成功失败都复位 —— 放 finally 而不是只在成功路径,
        // 否则一次失败后按钮永远卡住。@yutiansut @quantaxis
        finish()
      }
    },

    async handleCancelOrder(row) {
      try {
        // ✨ 交易所模式：user_id 和 account_id 都使用账户ID @yutiansut @quantaxis
        await cancelOrder({
          user_id: row.user_id,     // 账户ID (交易所模式)
          account_id: row.user_id,  // 必须传递
          order_id: row.order_id
        })
        this.$message.success('撤单成功')
        this.loadPendingOrders()

        // ✨ 撤单后刷新账户数据，更新可用资金显示 @yutiansut @quantaxis
        const userInfo = this.$store.state.userInfo
        const userId = userInfo && userInfo.user_id
        if (userId) {
          try {
            await this.fetchUserAccounts(userId)
          } catch (err) {
            console.warn('[Trade] Failed to refresh account after cancel:', err)
          }
        }
      } catch (error) {
        this.$message.error('撤单失败: ' + extractErrorMessage(error))
      }
    },

    getStatusType(status) {
      const map = {
        'PendingRisk': 'warning',
        'PendingRoute': 'warning',
        'Submitted': 'warning',
        'PartiallyFilled': 'info',
        'Filled': 'success',
        'Cancelled': 'info',
        'Rejected': 'danger'
      }
      return map[status] || 'info'
    },

    getStatusText(status) {
      const map = {
        'PendingRisk': '风控中',
        'PendingRoute': '路由中',
        'Submitted': '已提交',
        'PartiallyFilled': '部分成交',
        'Filled': '已成交',
        'Cancelled': '已撤销',
        'Rejected': '已拒绝'
      }
      return map[status] || status
    },

    startAutoRefresh() {
      this.loadOrderBook()
      this.loadTick()
      this.loadPendingOrders()
      this.loadRecentTrades()

      this.refreshTimer = setInterval(() => {
        this.loadOrderBook()
        this.loadTick()
        this.loadPendingOrders()
        this.loadRecentTrades()
      }, 2000)  // 每2秒刷新
    },

    stopAutoRefresh() {
      if (this.refreshTimer) {
        clearInterval(this.refreshTimer)
        this.refreshTimer = null
      }
      if (this.ordersRefreshTimer) {
        clearInterval(this.ordersRefreshTimer)
        this.ordersRefreshTimer = null
      }
    },

    // ====================================================================
    // ✨ P1-3: WebSocket 实时数据集成 @yutiansut @quantaxis
    // ====================================================================

    /**
     * 初始化 WebSocket 连接
     */
    initWebSocket() {
      // ✨ 订阅键必须是 **account_id** 而不是 user_id @yutiansut @quantaxis
      //
      // 与 store/modules/websocket.js:163-171 保持一致(那里记着实测结论):
      //   ws://.../ws/diff?user_id=<用户 UUID>  → patch 只有 quotes / notify / ins_list
      //   ws://.../ws/diff?user_id=ACC_xxxxxx   → patch 还有 trades / orders / accounts
      // 后端 trade_gateway.rs:1660 的账户 patch 明确 push_patch(&account_id, ..),
      // account_id 形如 `ACC_<uuid>`(account_mgr.rs:108),与 user_id 是两个命名空间。
      //
      // 原先 user_id 优先 → 本页的成交/委托/账户三类推送全部收不到,
      // 只能靠 5 秒 HTTP 轮询兜底。这里把顺序对调。
      // ⚠️ `this.currentUser` 是**字符串**(store/index.js:142 存的就是 userId 本身),
      //    对它取 `.user_id` 恒为 undefined —— 这半截兜底从来没生效过,已删。
      //    而 mounted() 里 initWebSocket 先于 OrderForm 的 account-change 触发,
      //    此时 selectedAccountId 还是 null → 整个表达式落到 'anonymous',
      //    于是 orders/trades/accounts 三类推送全收不到(后端按 account_id 分发)。
      //    修法:没有 account_id 就先不连,等 handleAccountChange 拿到再连。
      //    @yutiansut @quantaxis
      const userId = this.selectedAccountId
      if (!userId) {
        console.info('[trade] 尚未选定账户,WebSocket 暂不连接,待 account-change 后再连')
        return
      }

      this.ws = new WebSocketManager({
        autoConnect: false,
        userId: userId,
        logLevel: 'DEBUG'
      })

      // 监听连接状态变化
      this.ws.on('stateChange', ({ newState }) => {
        this.wsState = newState
        console.log('[Trade] WebSocket state changed:', newState)
      })

      // 监听消息更新
      this.ws.on('message', () => {
        this.handleSnapshotUpdate()
      })

      // 监听连接打开
      this.ws.on('open', () => {
        console.log('[Trade] WebSocket connected, subscribing to quotes')
        // 订阅当前选中合约的行情
        if (this.selectedInstrument) {
          this.ws.subscribeQuote([this.selectedInstrument])
        }
        // 初始加载订单簿和成交（HTTP 作为补充）
        this.loadOrderBook()
        this.loadRecentTrades()
      })

      // 监听错误
      this.ws.on('error', (error) => {
        console.error('[Trade] WebSocket error:', error)
        this.$message.error('WebSocket 连接错误')
      })

      // 连接 WebSocket
      this.ws.connect(userId).catch(error => {
        console.error('[Trade] Failed to connect WebSocket:', error)
        // 回退到 HTTP 轮询
        this.$message.warning('WebSocket 连接失败，使用 HTTP 轮询')
        this.startAutoRefresh()
      })

      // 委托单仍然使用 HTTP 轮询（频率降低到 5 秒）
      this.loadPendingOrders()
      // 纵深防御:建之前先清旧的 —— 即使调用方忘了 destroy 也不会泄漏。
      // @yutiansut @quantaxis
      if (this.ordersRefreshTimer) {
        clearInterval(this.ordersRefreshTimer)
      }
      this.ordersRefreshTimer = setInterval(() => {
        this.loadPendingOrders()
      }, 5000)
    },

    /**
     * 销毁 WebSocket 连接
     */
    destroyWebSocket() {
      // ⚠️ 必须在这里清委托轮询定时器 —— 它是在 initWebSocket 里建的。
      // @yutiansut @quantaxis
      //
      // 回验代理实测:`handleAccountChange`(:429)每次切账户都
      // destroy + init,而 destroy **不清** `ordersRefreshTimer`
      // (清理只写在 `stopAutoRefresh`,那是 beforeDestroy 才走的)。
      // ⇒ **每切一次账户泄漏一个 5 秒轮询器**:12 次切换后活跃定时器
      // 由 3 涨到 15(setInterval +2 / clearInterval +1,只清了 WS 心跳)。
      // 更糟的是**离开页面后仍在跑** —— 在 /#/password 上 30 秒内
      // 观察到 **72 个** `/api/order/user` 请求。
      if (this.ordersRefreshTimer) {
        clearInterval(this.ordersRefreshTimer)
        this.ordersRefreshTimer = null
      }
      if (this.snapshotUnsubscribe) {
        this.snapshotUnsubscribe()
        this.snapshotUnsubscribe = null
      }
      if (this.ws) {
        this.ws.destroy()
        this.ws = null
      }
    },

    /**
     * 处理快照更新
     * 从 WebSocket snapshot 中提取行情数据更新 UI
     */
    handleSnapshotUpdate() {
      if (!this.ws) return

      const snapshot = this.ws.getSnapshot()
      if (!snapshot) return

      // 更新行情数据
      this.updateTickFromSnapshot(snapshot)

      // 更新订单簿（如果有）
      this.updateOrderbookFromSnapshot(snapshot)

      // 更新委托单（如果有）
      this.updateOrdersFromSnapshot(snapshot)
    },

    /**
     * 从快照更新 Tick 数据
     */
    updateTickFromSnapshot(snapshot) {
      const quotes = snapshot.quotes || {}
      const quote = quotes[this.selectedInstrument]

      if (quote) {
        // 计算价格变化
        if (this.tickData.last_price && quote.last_price) {
          this.prevPrice = this.tickData.last_price
          this.priceChange = quote.last_price - this.prevPrice
          if (this.prevPrice !== 0) {
            this.priceChangePercent = (this.priceChange / this.prevPrice) * 100
          }
        }

        // 更新 tick 数据
        // ⚠️ 用 `??` 而非 `|| 0` —— 后者把「字段缺失」变成一个**错的数**。
        // @yutiansut @quantaxis
        //
        // 实测(2026-09-05,回验代理):默认 `wsEnabled: true`,活的写入方就是本函数。
        // 而 DIFF 推来的 quote 对 IF2501 **既没有 volume 也没有 last_price**,
        // `|| 0` 于是让 vm 变成 `{last_price: 0, volume: 0}`,页面显示 **成交量 0**;
        // 同一时刻后端 `GET /api/market/tick/IF2501` 返回 `volume: 1`。
        // **这是显示了一个错的数,不只是没显示** —— 比空白更坏。
        // (点 WS 标签切到 HTTP 轮询立刻恢复成 1,证明数据本身是有的。)
        //
        // 改成 `??` 后缺失字段保持 null,交由模板的三态判断显示 `--`;
        // 而真值 0(确实没成交)会原样保留 —— `0 ?? x` 是 0,`0 || x` 是 x。
        // ⚠️ 不能用 `??` —— 本项目的 babel 配置不支持空值合并运算符,
        // 会在 `vue&type=script` 阶段编译失败(实测 2026-09-05)。用显式判断等价实现。
        const pick = (...vs) => {
          for (const v of vs) if (v !== null && v !== undefined) return v
          return null
        }
        this.tickData = {
          last_price: pick(quote.last_price),
          bid_price: pick(quote.bid_price1, quote.bid_price),
          ask_price: pick(quote.ask_price1, quote.ask_price),
          volume: pick(quote.volume),
          // 额外字段
          open: quote.open,
          high: quote.highest || quote.high,
          low: quote.lowest || quote.low,
          pre_close: quote.pre_close,
          open_interest: quote.open_interest
        }
      }
    },

    /**
     * 从快照更新订单簿
     * 注意：DIFF 协议中 orderbook 数据可能在 quotes 或单独的 orderbook 字段
     */
    updateOrderbookFromSnapshot(snapshot) {
      const quotes = snapshot.quotes || {}
      const quote = quotes[this.selectedInstrument]

      if (quote) {
        // 从五档行情构建订单簿
        const bids = []
        const asks = []

        // ✨ 解析到 10 档 —— 原先写死 5 档 @yutiansut @quantaxis
        //
        // 后端 DIFF quote 现在推 bid_price1~10 / ask_price1~10
        // (diff_handler.rs 的 quote 构造)。原先这里写死 `i <= 5`,
        // 于是点「10档」后:HTTP 拿到 10 档渲染出来 → 下一帧 WS 快照只解析 5 档
        // → 覆盖回 5 档,表现为「跳一下 10 档然后还是 5 档」。
        const MAX_LEVELS = 10

        // 解析买盘（bid_price1~10, bid_volume1~10）
        for (let i = 1; i <= MAX_LEVELS; i++) {
          const price = quote[`bid_price${i}`]
          const volume = quote[`bid_volume${i}`]
          if (price && price > 0) {
            bids.push({ price, volume: volume || 0 })
          }
        }

        // 解析卖盘（ask_price1~10, ask_volume1~10）
        for (let i = 1; i <= MAX_LEVELS; i++) {
          const price = quote[`ask_price${i}`]
          const volume = quote[`ask_volume${i}`]
          if (price && price > 0) {
            asks.push({ price, volume: volume || 0 })
          }
        }

        // 如果有五档数据则更新订单簿
        if (bids.length > 0 || asks.length > 0) {
          this.orderbook = {
            bids: bids.slice(0, this.depth),
            asks: asks.slice(0, this.depth)
          }
        }
      }
    },

    /**
     * 从快照更新委托单
     */
    updateOrdersFromSnapshot(snapshot) {
      const trade = snapshot.trade || {}
      const userData = trade[this.selectedAccountId] || {}
      const orders = userData.orders || {}

      if (Object.keys(orders).length > 0) {
        // 转换为数组并过滤活跃订单
        const orderList = Object.values(orders).filter(o => {
          const status = o.status
          return status !== 'FINISHED' && status !== 'Filled' && status !== 'Cancelled' && status !== 'Rejected'
        })

        // 格式转换以匹配前端期望的格式
        this.pendingOrders = orderList.map(o => ({
          order_id: o.order_id,
          instrument_id: o.instrument_id,
          direction: o.direction,
          price: o.limit_price || o.price || 0,
          volume: o.volume_orign || o.volume || 0,
          filled_volume: (Number(o.volume_orign) || 0) - (Number(o.volume_left) || 0),
          status: this.mapOrderStatus(o.status),
          user_id: o.user_id
        }))
      }
    },

    /**
     * 映射 QIFI 订单状态到前端状态
     */
    mapOrderStatus(qifiStatus) {
      const statusMap = {
        'ALIVE': 'Submitted',
        'FINISHED': 'Filled',
        'PENDING': 'PendingRisk'
      }
      return statusMap[qifiStatus] || qifiStatus
    },

    /**
     * 切换 WebSocket 启用状态
     */
    toggleWebSocket() {
      this.wsEnabled = !this.wsEnabled

      if (this.wsEnabled) {
        this.stopAutoRefresh()
        this.initWebSocket()
        this.$message.success('已切换到 WebSocket 实时推送')
      } else {
        this.destroyWebSocket()
        this.startAutoRefresh()
        this.$message.info('已切换到 HTTP 轮询模式')
      }
    }
  }
}
</script>

<style lang="scss" scoped>
// @yutiansut @quantaxis - 专业量化交易页面深色主题
$dark-bg-primary: #0d1117;
$dark-bg-secondary: #161b22;
$dark-bg-tertiary: #21262d;
$dark-bg-card: #1c2128;
$dark-border: #30363d;
$dark-text-primary: #f0f6fc;
$dark-text-secondary: #8b949e;
$dark-text-muted: #6e7681;

$buy-color: #f5222d;
$buy-light: #ff4d4f;
$buy-bg: rgba(245, 34, 45, 0.1);
$sell-color: #52c41a;
$sell-light: #73d13d;
$sell-bg: rgba(82, 196, 26, 0.1);
$primary-color: #1890ff;

.trade-page {
  padding: 16px;
  background: $dark-bg-primary;
  // ⚠️ 用 --qa-content-h,不要手写 calc(100vh - N)。@yutiansut @quantaxis
  //    原值 -56px 少算了 content-wrapper 的上下 padding(各 20px):
  //    真实 chrome = top-header 56 + padding 40 = 96px(有公告条时再 +40)。
  //    实测本页在无内容可滚时仍有 41px 的死滚动条
  //    (.content-wrapper 的 scrollHeight - clientHeight = 41)。
  //    `layout/index.vue:546` 早就写明「各页面请一律用 var(--qa-content-h)」。
  min-height: var(--qa-content-h, calc(100vh - 96px));

  ::v-deep .el-card {
    background: $dark-bg-card;
    border: 1px solid $dark-border;
    border-radius: 8px;
    margin-bottom: 16px;

    &:last-child {
      margin-bottom: 0;
    }

    .el-card__header {
      background: $dark-bg-card !important;
      border-bottom: 1px solid $dark-border;
      padding: 12px 16px;
      color: $dark-text-primary !important;
      font-weight: 600;
    }

    .el-card__body {
      padding: 16px;
      background: $dark-bg-card !important;
    }
  }

  .card-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    color: $dark-text-primary;
  }

  // 合约选择器 + WebSocket 状态 @yutiansut @quantaxis
  .instrument-selector {
    ::v-deep .el-card__body {
      padding: 12px 16px;
    }

    .selector-header {
      display: flex;
      align-items: center;
    }

    .ws-status-tag {
      white-space: nowrap;
      transition: all 0.2s ease;

      &:hover {
        transform: scale(1.05);
      }

      i {
        margin-right: 4px;
      }
    }

    ::v-deep .el-select {
      .el-input__inner {
        background: $dark-bg-tertiary;
        border-color: $dark-border;
        color: $dark-text-primary;
        font-weight: 600;

        &:focus {
          border-color: $primary-color;
        }
      }
    }
  }

  // 实时行情
  .market-info {
    .price-display {
      text-align: center;
      padding: 16px 0;

      .last-price {
        font-size: 42px;
        font-weight: 700;
        font-family: 'JetBrains Mono', monospace;
        color: $dark-text-primary;
        letter-spacing: -1px;

        &.price-up {
          color: $buy-color;
          text-shadow: 0 0 20px rgba(245, 34, 45, 0.3);
        }

        &.price-down {
          color: $sell-color;
          text-shadow: 0 0 20px rgba(82, 196, 26, 0.3);
        }
      }

      .price-change {
        font-size: 14px;
        margin-top: 8px;
        color: $dark-text-secondary;
        font-family: 'JetBrains Mono', monospace;
      }
    }

    .tick-info {
      margin-top: 16px;
      padding-top: 16px;
      border-top: 1px solid $dark-border;

      .info-item {
        display: flex;
        flex-direction: column;
        align-items: center;

        .label {
          font-size: 12px;
          color: $dark-text-muted;
          margin-bottom: 6px;
          text-transform: uppercase;
          letter-spacing: 0.5px;
        }

        .value {
          font-size: 18px;
          font-weight: 600;
          font-family: 'JetBrains Mono', monospace;
          color: $dark-text-primary;

          &.bid {
            color: $buy-color;
          }

          &.ask {
            color: $sell-color;
          }
        }
      }
    }
  }

  // 订单簿 - 修复文字可见性 @yutiansut @quantaxis
  .orderbook {
    ::v-deep .el-card__header {
      background: $dark-bg-card !important;
      color: $dark-text-primary !important;
    }

    .orderbook-content {
      font-family: 'JetBrains Mono', 'Monaco', monospace;
      font-size: 13px;
      background: $dark-bg-card !important;

      .header-row {
        display: flex;
        justify-content: space-between;
        padding: 8px 12px;
        background: $dark-bg-tertiary !important;
        font-weight: 600;
        color: $dark-text-secondary !important;
        font-size: 11px;
        text-transform: uppercase;
        letter-spacing: 0.5px;
        border-radius: 4px;
        margin-bottom: 4px;

        span {
          color: $dark-text-secondary !important;
        }
      }

      .order-row {
        position: relative;
        display: flex;
        justify-content: space-between;
        padding: 6px 12px;
        cursor: pointer;
        transition: all 0.15s ease;
        border-radius: 4px;
        margin: 1px 0;
        background: transparent;

        &:hover {
          background: $dark-bg-tertiary !important;
        }

        .price,
        .volume {
          position: relative;
          z-index: 1;
        }

        .price {
          font-weight: 600;
        }

        .volume {
          color: $dark-text-secondary !important;
        }

        .volume-bar {
          position: absolute;
          right: 0;
          top: 0;
          height: 100%;
          opacity: 0.15;
          border-radius: 4px;
        }

        &.ask-row {
          .price { color: $sell-color !important; }
          .volume-bar { background: $sell-color; }
        }

        &.bid-row {
          .price { color: $buy-color !important; }
          .volume-bar { background: $buy-color; }
        }
      }

      .last-price-separator {
        padding: 12px;
        text-align: center;
        background: linear-gradient(90deg, transparent, $dark-bg-tertiary, transparent);
        margin: 8px 0;
        border-radius: 4px;

        .price {
          font-size: 18px;
          font-weight: 700;
          color: $dark-text-primary !important;

          &.price-up { color: $buy-color !important; }
          &.price-down { color: $sell-color !important; }
        }
      }
    }
  }

  // 最新成交 - 修复文字可见性 @yutiansut @quantaxis
  .recent-trades {
    ::v-deep .el-card__header {
      background: $dark-bg-card !important;
      color: $dark-text-primary !important;
    }

    .trades-list {
      font-family: 'JetBrains Mono', 'Monaco', monospace;
      font-size: 12px;
      max-height: 280px;
      overflow-y: auto;
      background: $dark-bg-card !important;

      &::-webkit-scrollbar {
        width: 4px;
      }

      &::-webkit-scrollbar-thumb {
        background: $dark-border;
        border-radius: 2px;
      }

      .header-row {
        display: grid;
        grid-template-columns: 70px 1fr 60px;
        padding: 8px 12px;
        background: $dark-bg-tertiary !important;
        font-weight: 600;
        color: $dark-text-secondary !important;
        font-size: 11px;
        text-transform: uppercase;
        letter-spacing: 0.5px;
        position: sticky;
        top: 0;
        z-index: 1;
        border-radius: 4px;

        span {
          color: $dark-text-secondary !important;
        }
      }

      .trade-row {
        display: grid;
        grid-template-columns: 70px 1fr 60px;
        padding: 6px 12px;
        border-radius: 4px;
        transition: background 0.15s ease;

        &:hover {
          background: $dark-bg-tertiary !important;
        }

        &.buy-trade .price { color: $buy-color !important; }
        &.sell-trade .price { color: $sell-color !important; }

        .time { color: $dark-text-muted !important; }
        .price { font-weight: 600; }
        .volume { color: $dark-text-secondary !important; }
      }
    }
  }

  // 订单面板
  .order-panel {
    min-height: 400px;

    ::v-deep .el-card__body {
      background: $dark-bg-card !important;
    }

    /* 方向切换按钮组 —— 取代原 el-tabs @yutiansut @quantaxis */
    .op-switch {
      display: flex;
      gap: 0;
      margin: -4px 0 16px;
      border: 1px solid $dark-border;
      border-radius: 4px;
      overflow: hidden;

      .op-btn {
        flex: 1;
        padding: 9px 0;
        font-size: 14px;
        font-weight: 600;
        letter-spacing: 1px;
        color: $dark-text-secondary;
        background: transparent;
        border: none;
        border-right: 1px solid $dark-border;
        cursor: pointer;
        transition: all .15s;

        &:last-child { border-right: none; }
        &:hover { color: $dark-text-primary; background: rgba(255,255,255,.04); }

        &.is-on {
          color: #fff;
          &.op-buy   { background: #f56c6c; }
          &.op-sell  { background: #67c23a; }
          &.op-close { background: #909399; }
        }
      }
    }

    ::v-deep .el-input__inner,
    ::v-deep .el-input-number__decrease,
    ::v-deep .el-input-number__increase {
      background: $dark-bg-tertiary;
      border-color: $dark-border;
      color: $dark-text-primary;
    }

    ::v-deep .el-button--primary {
      background: linear-gradient(135deg, $primary-color 0%, #096dd9 100%);
      border: none;

      &:hover {
        background: linear-gradient(135deg, #40a9ff 0%, $primary-color 100%);
      }
    }
  }

  // 当前委托 - 修复文字可见性 @yutiansut @quantaxis
  .pending-orders {
    // 强制覆盖全局样式
    &.el-card {
      background: $dark-bg-card !important;
      border-color: $dark-border !important;
    }

    ::v-deep .el-card__header {
      background: $dark-bg-card !important;
      background-color: $dark-bg-card !important;
      color: $dark-text-primary !important;
      border-bottom-color: $dark-border !important;
    }

    ::v-deep .el-card__body {
      background: $dark-bg-card !important;
      background-color: $dark-bg-card !important;
      padding: 0 !important;
    }

    ::v-deep .el-table {
      background: $dark-bg-card !important;
      background-color: $dark-bg-card !important;
      color: $dark-text-primary !important;

      // 表格边框
      &::before,
      &::after {
        background-color: $dark-border !important;
      }

      // 表格包装器
      .el-table__header-wrapper,
      .el-table__body-wrapper,
      .el-table__footer-wrapper {
        background: $dark-bg-card !important;
        background-color: $dark-bg-card !important;
      }

      // 表头
      .el-table__header {
        background: $dark-bg-tertiary !important;

        th,
        th.el-table__cell {
          background: $dark-bg-tertiary !important;
          background-color: $dark-bg-tertiary !important;
          color: $dark-text-secondary !important;
          border-bottom: 1px solid $dark-border !important;
          border-right-color: $dark-border !important;
          font-weight: 600;

          .cell {
            color: $dark-text-secondary !important;
          }
        }
      }

      // 表体
      .el-table__body {
        background: $dark-bg-card !important;

        tr,
        tr.el-table__row {
          background: $dark-bg-card !important;
          background-color: $dark-bg-card !important;

          td,
          td.el-table__cell {
            background: $dark-bg-card !important;
            background-color: $dark-bg-card !important;
            border-bottom: 1px solid $dark-border !important;
            border-right-color: $dark-border !important;
            color: $dark-text-primary !important;

            .cell {
              color: $dark-text-primary !important;
            }
          }
        }

        // 斑马纹行
        tr.el-table__row--striped,
        .el-table__row--striped {
          background: $dark-bg-secondary !important;

          td,
          td.el-table__cell {
            background: $dark-bg-secondary !important;
            background-color: $dark-bg-secondary !important;
            color: $dark-text-primary !important;

            .cell {
              color: $dark-text-primary !important;
            }
          }
        }

        // 悬停行 - 确保文字可见
        tr:hover,
        tr.el-table__row:hover,
        .el-table__row:hover {
          td,
          td.el-table__cell,
          > td.el-table__cell {
            background: $dark-bg-tertiary !important;
            background-color: $dark-bg-tertiary !important;
            color: $dark-text-primary !important;

            .cell {
              color: $dark-text-primary !important;
            }
          }
        }
      }

      // 固定列
      .el-table__fixed,
      .el-table__fixed-right {
        background: $dark-bg-card !important;
        box-shadow: none !important;

        .el-table__fixed-header-wrapper,
        .el-table__fixed-body-wrapper {
          background: $dark-bg-card !important;
        }

        th,
        th.el-table__cell {
          background: $dark-bg-tertiary !important;
          color: $dark-text-secondary !important;
        }

        td,
        td.el-table__cell {
          background: $dark-bg-card !important;
          color: $dark-text-primary !important;
        }

        tr.el-table__row--striped td,
        tr.el-table__row--striped td.el-table__cell {
          background: $dark-bg-secondary !important;
        }

        tr:hover td,
        tr:hover td.el-table__cell {
          background: $dark-bg-tertiary !important;
        }
      }

      // 固定列阴影
      .el-table__fixed-right::before,
      .el-table__fixed::before {
        background-color: $dark-border !important;
      }

      // 空数据
      .el-table__empty-block {
        background: $dark-bg-card !important;
        background-color: $dark-bg-card !important;
      }

      .el-table__empty-text {
        color: $dark-text-muted !important;
      }

      // 排序图标
      .sort-caret {
        border-color: transparent !important;

        &.ascending {
          border-bottom-color: $dark-text-secondary !important;
        }

        &.descending {
          border-top-color: $dark-text-secondary !important;
        }
      }

      // 滚动条
      .el-table__body-wrapper::-webkit-scrollbar {
        // ⚠️ 必须与 Element 的 gutterWidth(8)一致,统一走 --qa-sbw。@yutiansut @quantaxis
        //    原为 6px ⇒ rowOff = 6-8 = -2:方向相反故不产生滞后,
        //    但冻结列底边会比正文可视区低 2px。
        width: var(--qa-sbw, 8px);
        height: var(--qa-sbw, 8px);
      }

      .el-table__body-wrapper::-webkit-scrollbar-thumb {
        background: $dark-border;
        border-radius: 3px;
      }

      .el-table__body-wrapper::-webkit-scrollbar-track {
        background: $dark-bg-secondary;
      }
    }

    .buy-text {
      color: $buy-color !important;
      font-weight: 600;
    }

    .sell-text {
      color: $sell-color !important;
      font-weight: 600;
    }
  }
}

// Element UI 深色主题覆盖
::v-deep .el-button-group {
  .el-button {
    background: $dark-bg-tertiary;
    border-color: $dark-border;
    color: $dark-text-secondary;

    &:hover {
      color: $primary-color;
      border-color: $primary-color;
    }

    &.el-button--primary {
      background: $primary-color;
      border-color: $primary-color;
      color: white;
    }
  }
}

::v-deep .el-tag {
  border: none;

  &.el-tag--warning {
    background: rgba(250, 173, 20, 0.15);
    color: #faad14;
  }

  &.el-tag--success {
    background: rgba(82, 196, 26, 0.15);
    color: $sell-color;
  }

  &.el-tag--info {
    background: rgba(139, 148, 158, 0.15);
    color: $dark-text-secondary;
  }

  &.el-tag--danger {
    background: rgba(245, 34, 45, 0.15);
    color: $buy-color;
  }
}

// 响应式
@media (max-width: 1200px) {
  .trade-page {
    padding: 12px;

    .market-info .price-display .last-price {
      font-size: 32px;
    }
  }
}

/* ① 顶部条:合约 + 行情 + 连接状态 @yutiansut @quantaxis */
.trade-topbar {
  display: flex;
  align-items: center;
  gap: 16px;
  flex-wrap: wrap;
  padding: 10px 14px;
  margin-bottom: 16px;
  /* ⚠️ 原来写的是 var(--qa-card-bg, #fff) —— 该 CSS 变量在本项目中
     **从未定义**,于是回退到 #fff,顶部条变成整条纯白(实测 1140×58
     rgb(255,255,255))。改用 SCSS 变量。@yutiansut @quantaxis */
  background: $dark-bg-card;
  border: 1px solid $dark-border;
  border-radius: 4px;

  /* 220px 装不下 `IF2501 - 沪深300股指期货2501`(实测需 226px,尾字被切)
     @yutiansut @quantaxis */
  .tb-inst { flex: 0 0 260px; max-width: 100%; }

  .tb-price {
    display: flex;
    align-items: baseline;
    gap: 8px;
    &.price-up   .tb-last, &.price-up   .tb-chg { color: #f56c6c; }
    &.price-down .tb-last, &.price-down .tb-chg { color: #67c23a; }
    .tb-last {
      color: $dark-text-primary;
      font-size: 26px;
      font-weight: 700;
      line-height: 1;
      font-variant-numeric: tabular-nums;
    }
    .tb-chg { font-size: 13px; font-variant-numeric: tabular-nums; }
  }

  .tb-quotes {
    display: flex;
    gap: 18px;
    margin-left: auto;
    .tb-q {
      display: flex;
      flex-direction: column;
      align-items: flex-end;
      span { font-size: 11px; color: #909399; line-height: 1.4; }
      b {
        font-size: 14px;
        font-weight: 600;
        font-variant-numeric: tabular-nums;
        &.bid { color: #67c23a; }
        &.ask { color: #f56c6c; }
      }
    }
  }

  .tb-ws { cursor: pointer; flex: 0 0 auto; }
}

/* 窄屏:行情与盘口报价换行,合约选择占整宽 */
@media (max-width: 768px) {
  .trade-topbar {
    gap: 10px;
    .tb-inst { flex: 1 1 100%; }
    .tb-quotes { margin-left: 0; width: 100%; justify-content: space-between; }
    .tb-price .tb-last { font-size: 22px; }
  }
}
</style>
