<template>
  <div class="margin-container">
    <!-- 页面标题 -->
    <div class="page-header">
      <h2>保证金查询</h2>
      <div class="header-actions">
        <el-button icon="el-icon-refresh" @click="loadData">刷新</el-button>
      </div>
    </div>

    <!-- 账户保证金汇总 -->
    <el-card class="summary-card">
      <div slot="header">
        <span>保证金汇总</span>
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
            <el-select v-model="selectedAccountId" placeholder="请选择账户" style="width: 380px" @change="loadSummary">
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
      <el-row :gutter="20" v-if="summary">
        <el-col :xs="12" :sm="8" :lg="4">
          <div class="stat-item">
            <div class="stat-label">占用保证金</div>
            <div class="stat-value primary">{{ $num(summary.total_margin, 2) }}</div>
          </div>
        </el-col>
        <el-col :xs="12" :sm="8" :lg="4">
          <div class="stat-item">
            <div class="stat-label">冻结保证金</div>
            <div class="stat-value warning">{{ $num(summary.frozen_margin, 2) }}</div>
          </div>
        </el-col>
        <el-col :xs="12" :sm="8" :lg="4">
          <div class="stat-item">
            <div class="stat-label">可用保证金</div>
            <div class="stat-value success">{{ $num(summary.available_margin, 2) }}</div>
          </div>
        </el-col>
        <el-col :xs="12" :sm="8" :lg="4">
          <div class="stat-item">
            <div class="stat-label">保证金占比</div>
            <div class="stat-value">{{ $pct(summary.margin_ratio, 2) }}</div>
          </div>
        </el-col>
        <el-col :xs="12" :sm="8" :lg="4">
          <div class="stat-item">
            <div class="stat-label">风险度</div>
            <div :class="['stat-value', getRiskClass(summary.risk_degree)]">
              {{ $pct(summary.risk_degree, 2) }}
            </div>
          </div>
        </el-col>
      </el-row>

      <!-- 持仓保证金明细 -->
      <el-table
        v-if="summary && summary.positions && summary.positions.length > 0"
        :data="summary.positions"
        stripe
        border
        style="width: 100%; margin-top: 20px;"
      >
        <el-table-column prop="instrument_id" label="合约" min-width="150" show-overflow-tooltip></el-table-column>
        <el-table-column label="多头" align="center">
          <el-table-column prop="volume_long" label="持仓" width="80"></el-table-column>
          <el-table-column prop="margin_long" label="保证金" min-width="120">
            <template slot-scope="scope">
              {{ $num(scope.row.margin_long, 2) }}
            </template>
          </el-table-column>
          <el-table-column prop="margin_rate_long" label="保证金率" min-width="100">
            <template slot-scope="scope">
              {{ $pct(scope.row.margin_rate_long, 2) }}
            </template>
          </el-table-column>
        </el-table-column>
        <el-table-column label="空头" align="center">
          <el-table-column prop="volume_short" label="持仓" width="80"></el-table-column>
          <el-table-column prop="margin_short" label="保证金" min-width="120">
            <template slot-scope="scope">
              {{ $num(scope.row.margin_short, 2) }}
            </template>
          </el-table-column>
          <el-table-column prop="margin_rate_short" label="保证金率" min-width="100">
            <template slot-scope="scope">
              {{ $pct(scope.row.margin_rate_short, 2) }}
            </template>
          </el-table-column>
        </el-table-column>
        <el-table-column prop="last_price" label="最新价" min-width="100">
          <template slot-scope="scope">
            {{ $num(scope.row.last_price, 2) }}
          </template>
        </el-table-column>
      </el-table>
    </el-card>

    <!-- 保证金率表 -->
    <el-card class="rates-card">
      <div slot="header">
        <span>保证金率表</span>
        <el-input
          v-model="searchKeyword"
          placeholder="搜索合约"
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
        max-height="400"
      >
        <el-table-column prop="instrument_id" label="合约" min-width="120" show-overflow-tooltip></el-table-column>
        <!-- 本表绑的是 `filteredRates`(来自 `/account-admin/margin/rates`),
             它返回的正是 `long_margin_ratio_by_money` / `_by_volume` 这套字段名。
             ⚠️ **不要**把这里换成 `margin_rate_long` / `margin_rate_short` ——
             那是**另一个端点** `/account-admin/margin/summary` 的 `positions[]` 字段,
             对应本页上方那张「持仓保证金明细」表(:81 `:data="summary.positions"`)。
             两张表数据源不同、字段名不同,曾被误当成同一套而改错。@yutiansut @quantaxis
             `*_by_volume` 在后端 `init_default_margin_rates` 里恒填 0.0
             (按手数计的保证金率本就不适用于这些品种),所以那两列显示 `-` 是正确的。 -->
        <el-table-column label="多头保证金率" align="center">
          <el-table-column prop="long_margin_ratio_by_money" label="按金额" min-width="70">
            <template slot-scope="scope">
              {{ $pct(scope.row.long_margin_ratio_by_money, 2) }}
            </template>
          </el-table-column>
          <el-table-column prop="long_margin_ratio_by_volume" label="按手数" min-width="120">
            <template slot-scope="scope">
              {{ (Number(scope.row.long_margin_ratio_by_volume) || 0) > 0 ? $num(scope.row.long_margin_ratio_by_volume, 2) + '元/手' : '-' }}
            </template>
          </el-table-column>
        </el-table-column>
        <el-table-column label="空头保证金率" align="center">
          <el-table-column prop="short_margin_ratio_by_money" label="按金额" min-width="120">
            <template slot-scope="scope">
              {{ $pct(scope.row.short_margin_ratio_by_money, 2) }}
            </template>
          </el-table-column>
          <el-table-column prop="short_margin_ratio_by_volume" label="按手数" min-width="120">
            <template slot-scope="scope">
              {{ (Number(scope.row.short_margin_ratio_by_volume) || 0) > 0 ? $num(scope.row.short_margin_ratio_by_volume, 2) + '元/手' : '-' }}
            </template>
          </el-table-column>
        </el-table-column>
      </el-table>
    </el-card>
  </div>
