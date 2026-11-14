<template>
  <div class="monitoring-container">
    <div class="monitoring-header">
      <h2>系统监控</h2>
      <el-button @click="loadMonitoringData" icon="el-icon-refresh" size="small">刷新</el-button>
    </div>

    <div class="monitoring-content" v-loading="loading">
      <!-- 系统状态卡片 -->
      <el-row :gutter="20" class="status-cards">
        <el-col :xs="24" :sm="12" :lg="6">
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
        <el-col :xs="24" :sm="12" :lg="6">
          <el-card class="status-card">
            <div class="status-item">
              <div class="status-icon running">
                <i class="el-icon-document"></i>
              </div>
              <div class="status-info">
                <!-- ⚠️ 口径:本数字来自 /api/monitoring/orders,统计的是各账户
                     QIFI dailyorders —— **含全部历史**(已撤、已成交都算)。
                     「全市场行情」页的「活跃订单」只含当前在册,
                     两者可以相差数倍,都不是错的。@yutiansut @quantaxis -->
                <h3>
                  总订单数（历史累计）
                  <el-tooltip
                    content="含已撤单与已成交的全部历史订单。当前在册数量请看「全市场行情」页"
                    placement="top"
                  >
                    <i class="el-icon-question" style="color:#909399;font-size:12px;cursor:help;"></i>
                  </el-tooltip>
                </h3>
                <p class="status-value">{{ monitoringData.orders ? monitoringData.orders.total_count : 0 }}</p>
                <p class="status-sub">待成交: {{ monitoringData.orders ? monitoringData.orders.pending_count : 0 }}</p>
              </div>
            </div>
          </el-card>
        </el-col>
        <el-col :xs="24" :sm="12" :lg="6">
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
        <el-col :xs="24" :sm="12" :lg="6">
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
        <el-col :xs="24" :md="12">
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
        <el-col :xs="24" :md="12">
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
                <el-col :xs="12" :sm="6">
                  <div class="stat-item">
                    <span class="stat-label">总任务数:</span>
                    <span class="stat-value">{{ monitoringData.storage && monitoringData.storage.olap ? monitoringData.storage.olap.total_tasks : 0 }}</span>
                  </div>
                </el-col>
                <el-col :xs="12" :sm="6">
                  <div class="stat-item">
                    <span class="stat-label">待转换:</span>
                    <span class="stat-value warning">{{ monitoringData.storage && monitoringData.storage.olap ? monitoringData.storage.olap.pending_tasks : 0 }}</span>
                  </div>
                </el-col>
                <el-col :xs="12" :sm="6">
                  <div class="stat-item">
                    <span class="stat-label">成功:</span>
                    <span class="stat-value success">{{ monitoringData.storage && monitoringData.storage.olap ? monitoringData.storage.olap.success_tasks : 0 }}</span>
                  </div>
                </el-col>
                <el-col :xs="12" :sm="6">
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
        // axios 拦截器已处理响应，直接返回数据 @yutiansut @quantaxis
        const data = await getSystemMonitoring()
        if (data && data.accounts) {
          this.monitoringData = data
        }
      } catch (error) {
        console.error('加载监控数据失败:', error)
        this.$message.error('加载监控数据失败')
      } finally {
        this.loading = false
      }
    },

    // ✨ 委托给全局 `$money`(main.js)—— 判空 → isFinite → **量级上限 1e15**。
    // @yutiansut @quantaxis
    //
    // 本页此前是自己写的格式化器,挡不住 `balance = 1e300` 的污染账户:
    // 子代理实测它在这里渲染成 404 个字符(或 `¥1e+300`)。
    // `Number.isFinite(1e300)` 为真,所以只判 isFinite 也不够,必须判量级。
    // 全站四个各写各的格式化器现已统一到同一个判据。
    formatNumber(num) {
      return this.$money(num)
    },

    getMarginRatio() {
      if (!this.monitoringData.accounts) return '--'
      const { total_balance, total_margin_used } = this.monitoringData.accounts
      // ✨ 与 $money 同口径兜底:total_balance 非有限或超 1e15(被 T1e300 的 1e300 吸污)时,
      // 不要算出误导性的 0.00%(896M 占用会显示成健康绿 0%)。@yutiansut @quantaxis
      const tb = Number(total_balance)
      if (!Number.isFinite(tb) || Math.abs(tb) >= 1e15 || tb === 0) return '--'
      const tm = Number(total_margin_used)
      if (!Number.isFinite(tm) || Math.abs(tm) >= 1e15) return '--'
      return ((tm / tb) * 100).toFixed(2)
    },

    getMarginRatioClass() {
      const ratio = parseFloat(this.getMarginRatio())
      if (!Number.isFinite(ratio)) return ''   // '--' 时不上任何颜色(不伪装成健康绿)
      if (ratio >= 80) return 'danger'
      if (ratio >= 60) return 'warning'
      return 'success'
    }
  }
}
</script>

