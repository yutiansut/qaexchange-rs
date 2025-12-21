<template>
  <div class="trades-page">
    <el-card>
      <div slot="header" class="card-header">
        <span>成交记录</span>
        <el-button type="primary" size="small" @click="loadTrades">
          <i class="el-icon-refresh"></i> 刷新
        </el-button>
      </div>

      <el-form :inline="true" size="small">
        <el-form-item label="合约">
          <el-select v-model="queryInstrument" placeholder="选择合约" clearable>
            <el-option label="IF2501" value="IF2501" />
            <el-option label="IF2502" value="IF2502" />
            <el-option label="IC2501" value="IC2501" />
            <el-option label="IH2501" value="IH2501" />
          </el-select>
        </el-form-item>
        <el-form-item label="日期">
          <el-date-picker
            v-model="queryDate"
            type="daterange"
            range-separator="至"
            start-placeholder="开始日期"
            end-placeholder="结束日期"
            value-format="yyyy-MM-dd"
          />
        </el-form-item>
        <el-form-item>
          <el-button type="primary" @click="handleQuery">查询</el-button>
          <el-button @click="handleReset">重置</el-button>
          <el-button @click="handleExport">导出</el-button>
        </el-form-item>
      </el-form>

      <el-row :gutter="20" style="margin-bottom: 20px">
        <el-col :span="6">
          <el-card shadow="hover">
            <div class="stat-item">
              <div class="stat-label">今日成交</div>
              <div class="stat-value">{{ todayStats.count }}</div>
            </div>
          </el-card>
        </el-col>
        <el-col :span="6">
          <el-card shadow="hover">
            <div class="stat-item">
              <div class="stat-label">成交金额</div>
              <div class="stat-value">¥{{ formatNumber(todayStats.volume) }}</div>
            </div>
          </el-card>
        </el-col>
        <el-col :span="6">
          <el-card shadow="hover">
            <div class="stat-item">
              <div class="stat-label">买入笔数</div>
              <div class="stat-value">{{ todayStats.buy_count }}</div>
            </div>
          </el-card>
        </el-col>
        <el-col :span="6">
          <el-card shadow="hover">
            <div class="stat-item">
              <div class="stat-label">卖出笔数</div>
              <div class="stat-value">{{ todayStats.sell_count }}</div>
            </div>
          </el-card>
        </el-col>
      </el-row>

      <el-table
        :data="tradeList"
        border
        stripe
        height="500"
        :loading="loading"
        show-overflow
      >
        <el-table-column prop="trade_id" label="成交编号" width="200" />
        <el-table-column prop="order_id" label="订单编号" width="200" />
        <el-table-column prop="instrument_id" label="合约" width="100" />
        <el-table-column prop="direction" label="方向" width="80" align="center">
          <template slot-scope="scope">
            <el-tag :type="scope.row.direction === 'BUY' ? 'danger' : 'success'" size="mini">
              {{ scope.row.direction === 'BUY' ? '买入' : '卖出' }}
            </el-tag>
          </template>
        </el-table-column>
        <el-table-column prop="is_taker" label="主动/被动" width="100" align="center">
          <template slot-scope="scope">
            <el-tag :type="scope.row.is_taker ? 'warning' : 'info'" size="mini">
              {{ scope.row.is_taker ? '主动成交' : '被动成交' }}
            </el-tag>
          </template>
        </el-table-column>
        <el-table-column prop="price" label="成交价" width="100" align="right">
          <template slot-scope="scope">
            {{ scope.row.price.toFixed(2) }}
          </template>
        </el-table-column>
        <el-table-column prop="volume" label="成交量" width="80" align="right" />
        <el-table-column prop="trade_amount" label="成交额" width="130" align="right">
          <template slot-scope="scope">
            ¥{{ formatNumber(scope.row.trade_amount) }}
          </template>
        </el-table-column>
        <el-table-column prop="commission" label="手续费" width="100" align="right">
          <template slot-scope="scope">
            ¥{{ scope.row.commission.toFixed(2) }}
          </template>
        </el-table-column>
        <el-table-column prop="trade_time" label="成交时间" width="160" />
        <el-table-column label="操作" width="100" fixed="right">
          <template slot-scope="scope">
            <el-button type="text" size="small" @click="handleViewDetail(scope.row)">
              详情
            </el-button>
          </template>
        </el-table-column>
      </el-table>
    </el-card>
  </div>
</template>

<script>
import { queryUserTrades } from '@/api'
import { mapGetters } from 'vuex'