</template>

<script>
/**
 * 保证金查询页面 @yutiansut @quantaxis
 */
import { mapGetters } from 'vuex'
import { getMarginRates, getAccountMarginSummary, getUserAccounts } from '@/api'

export default {
  name: 'MarginQuery',

  data() {
    return {
      // 加载失败原因 —— 与「真的没数据」区分开,交给 $emptyText 渲染
      // @yutiansut @quantaxis
      loadError: null,
      loading: false,
      accounts: [],
      selectedAccountId: '',
      summary: null,
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
        rate.instrument_id.toLowerCase().includes(keyword)
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
          this.loadSummary()
        }
      } catch (err) {
        this.loadError = (err && err.message) || '加载失败'
        console.error('加载账户列表失败:', err)
      }
    },

    async loadRates() {
      this.loading = true; this.loadError = null
      try {
        // request 拦截器已解包，直接拿到保证金率数组
        const rates = await getMarginRates()
        this.rates = rates || []
      } catch (err) {
        this.loadError = (err && err.message) || '加载失败'
        console.error('加载保证金率失败:', err)
        this.$message.error('加载保证金率失败')
      } finally {
        this.loading = false
      }
    },

    async loadSummary() {
      if (!this.selectedAccountId) return
      try {
        // request 拦截器已解包，直接拿到保证金汇总对象
        this.summary = await getAccountMarginSummary(this.selectedAccountId)
      } catch (err) {
        this.loadError = (err && err.message) || '加载失败'
        console.error('加载保证金汇总失败:', err)
      }
    },

    getRiskClass(risk) {
      if (risk >= 0.9) return 'danger'
      if (risk >= 0.8) return 'warning'
      return 'success'
    }
  }
}
</script>

<style scoped lang="scss">
@import '@/styles/variables.scss';
.margin-container {
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

.summary-card {
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
  font-size: 20px;
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
