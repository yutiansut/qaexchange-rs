<template>
  <div class="positions-page">
    <el-card>
      <div slot="header" class="card-header">
        <span>持仓管理</span>
        <el-button type="primary" size="small" @click="loadPositions">
          <i class="el-icon-refresh"></i> 刷新
        </el-button>
      </div>

      <el-row :gutter="20" style="margin-bottom: 20px">
        <el-col :span="6">
          <el-card shadow="hover">
            <div class="stat-item">
              <div class="stat-label">总持仓市值</div>
              <div class="stat-value">¥{{ formatNumber(summary.total_value) }}</div>
            </div>
          </el-card>
        </el-col>
        <el-col :span="6">
          <el-card shadow="hover">
            <div class="stat-item">
              <div class="stat-label">浮动盈亏</div>
              <div class="stat-value" :class="summary.total_profit >= 0 ? 'profit' : 'loss'">
                {{ summary.total_profit >= 0 ? '+' : '' }}¥{{ formatNumber(summary.total_profit) }}
              </div>
            </div>
          </el-card>
        </el-col>
        <el-col :span="6">
          <el-card shadow="hover">
            <div class="stat-item">
              <div class="stat-label">持仓品种数</div>
              <div class="stat-value">{{ positionList.length }}</div>
            </div>
          </el-card>
        </el-col>
        <el-col :span="6">
          <el-card shadow="hover">
            <div class="stat-item">
              <div class="stat-label">盈亏比</div>
              <div class="stat-value" :class="summary.profit_ratio >= 0 ? 'profit' : 'loss'">
                {{ (summary.profit_ratio * 100).toFixed(2) }}%
              </div>
            </div>
          </el-card>
        </el-col>
      </el-row>

      <el-table
        :data="positionList"
        border
        stripe
        height="500"
        :loading="loading"
        show-overflow
      >
        <el-table-column prop="instrument_id" label="合约代码" width="120" />
        <el-table-column prop="direction" label="方向" width="80" align="center">
          <template slot-scope="scope">
            <el-tag :type="scope.row.direction === 'LONG' ? 'danger' : 'success'" size="mini">
              {{ scope.row.direction === 'LONG' ? '多头' : '空头' }}
            </el-tag>
          </template>
        </el-table-column>
        <el-table-column prop="volume" label="持仓量" width="100" align="right" />
        <el-table-column prop="available" label="可平量" width="100" align="right" />
        <el-table-column prop="open_price" label="开仓均价" width="120" align="right">
          <template slot-scope="scope">
            {{ scope.row.open_price.toFixed(2) }}
          </template>
        </el-table-column>
        <el-table-column prop="last_price" label="最新价" width="120" align="right">
          <template slot-scope="scope">
            {{ scope.row.last_price.toFixed(2) }}
          </template>
        </el-table-column>
        <el-table-column prop="position_value" label="持仓市值" width="130" align="right">
          <template slot-scope="scope">
            ¥{{ formatNumber(scope.row.position_value) }}
          </template>
        </el-table-column>
        <el-table-column prop="profit" label="浮动盈亏" width="130" align="right">
          <template slot-scope="scope">
            <span :class="scope.row.profit >= 0 ? 'profit-text' : 'loss-text'">
              {{ scope.row.profit >= 0 ? '+' : '' }}¥{{ formatNumber(scope.row.profit) }}
            </span>
          </template>
        </el-table-column>
        <el-table-column prop="profit_ratio" label="盈亏比" width="100" align="right">
          <template slot-scope="scope">
            <span :class="scope.row.profit_ratio >= 0 ? 'profit-text' : 'loss-text'">
              {{ scope.row.profit_ratio >= 0 ? '+' : '' }}{{ (scope.row.profit_ratio * 100).toFixed(2) }}%
            </span>
          </template>
        </el-table-column>
        <el-table-column prop="margin" label="占用保证金" width="130" align="right">
          <template slot-scope="scope">
            ¥{{ formatNumber(scope.row.margin) }}
          </template>
        </el-table-column>
        <el-table-column label="操作" width="150" fixed="right">
          <template slot-scope="scope">
            <el-button
              type="text"
              size="small"
              @click="handleClosePosition(scope.row)"
            >
              平仓
            </el-button>
            <el-button type="text" size="small" @click="handleViewDetail(scope.row)">
              详情
            </el-button>
          </template>
        </el-table-column>
      </el-table>
    </el-card>

    <!-- 平仓对话框 -->
    <el-dialog label="平仓" :visible.sync="closeDialogVisible" width="400px">
      <el-form :model="closeForm" ref="closeForm" label-width="100px">
        <el-form-item label="合约">
          <el-input v-model="closeForm.instrument_id" disabled />
        </el-form-item>
        <el-form-item label="方向">
          <el-input v-model="closeForm.direction_text" disabled />
        </el-form-item>
        <el-form-item label="可平量">
          <el-input v-model="closeForm.available" disabled />
        </el-form-item>
        <el-form-item label="平仓量" prop="volume">
          <el-input-number
            v-model="closeForm.volume"
            :min="1"
            :max="closeForm.available"
            :step="1"
            style="width: 100%"
          />
        </el-form-item>
        <el-form-item label="平仓类型">
          <el-radio-group v-model="closeForm.close_type">
            <el-radio label="MARKET">市价平仓</el-radio>
            <el-radio label="LIMIT">限价平仓</el-radio>
          </el-radio-group>
        </el-form-item>
        <el-form-item label="限价" v-if="closeForm.close_type === 'LIMIT'">
          <el-input-number
            v-model="closeForm.price"
            :min="0"
            :step="0.2"
            :precision="1"
            style="width: 100%"
          />
        </el-form-item>
      </el-form>
      <div slot="footer">
        <el-button @click="closeDialogVisible = false">取消</el-button>
        <el-button type="primary" @click="handleSubmitClose" :loading="submitting">
          确定平仓
        </el-button>
      </div>
    </el-dialog>
  </div>
