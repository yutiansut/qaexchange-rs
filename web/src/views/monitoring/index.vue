<template>
  <div class="monitoring-container">
    <div class="monitoring-header">
      <h2>系统监控</h2>
      <el-button @click="loadMonitoringData" icon="el-icon-refresh" size="small">刷新</el-button>
    </div>

    <div class="monitoring-content" v-loading="loading">
      <!-- 系统状态卡片 -->
      <el-row :gutter="20" class="status-cards">
        <el-col :span="6">
          <el-card class="status-card">
            <div class="status-item">
              <div class="status-icon running">
                <i class="el-icon-user"></i>
              </div>
              <div class="status-info">
                <h3>总账户数</h3>
                <p class="status-value">{{ monitoringData.accounts ? monitoringData.accounts.total_count : 0 }}</p>
                <p class="status-sub">活跃: {{ monitoringData.accounts ? monitoringData.accounts.active_count : 0 }}</p>
              </div>
            </div>
          </el-card>
        </el-col>
        <el-col :span="6">
          <el-card class="status-card">
            <div class="status-item">
              <div class="status-icon running">
                <i class="el-icon-document"></i>
              </div>
              <div class="status-info">
                <h3>总订单数</h3>
                <p class="status-value">{{ monitoringData.orders ? monitoringData.orders.total_count : 0 }}</p>
                <p class="status-sub">待成交: {{ monitoringData.orders ? monitoringData.orders.pending_count : 0 }}</p>
              </div>
            </div>
          </el-card>
        </el-col>
        <el-col :span="6">
          <el-card class="status-card">
            <div class="status-item">
              <div class="status-icon success">
                <i class="el-icon-check"></i>
              </div>
              <div class="status-info">
                <h3>总成交笔数</h3>
                <p class="status-value">{{ monitoringData.trades ? monitoringData.trades.total_count : 0 }}</p>
                <p class="status-sub">金额: ¥{{ formatNumber(monitoringData.trades ? monitoringData.trades.total_amount : 0) }}</p>
              </div>
            </div>
          </el-card>
        </el-col>
        <el-col :span="6">
          <el-card class="status-card">
            <div class="status-item">
              <div class="status-icon info">
                <i class="el-icon-database"></i>
              </div>
              <div class="status-info">
                <h3>存储记录数</h3>
                <p class="status-value">{{ monitoringData.storage && monitoringData.storage.oltp ? monitoringData.storage.oltp.total_records : 0 }}</p>
                <p class="status-sub">批次: {{ monitoringData.storage && monitoringData.storage.oltp ? monitoringData.storage.oltp.total_batches : 0 }}</p>
              </div>
            </div>
          </el-card>
        </el-col>
      </el-row>

      <!-- 详细统计 -->
      <el-row :gutter="20" style="margin-top: 20px;">
        <el-col :span="12">
          <el-card>
            <div slot="header">
              <span>资金统计</span>
            </div>
            <div class="stats-content">
              <div class="stat-item">
                <span class="stat-label">总权益:</span>
                <span class="stat-value primary">¥{{ formatNumber(monitoringData.accounts ? monitoringData.accounts.total_balance : 0) }}</span>
              </div>
              <div class="stat-item">
                <span class="stat-label">总可用资金:</span>
                <span class="stat-value success">¥{{ formatNumber(monitoringData.accounts ? monitoringData.accounts.total_available : 0) }}</span>
              </div>
              <div class="stat-item">
                <span class="stat-label">总保证金占用:</span>
                <span class="stat-value warning">¥{{ formatNumber(monitoringData.accounts ? monitoringData.accounts.total_margin_used : 0) }}</span>
              </div>
              <div class="stat-item">
                <span class="stat-label">保证金占用率:</span>
                <span class="stat-value" :class="getMarginRatioClass()">
                  {{ getMarginRatio() }}%
                </span>
              </div>
            </div>
          </el-card>
        </el-col>
        <el-col :span="12">
          <el-card>
            <div slot="header">
              <span>订单统计</span>
            </div>
            <div class="stats-content">
              <div class="stat-item">
                <span class="stat-label">待成交订单:</span>
                <span class="stat-value warning">{{ monitoringData.orders ? monitoringData.orders.pending_count : 0 }}</span>
              </div>
              <div class="stat-item">
                <span class="stat-label">完全成交订单:</span>
                <span class="stat-value success">{{ monitoringData.orders ? monitoringData.orders.filled_count : 0 }}</span>
              </div>
              <div class="stat-item">
                <span class="stat-label">已撤销订单:</span>
                <span class="stat-value info">{{ monitoringData.orders ? monitoringData.orders.cancelled_count : 0 }}</span>
              </div>
              <div class="stat-item">
                <span class="stat-label">总成交量:</span>
                <span class="stat-value primary">{{ formatNumber(monitoringData.trades ? monitoringData.trades.total_volume : 0) }}</span>
              </div>
            </div>
          </el-card>
        </el-col>
      </el-row>

      <!-- OLAP 转换统计 -->
      <el-row :gutter="20" style="margin-top: 20px;">
        <el-col :span="24">
          <el-card>
            <div slot="header">
              <span>OLAP 转换系统</span>
            </div>
            <div class="stats-content">
              <el-row :gutter="20">
                <el-col :span="6">
                  <div class="stat-item">
                    <span class="stat-label">总任务数:</span>
                    <span class="stat-value">{{ monitoringData.storage && monitoringData.storage.olap ? monitoringData.storage.olap.total_tasks : 0 }}</span>
                  </div>
                </el-col>
                <el-col :span="6">
                  <div class="stat-item">
                    <span class="stat-label">待转换:</span>
                    <span class="stat-value warning">{{ monitoringData.storage && monitoringData.storage.olap ? monitoringData.storage.olap.pending_tasks : 0 }}</span>
                  </div>
                </el-col>
                <el-col :span="6">
                  <div class="stat-item">
                    <span class="stat-label">成功:</span>
                    <span class="stat-value success">{{ monitoringData.storage && monitoringData.storage.olap ? monitoringData.storage.olap.success_tasks : 0 }}</span>
                  </div>
                </el-col>
                <el-col :span="6">
                  <div class="stat-item">
                    <span class="stat-label">失败:</span>
                    <span class="stat-value danger">{{ monitoringData.storage && monitoringData.storage.olap ? monitoringData.storage.olap.failed_tasks : 0 }}</span>
                  </div>
                </el-col>
              </el-row>
            </div>
          </el-card>
        </el-col>
      </el-row>
    </div>
  </div>
