<template>
  <div class="transactions-container">
    <!-- 页面标题 -->
    <div class="page-header">
      <h2>资金流水管理</h2>
    </div>

    <!-- 筛选条件 -->
    <div class="filter-container">
      <el-input
        v-model="filters.userId"
        placeholder="用户ID"
        clearable
        style="width: 200px; margin-right: 10px"
      />
      <el-select
        v-model="filters.transactionType"
        placeholder="交易类型"
        clearable
        style="width: 150px; margin-right: 10px"
      >
        <el-option label="全部" value=""></el-option>
        <el-option label="入金" value="deposit"></el-option>
        <el-option label="出金" value="withdrawal"></el-option>
        <el-option label="手续费" value="commission"></el-option>
        <el-option label="盈亏" value="pnl"></el-option>
        <el-option label="结算" value="settlement"></el-option>
      </el-select>
      <el-date-picker
        v-model="dateRange"
        type="daterange"
        value-format="yyyy-MM-dd"
        @change="dateRangeTouched = true"
        range-separator="至"
        start-placeholder="开始日期"
        end-placeholder="结束日期"
        style="width: 300px; margin-right: 10px"
      />
      <el-button type="primary" icon="el-icon-search" @click="handleSearch">查询</el-button>
      <el-button icon="el-icon-refresh" @click="resetFilters">重置</el-button>
      <el-button icon="el-icon-download" @click="exportData">导出Excel</el-button>
    </div>

    <!-- 统计卡片 -->
    <div class="stats-container">
      <el-card shadow="hover" class="stat-card">
        <div class="stat-item">
          <div class="stat-label">总入金</div>
          <div class="stat-value positive">+{{ fmtMoney(totalDeposit) }}</div>
        </div>
      </el-card>
      <el-card shadow="hover" class="stat-card">
        <div class="stat-item">
          <div class="stat-label">总出金</div>
          <div class="stat-value negative">-{{ fmtMoney(totalWithdrawal) }}</div>
        </div>
      </el-card>
      <el-card shadow="hover" class="stat-card">
        <div class="stat-item">
          <div class="stat-label">净流入</div>
          <div class="stat-value" :class="{ positive: netFlow > 0, negative: netFlow < 0 }">
            {{ netFlow > 0 ? '+' : '' }}{{ fmtMoney(netFlow) }}
          </div>
        </div>
      </el-card>
      <el-card shadow="hover" class="stat-card">
        <div class="stat-item">
          <div class="stat-label">交易笔数</div>
          <div class="stat-value">{{ transactions.length }}</div>
        </div>
      </el-card>
    </div>

    <!-- 流水列表 -->
    <div class="table-container">
      <!-- ✨ 表格高度改为跟随视口, 不再写死 450px @yutiansut @quantaxis
           实测 1366x768: 上方 318px + 底部分页 52px; 原 450px 令表格底部溢出 76px
           var(--qa-content-h) 由 layout/index.vue 统一定义 = 100vh - 56(顶栏) - 40(padding),
           有公告条时自动再减 40px; max(260px, ...) 是极短视口下的兜底 -->
      <el-table
        :empty-text="$emptyText(loadError, '暂无流水')"
        ref="transactionTable"
        :data="transactions"
        @sort-change="handleSortChange"
        border
        stripe
        v-loading="loading"
        :height="'max(260px, calc(var(--qa-content-h, calc(100vh - 96px)) - 411px))'"
      >
        <el-table-column prop="transaction_id" label="交易ID" min-width="180" sortable="custom" show-overflow-tooltip></el-table-column>
        <el-table-column prop="user_id" label="用户ID" min-width="150" sortable="custom" show-overflow-tooltip></el-table-column>
        <el-table-column prop="transaction_type" label="交易类型" min-width="120" sortable="custom">
          <template slot-scope="scope">
            <el-tag :type="getTypeTagType(scope.row.transaction_type)" size="small">
              {{ getTypeName(scope.row.transaction_type) }}
            </el-tag>
          </template>
        </el-table-column>
        <!-- show-overflow-tooltip:金额可达 ¥10^11(管理端 /api/management/deposit|withdraw 走 CapitalManager,
                 **只校验 amount<=0、没有上限**;/api/account/deposit 才有 1e12 上限)。
                 实测 `-100,000,000,000.00` 需 168.7px、单元格 139px,溢出 29.72px,
                 渲染成 `-100,000,000,0…`,11 位数被读成小数量级且 hover 无提示。
             @yutiansut @quantaxis -->
        <el-table-column prop="amount" label="金额" min-width="140" align="right" sortable="custom" show-overflow-tooltip>
          <template slot-scope="scope">
            <span :class="getAmountClass(scope.row.transaction_type)">
              {{ formatAmount(scope.row.transaction_type, scope.row.amount) }}
            </span>
          </template>
        </el-table-column>
        <el-table-column prop="balance_before" label="交易前余额" min-width="140" align="right" show-overflow-tooltip>
          <template slot-scope="scope">
            {{ $money(scope.row.balance_before) }}
          </template>
        </el-table-column>
        <el-table-column prop="balance_after" label="交易后余额" min-width="140" align="right" show-overflow-tooltip>
          <template slot-scope="scope">
            {{ $money(scope.row.balance_after) }}
          </template>
        </el-table-column>
        <el-table-column prop="status" label="状态" min-width="100" sortable="custom">
          <template slot-scope="scope">
            <el-tag :type="getStatusTagType(scope.row.status)" size="small">
              {{ getStatusName(scope.row.status) }}
            </el-tag>
          </template>
        </el-table-column>
        <el-table-column prop="method" label="方式" min-width="120">
          <template slot-scope="scope">
            {{ getMethodName(scope.row.method) }}
          </template>
        </el-table-column>
        <el-table-column prop="remark" label="备注" min-width="200"></el-table-column>
        <el-table-column prop="created_at" label="交易时间" min-width="180" sortable="custom">
          <template slot-scope="scope">
            {{ scope.row.created_at }}
          </template>
        </el-table-column>
      </el-table>
      <!-- 分页控件 @yutiansut @quantaxis -->
      <div class="pagination-container">
        <!-- ✨ page-sizes 去掉 200 —— 后端 management.rs:420 是 .min(100),
             前端按 200 算页数、后端按 100 取偏移,每页会漏掉 100 条且永远看不到。
             audit-logs.vue 已经是 [20,50,100]。
             ⚠️ 注释必须写在起始标签**之前**:写进属性区会让整个标签失效
             (Vue 编译器不报错,但 AST 里该标签消失、组件不渲染)。
             @yutiansut @quantaxis -->
        <el-pagination
          background
          layout="total, sizes, prev, pager, next, jumper"
          :total="pagination.total"
          :page-size="pagination.pageSize"
          :current-page="pagination.page"
          :page-sizes="[20, 50, 100]"
          @current-change="handlePageChange"
          @size-change="handleSizeChange"
        />
      </div>
    </div>
  </div>