<style scoped lang="scss">
@import '@/styles/variables.scss';
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
  color: $dark-text-primary;
}

.status-cards {
  margin-bottom: 20px;
}

.status-card {
  /* ⚠️ 不能用固定 height —— el-card 默认 overflow:hidden,内容一超就永久裁掉。
     实测 1280×800:卡片 clientH=118 而 body scrollH 达 149,副行
     「待成交: 778」「金额: ¥850,274,092.76」被拦腰切断,且祖先滚动无解。
     内容实际需要 125px(h3 21 + margin 8 + value 29 + margin 5 + sub 18 + padding 40),
     标题换行再 +21px。@yutiansut @quantaxis */
  min-height: 120px;
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
  color: #8b949e;  /* 浅色遗留 2.83:1 → 暗色 5.63:1 */
  font-weight: normal;
}

.status-value {
  margin: 0 0 5px 0;
  font-size: 24px;
  font-weight: bold;
  color: $dark-text-primary;
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
  border-bottom: 1px solid rgba(240, 246, 252, 0.08);  /* 浅色遗留 */
}

.stat-item:last-child {
  border-bottom: none;
  margin-bottom: 0;
}

.stat-label {
  color: #8b949e;  /* 浅色遗留 2.83:1 → 暗色 5.63:1 */
  font-size: 14px;
  /* ✨ 标签不参与压缩,也不许竖排。@yutiansut @quantaxis
     `.stat-item` 是 flex,长数字会把 `.stat-value` 撑到 3186px(实测),
     浏览器只好把标签压到 min-content —— 「总权益」竖成「总/权/益:」。 */
  flex: 0 0 auto;
  white-space: nowrap;
  margin-right: 12px;
}

.stat-value {
  /* 浅色主题遗留 —— 本站只有暗色(styles/index.scss:14-28 无条件设暗底),
     #303133 落在 #161b22 上对比度只有 1.33:1(AA 需 4.5),数字近乎隐形。
     模板 :149 的「总任务数」是全站唯一没带 .primary/.success 修饰类的
     裸 .stat-value,所以只有它中招。@yutiansut @quantaxis */
  color: #f0f6fc;
  font-weight: bold;
  font-size: 16px;
  /* ✨ flex 子项默认 `min-width:auto`,内容多长就撑多长 —— 不加这三行,
     一个超长数字会把本项撑到 3186px、横向压住右侧兄弟元素的文字,
     且**没有省略号、没有 tooltip、不可滚**,用户看到的是被静默切断的错数字。
     实测(1920 视口):值宽 3186.1px / 父 758px,溢出 2445.5px,
     并与右侧「待成交订单:」重叠 73.4×3px。
     `min-width:0` 解除 flex 的最小尺寸约束,余下两行给出可见的截断提示。
     @yutiansut @quantaxis */
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  text-align: right;
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
