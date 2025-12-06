<template>
  <div class="market-overview">
    <el-row :gutter="20" style="margin-bottom: 20px;">
      <!-- 统计卡片 -->
      <el-col :span="6">
        <el-card shadow="hover">
          <div class="stat-card">
            <i class="el-icon-user" style="color: #409EFF;"></i>
            <div class="stat-content">
              <div class="stat-label">总账户数</div>
              <div class="stat-value">{{ totalAccounts }}</div>
            </div>
          </div>
        </el-card>
      </el-col>
      <el-col :span="6">
        <el-card shadow="hover">
          <div class="stat-card">
            <i class="el-icon-document" style="color: #E6A23C;"></i>
            <div class="stat-content">
              <div class="stat-label">活跃订单</div>
              <div class="stat-value">{{ totalOrders }}</div>
            </div>
          </div>
        </el-card>
      </el-col>
      <el-col :span="6">
        <el-card shadow="hover">
          <div class="stat-card">
            <i class="el-icon-money" style="color: #67C23A;"></i>
            <div class="stat-content">
              <div class="stat-label">总资金</div>
              <div class="stat-value">{{ formatNumber(totalBalance) }}</div>
            </div>
          </div>
        </el-card>
      </el-col>
      <el-col :span="6">
        <el-card shadow="hover">
          <div class="stat-card">
            <i class="el-icon-warning" style="color: #F56C6C;"></i>
            <div class="stat-content">
              <div class="stat-label">高风险账户</div>
              <div class="stat-value" style="color: #F56C6C;">{{ highRiskCount }}</div>
            </div>
          </div>
        </el-card>
      </el-col>
    </el-row>

    <!-- 主内容区域 -->
    <el-tabs v-model="activeTab" type="border-card">
      <!-- 账户列表标签页 -->
      <el-tab-pane label="账户列表" name="accounts">
        <div style="margin-bottom: 15px;">
          <el-button type="primary" size="small" icon="el-icon-refresh" @click="loadAccounts" :loading="accountsLoading">
            刷新
          </el-button>
          <el-input
            v-model="accountSearch"
            placeholder="搜索账户ID或名称"
            size="small"
            style="width: 300px; margin-left: 10px;"
            clearable
          >
            <i slot="prefix" class="el-input__icon el-icon-search"></i>
          </el-input>
        </div>

        <el-table
          :data="filteredAccounts"
          border
          stripe
          height="600"
          v-loading="accountsLoading"
          :default-sort="{prop: 'risk_ratio', order: 'descending'}"
        >
          <el-table-column prop="user_id" label="账户ID" width="280" show-overflow-tooltip />
          <el-table-column prop="user_name" label="账户名称" width="180" />
          <el-table-column prop="balance" label="余额" width="140" align="right" sortable>
            <template slot-scope="scope">
              {{ formatNumber(scope.row.balance) }}
            </template>
          </el-table-column>
          <el-table-column prop="available" label="可用资金" width="140" align="right" sortable>
            <template slot-scope="scope">
              {{ formatNumber(scope.row.available) }}
            </template>
          </el-table-column>
          <el-table-column prop="margin_used" label="占用保证金" width="140" align="right" sortable>
            <template slot-scope="scope">
              {{ formatNumber(scope.row.margin_used) }}
            </template>
          </el-table-column>
          <el-table-column prop="risk_ratio" label="风险率" width="120" align="center" sortable>
            <template slot-scope="scope">
              <el-tag :type="getRiskType(scope.row.risk_ratio)" size="small">
                {{ (scope.row.risk_ratio * 100).toFixed(2) }}%
              </el-tag>
            </template>
          </el-table-column>
          <el-table-column label="操作" width="200" fixed="right">
            <template slot-scope="scope">
              <el-button type="text" size="small" @click="viewAccountDetail(scope.row)">
                详情
              </el-button>
              <el-button type="text" size="small" @click="viewAccountOrders(scope.row)">
                订单
              </el-button>
            </template>
          </el-table-column>
        </el-table>

        <el-pagination
          v-if="accountsTotal > accountsPageSize"
          @current-change="handleAccountsPageChange"
          :current-page="accountsPage"
          :page-size="accountsPageSize"
          layout="total, prev, pager, next"
          :total="accountsTotal"
          style="margin-top: 20px; text-align: center;"
        />
      </el-tab-pane>

      <!-- 订单列表标签页 -->
      <el-tab-pane label="活跃订单" name="orders">
        <div style="margin-bottom: 15px;">
          <el-button type="primary" size="small" icon="el-icon-refresh" @click="loadAllOrders" :loading="ordersLoading">
            刷新
          </el-button>
          <el-select
            v-model="orderStatusFilter"
            placeholder="订单状态"
            size="small"
            style="width: 150px; margin-left: 10px;"
            clearable
            @change="loadAllOrders"
          >
            <el-option label="全部" value="" />
            <el-option label="已提交" value="Submitted" />
            <el-option label="部分成交" value="PartiallyFilled" />
            <el-option label="已成交" value="Filled" />
            <el-option label="已撤销" value="Cancelled" />
            <el-option label="已拒绝" value="Rejected" />
          </el-select>
          <el-input
            v-model="orderSearch"
            placeholder="搜索订单ID或合约"
            size="small"
            style="width: 300px; margin-left: 10px;"
            clearable
          >
            <i slot="prefix" class="el-input__icon el-icon-search"></i>
          </el-input>
        </div>

        <el-table
          :data="filteredOrders"
          border
          stripe
          height="600"
          v-loading="ordersLoading"
        >
          <el-table-column prop="order_id" label="订单ID" width="200" show-overflow-tooltip />
          <el-table-column prop="user_id" label="账户ID" width="200" show-overflow-tooltip>
            <template slot-scope="scope">
              {{ getAccountName(scope.row.user_id) }}
            </template>
          </el-table-column>
          <el-table-column prop="instrument_id" label="合约" width="100" />
          <el-table-column prop="direction" label="方向" width="70" align="center">
            <template slot-scope="scope">
              <el-tag :type="scope.row.direction === 'BUY' ? 'danger' : 'success'" size="mini">
                {{ scope.row.direction === 'BUY' ? '买' : '卖' }}
              </el-tag>
            </template>
          </el-table-column>
          <el-table-column prop="offset" label="开平" width="70" align="center">
            <template slot-scope="scope">
              {{ scope.row.offset === 'OPEN' ? '开' : '平' }}
            </template>
          </el-table-column>
          <el-table-column prop="price" label="价格" width="100" align="right">
            <template slot-scope="scope">
              {{ scope.row.price.toFixed(2) }}
            </template>
          </el-table-column>
          <el-table-column prop="volume" label="数量" width="80" align="right" />
          <el-table-column prop="filled_volume" label="成交" width="80" align="right" />
          <el-table-column prop="status" label="状态" width="100" align="center">
            <template slot-scope="scope">
              <el-tag :type="getOrderStatusType(scope.row.status)" size="mini">
                {{ getOrderStatusText(scope.row.status) }}
              </el-tag>
            </template>
          </el-table-column>
          <el-table-column prop="submit_time" label="提交时间" width="160">
            <template slot-scope="scope">
              {{ formatTimestamp(scope.row.submit_time) }}
            </template>
          </el-table-column>
        </el-table>
      </el-tab-pane>

      <!-- 实时监控标签页 -->
      <el-tab-pane label="实时监控" name="realtime">
        <div class="realtime-monitor">
          <el-row :gutter="20">
            <el-col :span="12">
              <el-card shadow="hover">
                <div slot="header">
                  <span>订单流监控</span>
                  <el-tag size="mini" style="margin-left: 10px;">
                    实时更新
                  </el-tag>
                </div>
                <div class="order-flow-chart" style="height: 400px;">
                  <div v-for="(order, index) in recentOrders" :key="index" class="order-flow-item">
                    <div class="order-time">{{ formatTime(order.submit_time) }}</div>
                    <div class="order-info">
                      <el-tag :type="order.direction === 'BUY' ? 'danger' : 'success'" size="mini">
                        {{ order.direction }}
                      </el-tag>
                      <span style="margin: 0 10px;">{{ order.instrument_id }}</span>
                      <span>{{ order.volume }}手 @ {{ order.price.toFixed(2) }}</span>
                    </div>
                    <div class="order-account">{{ getAccountNameShort(order.user_id) }}</div>
                  </div>
                </div>
              </el-card>
            </el-col>
            <el-col :span="12">
              <el-card shadow="hover">
                <div slot="header">
                  <span>风险监控</span>
                </div>
                <div style="height: 400px; overflow-y: auto;">
                  <el-table
                    :data="highRiskAccounts"
                    border
                    size="small"
                    max-height="380"
                  >
                    <el-table-column prop="user_name" label="账户" width="150" />
                    <el-table-column prop="risk_ratio" label="风险率" align="center">
                      <template slot-scope="scope">
                        <el-tag type="danger" size="mini">
                          {{ (scope.row.risk_ratio * 100).toFixed(2) }}%
                        </el-tag>
                      </template>
                    </el-table-column>
                    <el-table-column prop="margin_used" label="保证金占用" align="right">
                      <template slot-scope="scope">
                        {{ formatNumber(scope.row.margin_used) }}
                      </template>
                    </el-table-column>
                  </el-table>
                </div>
              </el-card>
            </el-col>
          </el-row>
        </div>
      </el-tab-pane>
    </el-tabs>
  </div>