</template>

<script>
import { getSystemMonitoring } from '@/api'

export default {
  name: 'Monitoring',
  data() {
    return {
      loading: false,
      monitoringData: {
        accounts: null,
        orders: null,
        trades: null,
        storage: null
      },
      refreshTimer: null
    }
  },
  mounted() {
    this.loadMonitoringData()
    // 每10秒自动刷新
    this.refreshTimer = setInterval(() => {
      this.loadMonitoringData()
    }, 10000)
  },
  beforeDestroy() {
    if (this.refreshTimer) {
      clearInterval(this.refreshTimer)
    }
  },
  methods: {
    async loadMonitoringData() {
      this.loading = true
      try {
        const response = await getSystemMonitoring()
        if (response.data) {
          this.monitoringData = response.data
        }
      } catch (error) {
        console.error('加载监控数据失败:', error)
        this.$message.error('加载监控数据失败')
      } finally {
        this.loading = false
      }
    },

    formatNumber(num) {
      if (!num) return '0.00'
      return parseFloat(num).toLocaleString('zh-CN', {
        minimumFractionDigits: 2,
        maximumFractionDigits: 2
      })
    },

    getMarginRatio() {
      if (!this.monitoringData.accounts) return '0.00'
      const { total_balance, total_margin_used } = this.monitoringData.accounts
      if (!total_balance || total_balance === 0) return '0.00'
      return ((total_margin_used / total_balance) * 100).toFixed(2)
    },

    getMarginRatioClass() {
      const ratio = parseFloat(this.getMarginRatio())
      if (ratio >= 80) return 'danger'
      if (ratio >= 60) return 'warning'
      return 'success'
    }
  }
}
</script>

<style scoped>
.monitoring-container {
  padding: 20px;
}

.monitoring-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 20px;
}

.monitoring-header h2 {
  margin: 0;
  color: #303133;
}

.status-cards {
  margin-bottom: 20px;
}

.status-card {
  height: 120px;
}

.status-item {
  display: flex;
  align-items: center;
  height: 100%;
}

.status-icon {
  width: 60px;
  height: 60px;
  border-radius: 50%;
  display: flex;
  align-items: center;
  justify-content: center;
  margin-right: 15px;
  font-size: 24px;
  color: white;
  flex-shrink: 0;
}

.status-icon.running {
  background-color: #409EFF;
}

.status-icon.success {
  background-color: #67C23A;
}

.status-icon.warning {
  background-color: #E6A23C;
}

.status-icon.info {
  background-color: #909399;
}

.status-info {
  flex: 1;
  min-width: 0;
}

.status-info h3 {
  margin: 0 0 8px 0;
  font-size: 14px;
  color: #606266;
  font-weight: normal;
}

.status-value {
  margin: 0 0 5px 0;
  font-size: 24px;
  font-weight: bold;
  color: #303133;
}

.status-sub {
  margin: 0;
  font-size: 12px;
  color: #909399;
}

.stats-content {
  padding: 10px 0;
}

.stat-item {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 15px;
  padding-bottom: 10px;
  border-bottom: 1px solid #f0f0f0;
}

.stat-item:last-child {
  border-bottom: none;
  margin-bottom: 0;
}

.stat-label {
  color: #606266;
  font-size: 14px;
}

.stat-value {
  color: #303133;
  font-weight: bold;
  font-size: 16px;
}

.stat-value.primary {
  color: #409EFF;
}

.stat-value.success {
  color: #67C23A;
}

.stat-value.warning {
  color: #E6A23C;
}

.stat-value.danger {
  color: #F56C6C;
}

.stat-value.info {
  color: #909399;
}
</style>