</template>

<script>
import { queryPosition, submitOrder, getTick } from '@/api'
import { mapGetters } from 'vuex'

export default {
  name: 'Positions',
  data() {
    return {
      loading: false,
      submitting: false,
      positionList: [],
      closeDialogVisible: false,
      closeForm: {
        account_id: '',  // 添加account_id字段
        instrument_id: '',
        direction: '',
        direction_text: '',
        available: 0,
        volume: 1,
        close_type: 'MARKET',
        price: 0
      },
      summary: {
        total_value: 0,
        total_profit: 0,
        profit_ratio: 0
      }
    }
  },
  computed: {
    ...mapGetters(['currentUser'])
  },
  watch: {
    currentUser() {
      this.loadPositions()
    }
  },
  mounted() {
    if (this.currentUser) {
      this.loadPositions()
    }
  },
  methods: {
    formatNumber(num) {
      return (num || 0).toFixed(2).replace(/\B(?=(\d{3})+(?!\d))/g, ',')
    },

    async loadPositions() {
      if (!this.currentUser) {
        this.$message.warning('请先登录')
        return
      }

      this.loading = true
      try {
        // queryPosition 通过 axios 拦截器已经返回 res.data，直接就是数组
        const rawPositions = await queryPosition(this.currentUser) || []

        const uniqueInstruments = [...new Set(rawPositions.map(pos => pos.instrument_id))]
        const priceMap = {}
        await Promise.all(uniqueInstruments.map(async instrument => {
          try {
            const tick = await getTick(instrument)
            priceMap[instrument] = tick.last_price || tick.bid_price || tick.ask_price || 0
          } catch (error) {
            priceMap[instrument] = 0
          }
        }))

        // 将后台返回的多空合并格式转换为前端需要的格式
        // 后台格式：{ instrument_id, volume_long, volume_short, cost_long, cost_short, profit_long, profit_short }
        // 前端格式：{ instrument_id, direction, volume, available, open_price, last_price, position_value, profit, profit_ratio, margin }
        const positions = []

        rawPositions.forEach(pos => {
          const lastPrice = priceMap[pos.instrument_id] || pos.cost_long || pos.cost_short || 0

          // 如果有多头持仓
          if (pos.volume_long > 0) {
            const positionValue = pos.volume_long * lastPrice * 300
            // @yutiansut @quantaxis: 可平量 = 持仓量 - 冻结量
            const frozenLong = pos.volume_long_frozen || 0
            positions.push({
              account_id: pos.account_id,  // 保存account_id（用于平仓）
              instrument_id: pos.instrument_id,
              direction: 'LONG',
              volume: pos.volume_long,
              available: Math.max(0, pos.volume_long - frozenLong), // 可平量 = 持仓 - 冻结
              frozen: frozenLong,
              open_price: pos.cost_long,
              last_price: lastPrice,
              position_value: positionValue,
              profit: pos.profit_long,
              profit_ratio: positionValue > 0 ? pos.profit_long / positionValue : 0,
              margin: pos.volume_long * pos.cost_long * 300 * 0.15 // 假设保证金率15%
            })
          }

          // 如果有空头持仓
          if (pos.volume_short > 0) {
            const positionValue = pos.volume_short * lastPrice * 300
            // @yutiansut @quantaxis: 可平量 = 持仓量 - 冻结量
            const frozenShort = pos.volume_short_frozen || 0
            positions.push({
              account_id: pos.account_id,  // 保存account_id（用于平仓）
              instrument_id: pos.instrument_id,
              direction: 'SHORT',
              volume: pos.volume_short,
              available: Math.max(0, pos.volume_short - frozenShort), // 可平量 = 持仓 - 冻结
              frozen: frozenShort,
              open_price: pos.cost_short,
              last_price: lastPrice,
              position_value: positionValue,
              profit: pos.profit_short,
              profit_ratio: positionValue > 0 ? pos.profit_short / positionValue : 0,
              margin: pos.volume_short * pos.cost_short * 300 * 0.15 // 假设保证金率15%
            })
          }
        })

        this.positionList = positions
        this.calculateSummary()
      } catch (error) {
        const errorMsg = (error.response && error.response.data && error.response.data.error && error.response.data.error.message) || error.message
        this.$message.error('加载持仓失败: ' + errorMsg)
        this.positionList = []
      } finally {
        this.loading = false
      }
    },

    calculateSummary() {
      this.summary.total_value = this.positionList.reduce((sum, p) => sum + p.position_value, 0)
      this.summary.total_profit = this.positionList.reduce((sum, p) => sum + p.profit, 0)

      const total_cost = this.positionList.reduce((sum, p) => {
        return sum + p.volume * p.open_price * 300
      }, 0)

      this.summary.profit_ratio = total_cost > 0 ? this.summary.total_profit / total_cost : 0
    },

    handleClosePosition(row) {
      this.closeForm = {
        account_id: row.account_id,  // 设置account_id
        instrument_id: row.instrument_id,
        direction: row.direction,
        direction_text: row.direction === 'LONG' ? '多头' : '空头',
        available: row.available,
        volume: row.available,
        close_type: 'MARKET',
        price: row.last_price
      }
      this.closeDialogVisible = true
    },

    handleSubmitClose() {
      this.submitting = true

      // ✨ 交易所模式：user_id 和 account_id 都使用账户ID @yutiansut @quantaxis
      // 原因：交易所只关心账户，User→Account映射是经纪商业务
      const orderData = {
        user_id: this.closeForm.account_id,     // 交易所模式：使用账户ID
        account_id: this.closeForm.account_id,
        instrument_id: this.closeForm.instrument_id,
        direction: this.closeForm.direction === 'LONG' ? 'SELL' : 'BUY',
        offset: 'CLOSE',
        price: this.closeForm.close_type === 'LIMIT' ? this.closeForm.price : 0,
        volume: this.closeForm.volume,
        order_type: this.closeForm.close_type
      }

      submitOrder(orderData)
        .then(() => {
          this.$message.success('平仓委托已提交')
          this.closeDialogVisible = false
          this.loadPositions()
        })
        .catch(() => {
          this.$message.error('平仓失败')
        })
        .finally(() => {
          this.submitting = false
        })
    },

    handleViewDetail(row) {
      this.$alert(JSON.stringify(row, null, 2), '持仓详情', {
        confirmButtonText: '确定'
      })
    }
  }
}
</script>