export default {
  name: 'Trades',
  computed: {
    ...mapGetters(['currentUser'])
  },
  data() {
    return {
      loading: false,
      tradeList: [],
      queryInstrument: '',
      queryDate: null,
      todayStats: {
        count: 0,
        volume: 0,
        buy_count: 0,
        sell_count: 0
      }
    }
  },
  watch: {
    currentUser() {
      this.loadTrades()
    }
  },
  mounted() {
    if (this.currentUser) {
      this.loadTrades()
    }
  },
  methods: {
    formatNumber(num) {
      return (num || 0).toFixed(2).replace(/\B(?=(\d{3})+(?!\d))/g, ',')
    },

    async loadTrades() {
      if (!this.currentUser) {
        this.$message.warning('请先登录')
        return
      }

      this.loading = true
      try {
        const data = await queryUserTrades(this.currentUser)
        const trades = data.trades || []

        // 转换后端数据格式以匹配前端展示
        // @yutiansut @quantaxis - 使用后端返回的 user_direction 字段
        this.tradeList = trades.map(trade => {
          const multiplier = 300

          return {
            trade_id: trade.trade_id,
            order_id: trade.user_order_id, // 使用后端返回的用户订单ID
            instrument_id: trade.instrument_id,
            direction: trade.user_direction, // 直接使用后端返回的用户方向
            offset: 'UNKNOWN', // Backend doesn't provide this, would need order details
            price: trade.price,
            volume: trade.volume,
            trade_amount: trade.price * trade.volume * multiplier,
            commission: trade.price * trade.volume * multiplier * 0.0001,
            trade_time: new Date(trade.timestamp / 1000000).toLocaleString('zh-CN'),
            is_taker: trade.is_taker, // 是否为主动方
            opposite_order_id: trade.opposite_order_id // 对手方订单ID
          }
        })

        this.calculateStats()
      } catch (error) {
        this.$message.error('加载成交记录失败: ' + ((error.response && error.response.data && error.response.data.error) || error.message))
        this.tradeList = []
      } finally {
        this.loading = false
      }
    },

    calculateStats() {
      this.todayStats.count = this.tradeList.length
      this.todayStats.volume = this.tradeList.reduce((sum, t) => sum + t.trade_amount, 0)
      this.todayStats.buy_count = this.tradeList.filter(t => t.direction === 'BUY').length
      this.todayStats.sell_count = this.tradeList.filter(t => t.direction === 'SELL').length
    },

    handleQuery() {
      this.loadTrades()
    },

    handleReset() {
      this.queryInstrument = ''
      this.queryDate = null
      this.loadTrades()
    },

    handleExport() {
      this.$message.info('导出功能开发中...')
    },

    handleViewDetail(row) {
      this.$alert(JSON.stringify(row, null, 2), '成交详情', {
        confirmButtonText: '确定'
      })
    }
  }
}
</script>

<style lang="scss" scoped>
// @yutiansut @quantaxis - 专业量化交易系统成交记录页面样式
$dark-bg-primary: #0d1117;
$dark-bg-secondary: #161b22;
$dark-bg-card: #1c2128;
$dark-bg-tertiary: #21262d;
$dark-border: #30363d;
$dark-text-primary: #f0f6fc;
$dark-text-secondary: #8b949e;
$primary-color: #1890ff;
$buy-color: #f5222d;
$sell-color: #52c41a;

