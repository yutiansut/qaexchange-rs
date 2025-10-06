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
        <el-table-column prop="offset" label="开平" width="80" align="center">
          <template slot-scope="scope">
            {{ scope.row.offset === 'OPEN' ? '开仓' : '平仓' }}
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
        this.tradeList = trades.map(trade => {
          const isBuyer = trade.buy_user_id === this.currentUser
          const multiplier = 300

          return {
            trade_id: trade.trade_id,
            order_id: isBuyer ? trade.buy_order_id : trade.sell_order_id,
            instrument_id: trade.instrument_id,
            direction: isBuyer ? 'BUY' : 'SELL',
            offset: 'UNKNOWN', // Backend doesn't provide this, would need order details
            price: trade.price,
            volume: trade.volume,
            trade_amount: trade.price * trade.volume * multiplier,
            commission: trade.price * trade.volume * multiplier * 0.0001,
            trade_time: new Date(trade.timestamp / 1000000).toLocaleString('zh-CN'),
            buy_user_id: trade.buy_user_id,
            sell_user_id: trade.sell_user_id
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
.trades-page {
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
    }
  }
}
</style>
