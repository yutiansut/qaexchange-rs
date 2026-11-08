<template>
  <div class="commission-container">
    <!-- 页面标题 -->
    <div class="page-header">
      <h2>手续费查询</h2>
      <div class="header-actions">
        <el-button icon="el-icon-refresh" @click="loadData">刷新</el-button>
      </div>
    </div>

    <!-- 账户选择和统计 -->
    <el-card class="stats-card">
      <div slot="header">
        <span>手续费统计</span>
      </div>
      <!-- ⚠️ el-form-item 必须有 el-form 祖先:ElFormItem 会 inject('elForm'),
           拿不到就在 render 里读 undefined.$options 抛错,整块卡片渲染失败
           (`Injection "elForm" not found` / `Cannot read properties of undefined`)。
           @yutiansut @quantaxis -->
      <el-form :inline="true" @submit.native.prevent>
      <el-row :gutter="20">
        <el-col :xs="24" :sm="12" :md="8" :lg="6">
          <el-form-item label="选择账户">
            <!-- 200px 装不下 `ACC_` + 32 位 hex(实测需 305px,硬切 107px 且**无省略号**),
                 用户无法分辨当前选的是哪个账户。
                 ⚠️ 必须用**定值**:本表单是 `<el-form :inline="true">`,
                 `.el-form-item` 与 `.el-form-item__content` 都是 inline-block
                 (shrink-to-fit),`width:100%` 只等于父级收缩后的 196px,
                 `max-width` 永远不触发 —— 实测比改前还窄 4px。
                 @yutiansut @quantaxis -->
            <el-select v-model="selectedAccountId" placeholder="请选择账户" style="width: 380px" @change="loadStatistics">
              <el-option
                v-for="account in accounts"
                :key="account.account_id"
                :label="account.account_id"
                :value="account.account_id"
              ></el-option>
            </el-select>
          </el-form-item>
        </el-col>
      </el-row>
      </el-form>
      <el-row :gutter="20" v-if="statistics">
        <el-col :xs="24" :sm="12" :lg="8">
          <div class="stat-item">
            <div class="stat-label">累计手续费</div>
            <div class="stat-value danger">{{ $num(statistics.total_commission, 2) }}</div>
          </div>
        </el-col>
        <el-col :xs="24" :sm="12" :lg="8">
          <div class="stat-item">
            <div class="stat-label">今日手续费</div>
            <div class="stat-value warning">{{ $num(statistics.today_commission, 2) }}</div>
          </div>
        </el-col>
        <!-- ⚠️ 原来这里还有「本月手续费」和「交易笔数」两张卡,但后端
             CommissionStatistics(models.rs:418-423) 只有
             account_id / total_commission / today_commission / commission_by_instrument
             四个字段。`statistics.this_month_commission.toFixed(2)` 读 undefined
             直接把整个 el-card 渲染打断。此前该错误被 loadAccounts 的 400 掩盖
             (账户列表为空 → 从不调 loadStatistics)。@yutiansut @quantaxis -->
      </el-row>

      <!-- 按品种明细 -->
      <el-table
        v-if="statistics && statistics.commission_by_instrument && statistics.commission_by_instrument.length > 0"
        :data="statistics.commission_by_instrument"
        stripe
        border
        style="width: 100%; margin-top: 20px;"
      >
        <!-- 列对齐后端 InstrumentCommission(models.rs:427-432):
             instrument_id / open_commission / close_commission /
             close_today_commission / total。原来的 trade_count / total_volume /
             commission 三列后端一个都没有。@yutiansut @quantaxis -->
        <el-table-column prop="instrument_id" label="合约" min-width="120" show-overflow-tooltip></el-table-column>
        <el-table-column label="开仓手续费" min-width="110" align="right" show-overflow-tooltip>
          <template slot-scope="scope">{{ $num(scope.row.open_commission, 2) }}</template>
        </el-table-column>
        <el-table-column label="平仓手续费" min-width="110" align="right" show-overflow-tooltip>
          <template slot-scope="scope">{{ $num(scope.row.close_commission, 2) }}</template>
        </el-table-column>
        <el-table-column label="平今手续费" min-width="110" align="right" show-overflow-tooltip>
          <template slot-scope="scope">{{ $num(scope.row.close_today_commission, 2) }}</template>
        </el-table-column>
        <el-table-column label="合计" min-width="110" align="right" show-overflow-tooltip>
          <template slot-scope="scope">{{ $num(scope.row.total, 2) }}</template>
        </el-table-column>
      </el-table>
    </el-card>

    <!-- 手续费率表 -->
    <el-card class="rates-card">
      <div slot="header">
        <span>手续费率表</span>
        <el-input
          v-model="searchKeyword"
          placeholder="搜索品种"
          prefix-icon="el-icon-search"
          style="width: 200px; float: right;"
          clearable
        ></el-input>
      </div>
      <el-table
        :empty-text="$emptyText(loadError)"
        :data="filteredRates"
        v-loading="loading"
        stripe
        border
        style="width: 100%"
        max-height="500"
      >
        <el-table-column prop="product_id" label="品种" min-width="100" show-overflow-tooltip></el-table-column>
        <el-table-column prop="exchange_id" label="交易所" min-width="100" show-overflow-tooltip></el-table-column>
        <el-table-column label="开仓手续费" align="center">
          <el-table-column prop="open_ratio_by_money" label="按金额" min-width="75">
            <template slot-scope="scope">
              {{ scope.row.open_ratio_by_money > 0 ? $pct(scope.row.open_ratio_by_money, 4) : '-' }}
            </template>
          </el-table-column>
          <el-table-column prop="open_ratio_by_volume" label="按手数" min-width="70">
            <template slot-scope="scope">
              {{ (Number(scope.row.open_ratio_by_volume) || 0) > 0 ? $num(scope.row.open_ratio_by_volume, 2) + '元/手' : '-' }}
            </template>
          </el-table-column>
        </el-table-column>
        <el-table-column label="平仓手续费" align="center">
          <el-table-column prop="close_ratio_by_money" label="按金额" min-width="120">
            <template slot-scope="scope">
              {{ scope.row.close_ratio_by_money > 0 ? $pct(scope.row.close_ratio_by_money, 4) : '-' }}
            </template>
          </el-table-column>
          <el-table-column prop="close_ratio_by_volume" label="按手数" min-width="120">
            <template slot-scope="scope">
              {{ (Number(scope.row.close_ratio_by_volume) || 0) > 0 ? $num(scope.row.close_ratio_by_volume, 2) + '元/手' : '-' }}
            </template>
          </el-table-column>
        </el-table-column>
        <el-table-column label="平今手续费" align="center">
          <el-table-column prop="close_today_ratio_by_money" label="按金额" min-width="120">
            <template slot-scope="scope">
              {{ scope.row.close_today_ratio_by_money > 0 ? $pct(scope.row.close_today_ratio_by_money, 4) : '-' }}
            </template>
          </el-table-column>
          <el-table-column prop="close_today_ratio_by_volume" label="按手数" min-width="120">
            <template slot-scope="scope">
              {{ (Number(scope.row.close_today_ratio_by_volume) || 0) > 0 ? $num(scope.row.close_today_ratio_by_volume, 2) + '元/手' : '-' }}
            </template>
          </el-table-column>
        </el-table-column>
      </el-table>
    </el-card>
  </div>
