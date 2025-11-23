<template>
  <div class="account-detail" v-loading="loading">
    <div class="page-header">
      <el-button icon="el-icon-arrow-left" @click="$router.back()">返回</el-button>
      <h2>账户详情 - {{ accountId }}</h2>
    </div>

    <el-card shadow="hover" class="info-card" v-if="detail">
      <el-descriptions :column="2" border>
        <el-descriptions-item label="账户ID">{{ accountId }}</el-descriptions-item>
        <el-descriptions-item label="资金">{{ formatCurrency(detail.account_info.balance) }}</el-descriptions-item>
        <el-descriptions-item label="可用资金">{{ formatCurrency(detail.account_info.available) }}</el-descriptions-item>
        <el-descriptions-item label="占用保证金">{{ formatCurrency(detail.account_info.margin) }}</el-descriptions-item>
        <el-descriptions-item label="浮动盈亏">
          <span :class="detail.account_info.position_profit >= 0 ? 'profit' : 'loss'">
            {{ formatCurrency(detail.account_info.position_profit) }}
          </span>
        </el-descriptions-item>
        <el-descriptions-item label="风险率">
          {{ (detail.account_info.risk_ratio * 100).toFixed(2) }}%
        </el-descriptions-item>
      </el-descriptions>
    </el-card>

    <el-row :gutter="20" v-if="detail">
      <el-col :span="12">
        <el-card shadow="hover">
          <div slot="header">持仓</div>
          <el-table :data="detail.positions" height="300" size="mini" border>
            <el-table-column prop="instrument_id" label="合约" width="120" />
            <el-table-column prop="volume_long" label="多头" width="80" align="right" />
            <el-table-column prop="volume_short" label="空头" width="80" align="right" />
            <el-table-column prop="cost_long" label="多头均价" width="120" align="right" />
            <el-table-column prop="cost_short" label="空头均价" width="120" align="right" />
            <el-table-column prop="float_profit" label="浮动盈亏" width="120" align="right">
              <template slot-scope="{ row }">
                <span :class="row.float_profit >= 0 ? 'profit' : 'loss'">
                  {{ formatCurrency(row.float_profit) }}
                </span>
              </template>
            </el-table-column>
          </el-table>
        </el-card>
      </el-col>

      <el-col :span="12">
        <el-card shadow="hover">
          <div slot="header">订单</div>
          <el-table :data="detail.orders" height="300" size="mini" border>
            <el-table-column prop="order_id" label="订单号" width="150" />
            <el-table-column prop="instrument_id" label="合约" width="120" />
            <el-table-column prop="direction" label="方向" width="80" align="center" />
            <el-table-column prop="offset" label="开平" width="80" align="center" />
            <el-table-column prop="price" label="价格" width="100" align="right" />
            <el-table-column prop="volume" label="数量" width="80" align="right" />
            <el-table-column prop="status" label="状态" width="100" />
          </el-table>
        </el-card>
      </el-col>
    </el-row>
  </div>
</template>

<script>
import { getAccountDetail } from '@/api'

export default {
  name: 'AccountDetail',
  data() {
    return {
      loading: false,
      detail: null
    }
  },
  computed: {
    accountId() {
      return this.$route.params.accountId
    }
  },
  watch: {
    accountId() {
      this.fetchDetail()
    }
  },
  mounted() {
    this.fetchDetail()
  },
  methods: {
    async fetchDetail() {
      if (!this.accountId) return
      this.loading = true
      try {
        const data = await getAccountDetail(this.accountId)
        this.detail = {
          account_info: data.account_info || {},
          positions: data.positions || [],
          orders: data.orders || []
        }
      } catch (error) {
        this.$message.error('加载账户详情失败')
        console.error(error)
        this.detail = null
      } finally {
        this.loading = false
      }
    },
    formatCurrency(value) {
      return Number(value || 0).toLocaleString('zh-CN', { minimumFractionDigits: 2 })
    }
  }
}
</script>

<style scoped>
.account-detail {
  padding: 20px;
}

.page-header {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 20px;
}

.info-card {
  margin-bottom: 20px;
}

.profit {
  color: #F56C6C;
}

.loss {
  color: #67C23A;
}
</style>