<style lang="scss" scoped>
// @yutiansut @quantaxis - 专业量化交易系统持仓页面样式
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

.positions-page {
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

      &.profit {
        color: $buy-color;
        text-shadow: 0 0 20px rgba($buy-color, 0.3);
      }

      &.loss {
        color: $sell-color;
        text-shadow: 0 0 20px rgba($sell-color, 0.3);
      }
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

  // 盈亏文字
  .profit-text {
    color: $buy-color;
    font-weight: 600;
    font-family: 'JetBrains Mono', monospace;
    text-shadow: 0 0 10px rgba($buy-color, 0.2);
  }

  .loss-text {
    color: $sell-color;
    font-weight: 600;
    font-family: 'JetBrains Mono', monospace;
    text-shadow: 0 0 10px rgba($sell-color, 0.2);
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

// 平仓对话框
::v-deep .el-dialog {
  background: $dark-bg-card;
  border-radius: 12px;
  border: 1px solid $dark-border;

  .el-dialog__header {
    background: $dark-bg-secondary;
    border-bottom: 1px solid $dark-border;
    padding: 16px 20px;
    border-radius: 12px 12px 0 0;

    .el-dialog__title {
      color: $dark-text-primary;
      font-weight: 600;
    }

    .el-dialog__headerbtn .el-dialog__close {
      color: $dark-text-secondary;

      &:hover {
        color: $dark-text-primary;
      }
    }
  }

  .el-dialog__body {
    padding: 24px;
    background: $dark-bg-card;
  }

  .el-dialog__footer {
    background: $dark-bg-secondary;
    border-top: 1px solid $dark-border;
    padding: 16px 20px;
    border-radius: 0 0 12px 12px;

    .el-button--default {
      background: $dark-bg-tertiary;
      border-color: $dark-border;
      color: $dark-text-secondary;

      &:hover {
        border-color: $primary-color;
        color: $primary-color;
      }
    }

    .el-button--primary {
      background: linear-gradient(135deg, $buy-color 0%, darken($buy-color, 10%) 100%);
      border: none;
    }
  }

  .el-form-item__label {
    color: $dark-text-secondary;
  }

  .el-input__inner {
    background: $dark-bg-tertiary;
    border: 1px solid $dark-border;
    color: $dark-text-primary;

    &:disabled {
      background: $dark-bg-secondary;
      color: $dark-text-secondary;
    }
  }

  .el-input-number {
    .el-input__inner {
      background: $dark-bg-tertiary;
      border: 1px solid $dark-border;
      color: $dark-text-primary;
    }

    .el-input-number__decrease,
    .el-input-number__increase {
      background: $dark-bg-secondary;
      border-color: $dark-border;
      color: $dark-text-secondary;

      &:hover {
        color: $primary-color;
      }
    }
  }

  .el-radio {
    color: $dark-text-secondary;

    &.is-checked .el-radio__label {
      color: $primary-color;
    }

    .el-radio__inner {
      background: $dark-bg-tertiary;
      border-color: $dark-border;
    }

    &.is-checked .el-radio__inner {
      background: $primary-color;
      border-color: $primary-color;
    }
  }
}
</style>