</template>

<script>
/**
 * 手续费查询页面 @yutiansut @quantaxis
 */
import { mapGetters } from 'vuex'
import { getCommissionRates, getCommissionStatistics, getUserAccounts } from '@/api'

export default {
  name: 'CommissionQuery',

  data() {
    return {
      // 加载失败原因 —— 与「真的没数据」区分开,交给 $emptyText 渲染
      // @yutiansut @quantaxis
      loadError: null,
      loading: false,
      accounts: [],
      selectedAccountId: '',
      statistics: null,
      rates: [],
      searchKeyword: ''
    }
  },

  computed: {
    // 登录用户 ID 来自 vuex（store 持久化到 localStorage 'currentUser'）
    // @yutiansut @quantaxis
    ...mapGetters(['currentUser']),

    filteredRates() {
      if (!this.searchKeyword) return this.rates
      const keyword = this.searchKeyword.toLowerCase()
      return this.rates.filter(rate =>
        rate.product_id.toLowerCase().includes(keyword) ||
        rate.exchange_id.toLowerCase().includes(keyword)
      )
    }
  },

  created() {
    this.loadData()
  },

  methods: {
    async loadData() {
      await Promise.all([
        this.loadAccounts(),
        this.loadRates()
      ])
    },

    async loadAccounts() {
      try {
        const userId = this.currentUser
        if (!userId) return
        // request 拦截器已解包，后端返回 { accounts, total } @yutiansut @quantaxis
        const res = await getUserAccounts(userId)
        this.accounts = (res && res.accounts) || []
        if (this.accounts.length > 0 && !this.selectedAccountId) {
          this.selectedAccountId = this.accounts[0].account_id
          this.loadStatistics()
        }
      } catch (err) {
        this.loadError = (err && err.message) || '加载失败'
        console.error('加载账户列表失败:', err)
      }
    },

    async loadRates() {
      this.loading = true; this.loadError = null
      try {
        // request 拦截器已解包，直接拿到手续费率数组
        const rates = await getCommissionRates()
        this.rates = rates || []
      } catch (err) {
        this.loadError = (err && err.message) || '加载失败'
        console.error('加载手续费率失败:', err)
        this.$message.error('加载手续费率失败')
      } finally {
        this.loading = false
      }
    },

    async loadStatistics() {
      if (!this.selectedAccountId) return
      try {
        // request 拦截器已解包，直接拿到统计对象
        this.statistics = await getCommissionStatistics(this.selectedAccountId)
      } catch (err) {
        this.loadError = (err && err.message) || '加载失败'
        console.error('加载手续费统计失败:', err)
      }
    }
  }
}
</script>

<style scoped lang="scss">
@import '@/styles/variables.scss';
.commission-container {
  padding: 20px;
}

.page-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 20px;
}

.page-header h2 {
  margin: 0;
  color: $dark-text-primary;
}

.stats-card {
  margin-bottom: 20px;
}

.rates-card {
  margin-bottom: 20px;
}

/* ⚠️ 原来写死 background:#f5f7fa —— 在深色页面上是一块亮灰底。
   本文件既没引入 styles/variables.scss、也没自带 $dark-* 定义,
   所以只能硬编码,这正是「两套样式」的来源。
   实测 margin 页 5 块、commission 页 2 块 rgb(245,247,250)。
   @yutiansut @quantaxis */
.stat-item {
  text-align: center;
  padding: 20px;
  /* 值取自 styles/variables.scss:21,23 —— 本文件是 lang=css,
     不能 @import SCSS 变量,故直接写值并注明来源 */
  background: #21262d;
  border: 1px solid #30363d;
  border-radius: 8px;
}

.stat-label {
  font-size: 14px;
  color: #8b949e;
  margin-bottom: 10px;
}

.stat-value {
  font-size: 24px;
  font-weight: bold;
}

.stat-value.danger {
  color: #F56C6C;
}

.stat-value.warning {
  color: #E6A23C;
}

.stat-value.primary {
  color: #409EFF;
}

.stat-value.success {
  color: #67C23A;
}
</style>