</template>

<script>
// @yutiansut @quantaxis - 使用高效的全市场 API
import { listAllAccounts, listAllOrders } from '@/api'

export default {
  name: 'MarketOverview',
  data() {
    return {
      activeTab: 'accounts',
      // 账户相关
      accounts: [],
      accountsLoading: false,
      accountsPage: 1,
      accountsPageSize: 20,
      accountsTotal: 0,
      accountSearch: '',
      // 订单相关
      allOrders: [],
      ordersLoading: false,
      orderStatusFilter: '',
      orderSearch: '',
      // 实时监控
      recentOrders: [],
      // 自动刷新
      refreshTimer: null
    }
  },
  computed: {
    filteredAccounts() {
      if (!this.accountSearch) {
        return this.accounts
      }
      const search = this.accountSearch.toLowerCase()
      return this.accounts.filter(acc =>
        acc.user_id.toLowerCase().includes(search) ||
        acc.user_name.toLowerCase().includes(search)
      )
    },
    filteredOrders() {
      let orders = this.allOrders

      if (this.orderStatusFilter) {
        orders = orders.filter(o => o.status === this.orderStatusFilter)
      }

      if (this.orderSearch) {
        const search = this.orderSearch.toLowerCase()
        orders = orders.filter(o =>
          o.order_id.toLowerCase().includes(search) ||
          o.instrument_id.toLowerCase().includes(search) ||
          o.user_id.toLowerCase().includes(search)
        )
      }

      return orders
    },
    totalAccounts() {
      return this.accounts.length
    },
    // 活跃订单数 (Submitted/PartiallyFilled/PendingRoute/PendingRisk)
    totalOrders() {
      return this.allOrders.filter(o =>
        o.status === 'Submitted' ||
        o.status === 'PartiallyFilled' ||
        o.status === 'PendingRoute' ||
        o.status === 'PendingRisk'
      ).length
    },
    totalBalance() {
      return this.accounts.reduce((sum, acc) => sum + acc.balance, 0)
    },
    highRiskCount() {
      return this.accounts.filter(acc => acc.risk_ratio > 0.8).length
    },
    highRiskAccounts() {
      return this.accounts
        .filter(acc => acc.risk_ratio > 0.7)
        .sort((a, b) => b.risk_ratio - a.risk_ratio)
        .slice(0, 10)
    }
  },
  mounted() {
    this.loadAccounts()
    this.loadAllOrders()
    // 每10秒自动刷新
    this.refreshTimer = setInterval(() => {
      this.loadAccounts()
      this.loadAllOrders()
    }, 10000)
  },
  beforeDestroy() {
    if (this.refreshTimer) {
      clearInterval(this.refreshTimer)
    }
  },
  methods: {
    async loadAccounts() {
      this.accountsLoading = true
      try {
        const response = await listAllAccounts({
          page: this.accountsPage,
          page_size: this.accountsPageSize
        })
        this.accounts = response.accounts || []
        this.accountsTotal = response.total || 0
      } catch (error) {
        this.$message.error('加载账户列表失败: ' + (error.message || '未知错误'))
      } finally {
        this.accountsLoading = false
      }
    },

    // @yutiansut @quantaxis - 使用高效的全市场订单 API (单次请求)
    async loadAllOrders() {
      this.ordersLoading = true
      try {
        // 单次 API 调用获取所有订单（后端支持分页和过滤）
        const params = {
          page: 1,
          page_size: 200  // 获取最近200条订单
        }
        if (this.orderStatusFilter) {
          params.status = this.orderStatusFilter
        }

        const response = await listAllOrders(params)
        this.allOrders = response.orders || []

        // 更新最近订单（用于实时监控）
        this.recentOrders = this.allOrders
          .filter(o => o.status === 'Submitted' || o.status === 'PartiallyFilled' || o.status === 'PendingRoute')
          .sort((a, b) => b.submit_time - a.submit_time)
          .slice(0, 20)
      } catch (error) {
        console.error('加载订单失败:', error)
        this.$message.error('加载订单失败: ' + (error.message || '未知错误'))
      } finally {
        this.ordersLoading = false
      }
    },

    handleAccountsPageChange(page) {
      this.accountsPage = page
      this.loadAccounts()
    },

    getAccountName(accountId) {
      const account = this.accounts.find(acc => acc.user_id === accountId)
      return account ? `${account.user_name} (${accountId.slice(0, 8)}...)` : accountId
    },

    getAccountNameShort(accountId) {
      const account = this.accounts.find(acc => acc.user_id === accountId)
      return account ? account.user_name : accountId.slice(0, 12) + '...'
    },

    getRiskType(ratio) {
      if (ratio < 0.5) return 'success'
      if (ratio < 0.7) return 'warning'
      return 'danger'
    },

    // @yutiansut @quantaxis - 后端 OrderStatus 枚举映射
    getOrderStatusType(status) {
      const map = {
        'PendingRisk': 'warning',
        'PendingRoute': 'warning',
        'Submitted': 'primary',
        'PartiallyFilled': 'info',
        'Filled': 'success',
        'Cancelled': 'info',
        'Rejected': 'danger'
      }
      return map[status] || 'info'
    },

    getOrderStatusText(status) {
      const map = {
        'PendingRisk': '风控审核中',
        'PendingRoute': '等待路由',
        'Submitted': '已提交',
        'PartiallyFilled': '部分成交',
        'Filled': '已成交',
        'Cancelled': '已撤销',
        'Rejected': '已拒绝'
      }
      return map[status] || status
    },

    formatNumber(value) {
      if (value === null || value === undefined) return '0'
      return value.toLocaleString('zh-CN', {
        minimumFractionDigits: 2,
        maximumFractionDigits: 2
      })
    },

    formatTimestamp(timestamp) {
      if (!timestamp) return '-'
      const date = new Date(timestamp / 1000000) // 纳秒转毫秒
      return date.toLocaleString('zh-CN', {
        year: 'numeric',
        month: '2-digit',
        day: '2-digit',
        hour: '2-digit',
        minute: '2-digit',
        second: '2-digit'
      })
    },

    formatTime(timestamp) {
      if (!timestamp) return '-'
      const date = new Date(timestamp / 1000000)
      return date.toLocaleTimeString('zh-CN')
    },

    viewAccountDetail(account) {
      this.$router.push(`/account/${account.user_id}`)
    },

    viewAccountOrders(account) {
      this.activeTab = 'orders'
      this.orderSearch = account.user_id
    }
  }
}
</script>

<style lang="scss" scoped>
.market-overview {
  padding: 20px;

  .stat-card {
    display: flex;
    align-items: center;
    gap: 15px;

    i {
      font-size: 36px;
    }

    .stat-content {
      flex: 1;

      .stat-label {
        font-size: 14px;
        color: #909399;
        margin-bottom: 8px;
      }

      .stat-value {
        font-size: 24px;
        font-weight: bold;
        color: #303133;
      }
    }
  }

  .order-flow-chart {
    overflow-y: auto;

    .order-flow-item {
      display: flex;
      align-items: center;
      padding: 10px;
      border-bottom: 1px solid #EBEEF5;
      gap: 15px;

      &:hover {
        background-color: #F5F7FA;
      }

      .order-time {
        width: 80px;
        color: #909399;
        font-size: 12px;
      }

      .order-info {
        flex: 1;
        display: flex;
        align-items: center;
      }

      .order-account {
        width: 150px;
        text-align: right;
        color: #606266;
        font-size: 13px;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
      }
    }
  }
}
</style>
