<template>
  <div class="trade-page">
    <el-row :gutter="20">
      <!-- 左侧：订单簿和最新成交 -->
      <el-col :span="8">
        <!-- 合约选择 -->
        <el-card class="instrument-selector" shadow="hover">
          <el-select
            v-model="selectedInstrument"
            @change="handleInstrumentChange"
            placeholder="选择合约"
            style="width: 100%"
            size="medium"
          >
            <el-option
              v-for="inst in instruments"
              :key="inst.instrument_id"
              :label="`${inst.instrument_id} - ${inst.name}`"
              :value="inst.instrument_id"
            />
          </el-select>
        </el-card>

        <!-- 实时行情 -->
        <el-card class="market-info" shadow="hover">
          <div class="price-display">
            <div class="last-price" :class="priceChangeClass">
              {{ tickData.last_price ? tickData.last_price.toFixed(2) : '--' }}
            </div>
            <div class="price-change">
              <span v-if="priceChange !== 0">
                {{ priceChange > 0 ? '+' : '' }}{{ priceChange.toFixed(2) }}
                ({{ priceChangePercent > 0 ? '+' : '' }}{{ priceChangePercent.toFixed(2) }}%)
              </span>
            </div>
          </div>
          <el-row :gutter="10" class="tick-info">
            <el-col :span="8">
              <div class="info-item">
                <span class="label">买一</span>
                <span class="value bid">{{ bestBidPrice ? bestBidPrice.toFixed(2) : '--' }}</span>
              </div>
            </el-col>
            <el-col :span="8">
              <div class="info-item">
                <span class="label">卖一</span>
                <span class="value ask">{{ bestAskPrice ? bestAskPrice.toFixed(2) : '--' }}</span>
              </div>
            </el-col>
            <el-col :span="8">
              <div class="info-item">
                <span class="label">成交量</span>
                <span class="value">{{ tickData.volume || 0 }}</span>
              </div>
            </el-col>
          </el-row>
        </el-card>

        <!-- 订单簿 -->
        <el-card class="orderbook" shadow="hover">
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
        <el-card class="recent-trades" shadow="hover">
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
      <el-col :span="16">
        <el-card class="order-panel" shadow="hover">
          <el-tabs v-model="activeTab">
            <!-- 买入/卖出 -->
            <el-tab-pane label="买入开仓" name="buy">
              <order-form
                :instrument-id="selectedInstrument"
                :current-price="tickData.last_price"
                direction="BUY"
                offset="OPEN"
                @submit="handleOrderSubmit"
                @account-change="handleAccountChange"
              />
            </el-tab-pane>

            <el-tab-pane label="卖出开仓" name="sell">
              <order-form
                :instrument-id="selectedInstrument"
                :current-price="tickData.last_price"
                direction="SELL"
                offset="OPEN"
                @submit="handleOrderSubmit"
                @account-change="handleAccountChange"
              />
            </el-tab-pane>

            <el-tab-pane label="平仓" name="close">
              <close-form
                :instrument-id="selectedInstrument"
                :current-price="tickData.last_price"
                @submit="handleOrderSubmit"
                @account-change="handleAccountChange"
              />
            </el-tab-pane>
          </el-tabs>
        </el-card>

        <!-- 当前委托 -->
        <el-card class="pending-orders" shadow="hover">
          <div slot="header" class="card-header">
            <span>当前委托</span>
            <el-button size="mini" @click="loadPendingOrders">刷新</el-button>
          </div>
          <el-table
            :data="pendingOrders"
            border
            stripe
            height="300"
            size="mini"
          >
            <el-table-column prop="order_id" label="订单号" width="150" />
            <el-table-column prop="instrument_id" label="合约" width="100" />
            <el-table-column prop="direction" label="方向" width="60" align="center">
              <template slot-scope="scope">
                <span :class="scope.row.direction === 'BUY' ? 'buy-text' : 'sell-text'">
                  {{ scope.row.direction === 'BUY' ? '买' : '卖' }}
                </span>
              </template>
            </el-table-column>
            <el-table-column prop="price" label="价格" width="80" align="right">
              <template slot-scope="scope">
                {{ scope.row.price.toFixed(2) }}
              </template>
            </el-table-column>
            <el-table-column prop="volume" label="数量" width="60" align="right" />
            <el-table-column prop="filled_volume" label="成交" width="60" align="right" />
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
import { mapGetters } from 'vuex'
import OrderForm from './components/OrderForm.vue'
import CloseForm from './components/CloseForm.vue'

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
    }
  },
  data() {
    return {
      instruments: [],
      selectedInstrument: 'IF2501',
      selectedAccountId: null,  // 当前选中的账户ID（用于查询委托）
      activeTab: 'buy',
      depth: 5,
      orderbook: {
        bids: [],
        asks: []
      },
      tickData: {
        last_price: 0,
        bid_price: 0,
        ask_price: 0,
        volume: 0
      },
      recentTrades: [],
      pendingOrders: [],
      prevPrice: 0,
      priceChange: 0,
      priceChangePercent: 0,
      refreshTimer: null
    }
  },
  mounted() {
    this.loadInstruments()
    this.startAutoRefresh()
  },
  beforeDestroy() {
    this.stopAutoRefresh()
  },
  methods: {
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
      this.selectedAccountId = accountId
      // 立即刷新该账户的委托
      this.loadPendingOrders()
    },

    async loadPendingOrders() {
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

    async handleOrderSubmit(orderData) {
      if (!this.currentUser) {
        this.$message.warning('请先登录')
        return
      }

      // 检查子组件是否已经提供了 user_id（实际是 account_id）
      if (!orderData.user_id) {
        this.$message.warning('请选择交易账户')
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
      } catch (error) {
        this.$message.error('订单提交失败: ' + ((error.response && error.response.data && error.response.data.error) || error.message))
      }
    },

    async handleCancelOrder(row) {
      try {
        await cancelOrder({
          user_id: row.user_id,  // 使用订单所属的账户ID
          order_id: row.order_id
        })
        this.$message.success('撤单成功')
        this.loadPendingOrders()
      } catch (error) {
        this.$message.error('撤单失败: ' + ((error.response && error.response.data && error.response.data.error) || error.message))
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
  min-height: calc(100vh - 56px);

  ::v-deep .el-card {
    background: $dark-bg-card;
    border: 1px solid $dark-border;
    border-radius: 8px;
    margin-bottom: 16px;

    &:last-child {
      margin-bottom: 0;
    }

    .el-card__header {
      border-bottom: 1px solid $dark-border;
      padding: 12px 16px;
      color: $dark-text-primary;
      font-weight: 600;
    }

    .el-card__body {
      padding: 16px;
    }
  }

  .card-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    color: $dark-text-primary;
  }

  // 合约选择器
  .instrument-selector {
    ::v-deep .el-card__body {
      padding: 12px 16px;
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

  // 订单簿
  .orderbook {
    .orderbook-content {
      font-family: 'JetBrains Mono', 'Monaco', monospace;
      font-size: 13px;

      .header-row {
        display: flex;
        justify-content: space-between;
        padding: 8px 12px;
        background: $dark-bg-tertiary;
        font-weight: 600;
        color: $dark-text-muted;
        font-size: 11px;
        text-transform: uppercase;
        letter-spacing: 0.5px;
        border-radius: 4px;
        margin-bottom: 4px;
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

        &:hover {
          background: $dark-bg-tertiary;
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
          color: $dark-text-secondary;
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
          .price { color: $sell-color; }
          .volume-bar { background: $sell-color; }
        }

        &.bid-row {
          .price { color: $buy-color; }
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
          color: $dark-text-primary;

          &.price-up { color: $buy-color; }
          &.price-down { color: $sell-color; }
        }
      }
    }
  }

  // 最新成交
  .recent-trades {
    .trades-list {
      font-family: 'JetBrains Mono', 'Monaco', monospace;
      font-size: 12px;
      max-height: 280px;
      overflow-y: auto;

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
        background: $dark-bg-tertiary;
        font-weight: 600;
        color: $dark-text-muted;
        font-size: 11px;
        text-transform: uppercase;
        letter-spacing: 0.5px;
        position: sticky;
        top: 0;
        z-index: 1;
        border-radius: 4px;
      }

      .trade-row {
        display: grid;
        grid-template-columns: 70px 1fr 60px;
        padding: 6px 12px;
        border-radius: 4px;
        transition: background 0.15s ease;

        &:hover {
          background: $dark-bg-tertiary;
        }

        &.buy-trade .price { color: $buy-color; }
        &.sell-trade .price { color: $sell-color; }

        .time { color: $dark-text-muted; }
        .price { font-weight: 600; }
        .volume { color: $dark-text-secondary; }
      }
    }
  }

  // 订单面板
  .order-panel {
    min-height: 400px;

    ::v-deep .el-tabs {
      .el-tabs__header {
        border-bottom: 1px solid $dark-border;
        margin: 0;
      }

      .el-tabs__nav-wrap::after {
        background: $dark-border;
      }

      .el-tabs__item {
        color: $dark-text-secondary;
        font-weight: 500;

        &:hover { color: $dark-text-primary; }

        &.is-active {
          color: $primary-color;
          font-weight: 600;
        }
      }

      .el-tabs__active-bar {
        background: $primary-color;
      }

      .el-tabs__content {
        padding: 16px 0;
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

  // 当前委托
  .pending-orders {
    ::v-deep .el-table {
      background: transparent;
      color: $dark-text-primary;

      &::before { background: $dark-border; }

      th {
        background: $dark-bg-tertiary !important;
        color: $dark-text-secondary;
        border-bottom: 1px solid $dark-border;
        font-weight: 600;
      }

      tr {
        background: transparent;

        &:hover > td { background: $dark-bg-tertiary !important; }
      }

      td {
        border-bottom: 1px solid $dark-border;
        color: $dark-text-primary;
      }

      .el-table__empty-block {
        background: transparent;
      }

      .el-table__empty-text {
        color: $dark-text-muted;
      }
    }

    .buy-text {
      color: $buy-color;
      font-weight: 600;
    }

    .sell-text {
      color: $sell-color;
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
</style>