</template>

<script>
import dayjs from 'dayjs'
import { getTransactions, getAllTransactions } from '@/api'

export default {
  name: 'TransactionManagement',
  data() {
    return {
      // 加载失败原因(null = 未失败)
      loadError: null,
      loading: false,
      transactions: [],
      filters: {
        userId: '',
        transactionType: ''
      },
      dateRange: null,
      // 用户是否**主动**改过日期 —— 只有主动改过才提示「全局查询不支持日期」。
      // 否则 mounted() 的默认近 7 天会让**每次加载**都弹 warning。@yutiansut @quantaxis
      dateRangeTouched: false,
      // 排序状态(服务端排序) @yutiansut @quantaxis
      sort: { prop: '', order: '' },
      pagination: {
        page: 1,
        pageSize: 50,
        total: 0
      }
    }
  },
  computed: {
    filteredTransactions() {
      let result = this.transactions

      if (this.filters.userId) {
        result = result.filter(t => t.user_id.includes(this.filters.userId))
      }

      if (this.filters.transactionType) {
        result = result.filter(t => t.transaction_type === this.filters.transactionType)
      }

      return result
    },
    totalDeposit() {
      return this.transactions
        .filter(t => t.transaction_type === 'deposit')
        .reduce((sum, t) => sum + (Number(t.amount) || 0), 0)
    },
    totalWithdrawal() {
      return this.transactions
        .filter(t => t.transaction_type === 'withdrawal')
        .reduce((sum, t) => sum + (Number(t.amount) || 0), 0)
    },
    netFlow() {
      return this.totalDeposit - this.totalWithdrawal
    }
  },
  mounted() {
    // 默认查询近7天
    const end = new Date()
    const start = new Date()
    start.setDate(start.getDate() - 7)
    this.dateRange = [start, end]
    this.fetchTransactions()
  },
  methods: {
    // 金额格式化 —— 统一两位小数。@yutiansut @quantaxis
    //
    // ⚠️ 原来这几个统计卡用的是裸 `toLocaleString()`(无 options),
    // 而**同文件下方表格**用的是带 `minimumFractionDigits: 2` 的版本。
    // 结果:1366×768 实测卡片显示 `总资金 4,232,972,133.148`(**三位小数**)、
    // `可用资金 4,156,965,285.352`,而正下方表格里同类金额是两位。
    // 375×667 下这还会让 `.stat-value` 溢出容器(scrollWidth 164px / 槽宽 48px)。
      // ✨ 委托全局 `$money`(main.js):判空 → isFinite → **量级上限 1e15**。
      // 全仓曾有 13 个各写各的金额格式化器,**没有一个带量级闸**,
      // 于是 `balance = 1e300` 的污染账户在 6 个页面渲染成 404 个字符
      // (`Number.isFinite(1e300)` 为真,只判 isFinite 挡不住)。@yutiansut @quantaxis
    fmtMoney (v) {
      return this.$money(v)
    },

    // 统一成 yyyy-MM-dd —— 后端 start_date/end_date 是 String 类型。
    // 加了 value-format 后 v-model 已是字符串,这里再兜一层以防日后改回 Date。
    // @yutiansut @quantaxis
    toDateStr (v) {
      if (!v) return ''
      if (typeof v === 'string') return v.slice(0, 10)
      const d = v instanceof Date ? v : new Date(v)
      if (isNaN(d.getTime())) return ''
      const p = n => String(n).padStart(2, '0')
      return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`
    },
      // ✨ 查询必须重置页码 —— 否则在第 3 页上改筛选条件,
      //    会带着 page=3 去请求新的结果集,拿到空表。 @yutiansut @quantaxis
    handleSearch() {
      this.pagination.page = 1
      this.fetchTransactions()
    },

    // 列头排序 —— 回服务端重查。@yutiansut @quantaxis
    //
    // ⚠️ 本表服务端分页(:data 只有当前页),默认 `sortable="custom"` 只排当前页,
    // 顺序错却不声张。列已改 `sortable="custom"`,后端 management.rs 用
    // apply_sort! 宏在分页之前排全量;金额走 total_cmp(NaN 安全),
    // transaction_type / status 是枚举,按 Debug 串排。
    handleSortChange ({ prop, order }) {
      this.sort = { prop: order ? prop : '', order: order || '' }
      this.pagination.page = 1
      this.fetchTransactions()
    },
    async fetchTransactions() {
      this.loading = true
      try {
        this.loadError = null   // 重试先清除上次失败标记
        // 构建查询参数 @yutiansut @quantaxis
        const params = {
          page: this.pagination.page,
          page_size: this.pagination.pageSize
        }
        // 排序回服务端 —— 见 handleSortChange 的说明
        if (this.sort.prop) {
          params.sort_by = this.sort.prop
          params.sort_order = this.sort.order === 'ascending' ? 'asc' : 'desc'
        }

        // 类型筛选
        if (this.filters.transactionType) {
          params.transaction_type = this.filters.transactionType
        }

        // ✨ 日期筛选此前**完全没接** —— `dateRange` 只被写(:217/:282)从不被读,
        //    日期选择器是纯装饰。@yutiansut @quantaxis
        //
        // ⚠️ 两条路径的后端能力不同(实测 management.rs):
        //   · `/management/transactions/{userId}` (:350) —— **支持** start_date/end_date
        //   · `/management/transactions`          (:573) —— 只有 transaction_type/page/page_size
        // 所以只在「按用户查」时传日期;全局查时给出明确提示,而不是静默忽略
        //(静默忽略 = 用户选了日期、结果没变,以为系统坏了)。
        const dr = this.dateRange
        const hasRange = Array.isArray(dr) && dr.length === 2 && dr[0] && dr[1]

        let result
        if (this.filters.userId) {
          if (hasRange) {
            params.start_date = this.toDateStr(dr[0])
            params.end_date = this.toDateStr(dr[1])
          }
          // 如果指定了用户ID，使用用户专用API
          result = await getTransactions(this.filters.userId, params)
          // 旧接口返回数组
          this.transactions = Array.isArray(result) ? result : ((result && result.records) || [])
          // 优先用服务端总数(拦截器从 X-Total-Count 挂到 __total 上),
          // 拿不到才退回数组长度。@yutiansut @quantaxis
          //
          // 原来无条件用数组长度 ⇒ 分页条的「总数」恒等于当前页条数,
          // **把分页失效掩盖成看起来正常** —— 这不是编造数据,
          // 而是用一个自洽的错误值掩盖功能失效,比编造更难察觉。
          const srvTotal = (result && result.__total) != null
            ? result.__total
            : (result && result.total)
          this.pagination.total = srvTotal != null ? srvTotal : this.transactions.length
        } else {
          // 默认加载全部流水
          // ⚠️ 只在用户**主动改过**日期时提示 @yutiansut @quantaxis
          //
          // 回验实测:`mounted()`(:213)默认设了近 7 天,而 userId 默认为空,
          // 于是**每次页面加载和每次「重置」都会弹**这条 warning ——
          // 用户根本没碰过日期选择器。这是我加提示时引入的噪声。
          if (hasRange && this.dateRangeTouched) {
            this.$message.warning('全局流水查询暂不支持按日期筛选;请先填写用户ID')
          }
          result = await getAllTransactions(params)
          // 新接口返回 { records, total, page, page_size }
          this.transactions = (result && result.records) || []
          this.pagination.total = (result && result.total) || 0
        }
      } catch (error) {
        // ✨ 区分「加载失败」与「真的没数据」—— 两者原先在界面上完全同形,
        //    这是「无法显示」反馈的结构性来源。@yutiansut @quantaxis
        this.loadError = this.$errMsg(error, '加载失败')
        this.$message.error('获取资金流水失败: ' + error.message)
      } finally {
        this.loading = false
      }
    },
    handlePageChange(page) {
      this.pagination.page = page
      this.fetchTransactions()
    },
    handleSizeChange(size) {
      this.pagination.pageSize = size
      this.pagination.page = 1
      this.fetchTransactions()
    },
    resetFilters() {
      this.dateRangeTouched = false   // 重置后不再算「主动改过」
      this.filters = {
        userId: '',
        transactionType: ''
      }
      const end = new Date()
      const start = new Date()
      start.setDate(start.getDate() - 7)
      this.dateRange = [start, end]
      // ✨ 原先只把 transactions 置空就结束 —— 表格清空后不再加载,
      //    用户以为「没有数据」。重置页码并重新拉取。 @yutiansut @quantaxis
      this.pagination.page = 1
      this.fetchTransactions()
    },
    formatDate(date) {
      const year = date.getFullYear()
      const month = String(date.getMonth() + 1).padStart(2, '0')
      const day = String(date.getDate()).padStart(2, '0')
      return `${year}-${month}-${day}`
    },
    getTypeTagType(type) {
      const types = {
        deposit: 'success',
        withdrawal: 'warning',
        commission: 'info',
        pnl: 'primary',
        settlement: 'info'
      }
      return types[type] || 'info'
    },
    getTypeName(type) {
      const names = {
        deposit: '入金',
        withdrawal: '出金',
        commission: '手续费',
        pnl: '盈亏',
        settlement: '结算'
      }
      return names[type] || type
    },
    getStatusTagType(status) {
      const types = {
        completed: 'success',
        pending: 'warning',
        failed: 'danger',
        cancelled: 'info'
      }
      return types[status] || 'info'
    },
    getStatusName(status) {
      const names = {
        completed: '已完成',
        pending: '处理中',
        failed: '失败',
        cancelled: '已取消'
      }
      return names[status] || status
    },
    getMethodName(method) {
      const names = {
        bank_transfer: '银行转账',
        wechat: '微信支付',
        alipay: '支付宝',
        other: '其他'
      }
      return names[method] || method || '-'
    },
    getAmountClass(type) {
      return type === 'deposit' ? 'positive' : type === 'withdrawal' ? 'negative' : ''
    },
    formatAmount(type, amount) {
      const prefix = type === 'deposit' ? '+' : type === 'withdrawal' ? '-' : ''
      // Number(x)||0:同时挡住 null/缺失**和数字字符串** —— 后者会让 `sum + x`
      // 变成字符串拼接,再 .toFixed 就崩,且崩得静默(pageerror=0)。@yutiansut @quantaxis
      // 前缀已表达正负,值必须取绝对 —— 否则负 amount 渲染成 `+-9,876.54`。
      // 与 `admin/risk.vue:196` 同型(那处已修)。当前后端出金记正数故不可达,
      // 属潜伏对称问题。@yutiansut @quantaxis
      return prefix + Math.abs(Number(amount) || 0).toLocaleString('zh-CN', { minimumFractionDigits: 2, maximumFractionDigits: 2 })
    },
    exportData() {
      if (this.transactions.length === 0) {
        this.$message.warning('暂无数据可导出')
        return
      }
      const headers = ['时间', '账户', '类型', '金额', '状态', '方式', '备注']
      const rows = this.transactions.map(item => [
        item.timestamp || item.created_at || '',
        item.user_id || '',
        this.getTypeName(item.transaction_type || item.type || ''),
        Number(Number(item.amount) || 0).toFixed(2),
        this.getStatusName(item.status || ''),
        this.getMethodName(item.method || ''),
        item.remark || ''
      ])

      const csvContent = [headers.join(','), ...rows.map(r => r.join(','))].join('\n')
      const blob = new Blob(['\ufeff' + csvContent], { type: 'text/csv;charset=utf-8;' })
      const link = document.createElement('a')
      link.href = URL.createObjectURL(blob)
      link.download = `transactions_${dayjs().format('YYYYMMDD_HHmmss')}.csv`
      document.body.appendChild(link)
      link.click()
      document.body.removeChild(link)
    }
  }
}
</script>

<style lang="scss" scoped>
// @yutiansut @quantaxis - 资金流水管理页面深色主题
$dark-bg-primary: #0d1117;
$dark-bg-secondary: #161b22;
$dark-bg-card: #1c2128;
$dark-bg-tertiary: #21262d;
$dark-border: #30363d;
$dark-text-primary: #f0f6fc;
$dark-text-secondary: #8b949e;
$dark-text-muted: #6e7681;
$primary-color: #1890ff;
$success-color: #52c41a;
$danger-color: #f5222d;
$warning-color: #faad14;

.transactions-container {
  padding: 20px;
  min-height: 100%;
  background: $dark-bg-primary;
}

.page-header {
  margin-bottom: 20px;

  h2 {
    margin: 0 0 20px 0;
    font-size: 24px;
    font-weight: 600;
    color: $dark-text-primary;
  }
}

.filter-container {
  margin-bottom: 20px;
  padding: 16px;
  background: $dark-bg-secondary;
  border-radius: 8px;
  border: 1px solid $dark-border;

  ::v-deep .el-input__inner {
    background: $dark-bg-tertiary !important;
    border-color: $dark-border !important;
    color: $dark-text-primary !important;

    &::placeholder {
      color: $dark-text-muted;
    }
  }

  ::v-deep .el-select .el-input__inner {
    background: $dark-bg-tertiary !important;
    border-color: $dark-border !important;
    color: $dark-text-primary !important;
  }

  ::v-deep .el-date-editor {
    background: $dark-bg-tertiary !important;
    border-color: $dark-border !important;

    .el-range-input {
      background: transparent !important;
      color: $dark-text-primary !important;

      &::placeholder {
        color: $dark-text-muted;
      }
    }

    .el-range-separator {
      color: $dark-text-secondary;
    }

    .el-range__icon,
    .el-range__close-icon {
      color: $dark-text-secondary;
    }
  }

  ::v-deep .el-button--primary {
    background: $primary-color;
    border-color: $primary-color;
  }

  ::v-deep .el-button--default {
    background: $dark-bg-tertiary;
    border-color: $dark-border;
    color: $dark-text-secondary;

    &:hover {
      border-color: $primary-color;
      color: $primary-color;
    }
  }
}

.stats-container {
  display: flex;
  gap: 20px;
  margin-bottom: 20px;
}

.stat-card {
  flex: 1;

  ::v-deep.el-card {
    background: $dark-bg-card !important;
    border: 1px solid $dark-border !important;
    border-radius: 10px;

    .el-card__body {
      padding: 20px;
    }
  }
}

.stat-item {
  text-align: center;
}

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

.table-container {
  background: $dark-bg-card;
  padding: 20px;
  border-radius: 8px;
  border: 1px solid $dark-border;

  .pagination-container {
    margin-top: 20px;
    display: flex;
    justify-content: flex-end;

    ::v-deep .el-pagination {
      button {
        background: $dark-bg-tertiary !important;
        color: $dark-text-primary !important;
        border: 1px solid $dark-border !important;

        &:disabled {
          color: $dark-text-muted !important;
        }

        &:hover:not(:disabled) {
          color: $primary-color !important;
          border-color: $primary-color !important;
        }
      }

      .el-pager li {
        background: $dark-bg-tertiary !important;
        color: $dark-text-primary !important;
        border: 1px solid $dark-border !important;

        &.active {
          background: $primary-color !important;
          border-color: $primary-color !important;
        }

        &:hover {
          color: $primary-color !important;
        }
      }

      .el-pagination__total,
      .el-pagination__jump {
        color: $dark-text-secondary !important;
      }

      .el-pagination__editor.el-input .el-input__inner {
        background: $dark-bg-tertiary !important;
        border-color: $dark-border !important;
        color: $dark-text-primary !important;
      }

      .el-select .el-input .el-input__inner {
        background: $dark-bg-tertiary !important;
        border-color: $dark-border !important;
        color: $dark-text-primary !important;
      }
    }
  }

  ::v-deep .el-table {
    background: transparent !important;

    &::before {
      display: none;
    }

    th.el-table__cell {
      background: $dark-bg-secondary !important;
      border-bottom: 1px solid $dark-border !important;
      color: $dark-text-secondary !important;
      font-weight: 600;
      font-size: 12px;
      text-transform: uppercase;
      letter-spacing: 0.5px;
    }

    td.el-table__cell {
      background: $dark-bg-card !important;
      border-bottom: 1px solid $dark-border !important;
      color: $dark-text-primary !important;
      font-family: 'JetBrains Mono', monospace;
      font-size: 13px;
    }

    tr {
      background: $dark-bg-card !important;
    }

    .el-table__row--striped td.el-table__cell {
      background: $dark-bg-secondary !important;
    }

    .el-table__row:hover > td.el-table__cell {
      background: $dark-bg-tertiary !important;
    }

    .cell {
      color: $dark-text-primary !important;
    }

    .el-table__empty-block {
      background: $dark-bg-card;
    }

    .el-table__empty-text {
      color: $dark-text-muted;
    }

    // 滚动条
    .el-table__body-wrapper {
      &::-webkit-scrollbar {
        width: var(--qa-sbw, 8px);
        height: var(--qa-sbw, 8px);
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
  }
}

.positive {
  color: $success-color !important;
  font-weight: 600;
  text-shadow: 0 0 10px rgba($success-color, 0.3);
}

.negative {
  color: $danger-color !important;
  font-weight: 600;
  text-shadow: 0 0 10px rgba($danger-color, 0.3);
}

// 标签样式
::v-deep .el-tag {
  border: none;
  font-weight: 600;

  &--success {
    background: rgba($success-color, 0.15);
    color: $success-color;
  }

  &--warning {
    background: rgba($warning-color, 0.15);
    color: $warning-color;
  }

  &--info {
    background: rgba($dark-text-secondary, 0.15);
    color: $dark-text-secondary;
  }

  &--danger {
    background: rgba($danger-color, 0.15);
    color: $danger-color;
  }

  &--primary {
    background: rgba($primary-color, 0.15);
    color: $primary-color;
  }
}

// 下拉菜单
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
      font-weight: 600;
    }
  }
}
</style>