.trades-page {
  min-height: 100%;
  background: $dark-bg-primary;
  padding: 0;

  // 主卡片
  ::v-deep > .el-card {
    background: $dark-bg-card;
    border: 1px solid $dark-border;
    border-radius: 12px;

    .el-card__header {
      background: $dark-bg-secondary;
      border-bottom: 1px solid $dark-border;
      padding: 16px 20px;
    }

    .el-card__body {
      padding: 20px;
    }
  }

  .card-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    color: $dark-text-primary;
    font-size: 16px;
    font-weight: 600;

    .el-button {
      background: linear-gradient(135deg, $primary-color 0%, #096dd9 100%);
      border: none;
      font-weight: 500;

      &:hover {
        opacity: 0.9;
        transform: translateY(-1px);
      }
    }
  }

  // 筛选表单
  ::v-deep .el-form {
    margin-bottom: 20px;
    padding: 16px;
    background: $dark-bg-secondary;
    border-radius: 8px;
    border: 1px solid $dark-border;

    .el-form-item {
      margin-bottom: 0;
      margin-right: 16px;

      .el-form-item__label {
        color: $dark-text-secondary;
        font-size: 13px;
      }
    }

    .el-input__inner,
    .el-select .el-input__inner {
      background: $dark-bg-tertiary;
      border: 1px solid $dark-border;
      color: $dark-text-primary;

      &:focus {
        border-color: $primary-color;
      }

      &::placeholder {
        color: $dark-text-secondary;
      }
    }

    .el-date-editor {
      .el-range-input {
        background: transparent;
        color: $dark-text-primary;

        &::placeholder {
          color: $dark-text-secondary;
        }
      }

      .el-range-separator {
        color: $dark-text-secondary;
      }
    }

    .el-button--primary {
      background: $primary-color;
      border-color: $primary-color;
    }

    .el-button--default {
      background: $dark-bg-tertiary;
      border-color: $dark-border;
      color: $dark-text-secondary;

      &:hover {
        border-color: $primary-color;
        color: $primary-color;
      }
    }
  }

  // 统计卡片区域
  .el-row {
    margin-bottom: 20px;
  }

  ::v-deep .el-col .el-card {
    background: $dark-bg-secondary;
    border: 1px solid $dark-border;
    border-radius: 10px;
    transition: all 0.3s ease;

    &:hover {
      border-color: rgba($primary-color, 0.5);
      transform: translateY(-2px);
      box-shadow: 0 8px 25px rgba(0, 0, 0, 0.3);
    }

    .el-card__body {
      padding: 20px;
    }
  }

  .stat-item {
    text-align: center;

    .stat-label {
      font-size: 13px;
      color: $dark-text-secondary;
      margin-bottom: 12px;
      text-transform: uppercase;
      letter-spacing: 0.5px;
    }

    .stat-value {
      font-size: 26px;
      font-weight: 700;
      color: $dark-text-primary;
      font-family: 'JetBrains Mono', monospace;
    }
  }

  // 表格样式
  ::v-deep .el-table {
    background: transparent;
    border-radius: 8px;
    overflow: hidden;

    &::before {
      display: none;
    }

    th.el-table__cell {
      background: $dark-bg-secondary;
      border-bottom: 1px solid $dark-border;
      color: $dark-text-secondary;
      font-weight: 600;
      font-size: 12px;
      text-transform: uppercase;
      letter-spacing: 0.5px;
      padding: 14px 0;
    }

    td.el-table__cell {
      background: $dark-bg-card;
      border-bottom: 1px solid $dark-border;
      color: $dark-text-primary;
      padding: 12px 0;
      font-family: 'JetBrains Mono', monospace;
      font-size: 13px;
    }

    tr:hover td.el-table__cell {
      background: $dark-bg-tertiary !important;
    }

    .el-table__body-wrapper {
      &::-webkit-scrollbar {
        width: 8px;
        height: 8px;
      }

      &::-webkit-scrollbar-track {
        background: $dark-bg-secondary;
      }

      &::-webkit-scrollbar-thumb {
        background: $dark-border;
        border-radius: 4px;

        &:hover {
          background: #484f58;
        }
      }
    }

    .el-table__fixed-right {
      background: transparent;

      th.el-table__cell,
      td.el-table__cell {
        background: $dark-bg-card;
      }
    }

    .el-table__empty-block {
      background: $dark-bg-card;
    }

    .el-table__empty-text {
      color: $dark-text-secondary;
    }
  }

  // 标签样式
  ::v-deep .el-tag {
    border: none;
    font-weight: 600;
    font-family: 'JetBrains Mono', monospace;
    font-size: 11px;

    &--danger {
      background: rgba($buy-color, 0.15);
      color: $buy-color;
    }

    &--success {
      background: rgba($sell-color, 0.15);
      color: $sell-color;
    }
  }

  // 操作按钮
  ::v-deep .el-button--text {
    color: $primary-color;
    font-size: 13px;

    &:hover {
      color: lighten($primary-color, 10%);
    }
  }
}

// 日期选择器弹出框
::v-deep .el-picker-panel {
  background: $dark-bg-card;
  border: 1px solid $dark-border;
  color: $dark-text-primary;

  .el-date-range-picker__content {
    .el-date-range-picker__header {
      color: $dark-text-primary;

      div {
        color: $dark-text-primary;
      }

      button {
        color: $dark-text-secondary;

        &:hover {
          color: $primary-color;
        }
      }
    }

    .el-date-table {
      th {
        color: $dark-text-secondary;
      }

      td {
        &.available {
          color: $dark-text-primary;

          &:hover {
            color: $primary-color;
          }
        }

        &.in-range {
          background: rgba($primary-color, 0.2);
        }

        &.start-date,
        &.end-date {
          .el-date-table-cell__text {
            background: $primary-color;
          }
        }

        &.today {
          .el-date-table-cell__text {
            color: $primary-color;
          }
        }
      }
    }
  }

  .el-date-range-picker__time-header {
    border-color: $dark-border;

    .el-input__inner {
      background: $dark-bg-tertiary;
      border-color: $dark-border;
      color: $dark-text-primary;
    }
  }
}

// 下拉菜单全局
::v-deep .el-select-dropdown {
  background: $dark-bg-card !important;
  border: 1px solid $dark-border !important;

  .el-select-dropdown__item {
    color: $dark-text-primary;

    &:hover {
      background: $dark-bg-tertiary;
    }

    &.selected {
      color: $primary-color;
    }
  }
}
</style>
