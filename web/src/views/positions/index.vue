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
import { queryPosition, submitOrder } from '@/api'
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

        // 将后台返回的多空合并格式转换为前端需要的格式
        // 后台格式：{ instrument_id, volume_long, volume_short, cost_long, cost_short, profit_long, profit_short }
        // 前端格式：{ instrument_id, direction, volume, available, open_price, last_price, position_value, profit, profit_ratio, margin }
        const positions = []

        rawPositions.forEach(pos => {
          // 如果有多头持仓
          if (pos.volume_long > 0) {
            positions.push({
              instrument_id: pos.instrument_id,
              direction: 'LONG',
              volume: pos.volume_long,
              available: pos.volume_long, // 假设全部可平
              open_price: pos.cost_long,
              last_price: pos.cost_long, // TODO: 需要从行情获取最新价
              position_value: pos.volume_long * pos.cost_long * 300, // 合约乘数300
              profit: pos.profit_long,
              profit_ratio: pos.cost_long > 0 ? pos.profit_long / (pos.volume_long * pos.cost_long * 300) : 0,
              margin: pos.volume_long * pos.cost_long * 300 * 0.15 // 假设保证金率15%
            })
          }

          // 如果有空头持仓
          if (pos.volume_short > 0) {
            positions.push({
              instrument_id: pos.instrument_id,
              direction: 'SHORT',
              volume: pos.volume_short,
              available: pos.volume_short, // 假设全部可平
              open_price: pos.cost_short,
              last_price: pos.cost_short, // TODO: 需要从行情获取最新价
              position_value: pos.volume_short * pos.cost_short * 300, // 合约乘数300
              profit: pos.profit_short,
              profit_ratio: pos.cost_short > 0 ? pos.profit_short / (pos.volume_short * pos.cost_short * 300) : 0,
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

      const orderData = {
        user_id: this.currentUser,
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
.positions-page {
  padding: 20px;

  .card-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .stat-item {
    text-align: center;

    .stat-label {
      font-size: 14px;
      color: #909399;
      margin-bottom: 10px;
    }

    .stat-value {
      font-size: 24px;
      font-weight: 600;
      color: #303133;

      &.profit {
        color: #f56c6c;
      }

      &.loss {
        color: #67c23a;
      }
    }
  }

  .profit-text {
    color: #f56c6c;
    font-weight: 600;
  }

  .loss-text {
    color: #67c23a;
    font-weight: 600;
  }
}
</style>
