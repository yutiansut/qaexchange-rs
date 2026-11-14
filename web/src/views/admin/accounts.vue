<template>
  <div class="accounts-container qa-flex-page">
    <!-- 页面标题 -->
    <div class="page-header">
      <h2>账户管理</h2>
      <div class="header-stats">
        <el-card shadow="hover" class="stat-card">
          <div class="stat-item">
            <div class="stat-label">总账户数</div>
            <div class="stat-value">{{ totalAccounts }}</div>
          </div>
        </el-card>
        <el-card shadow="hover" class="stat-card">
          <div class="stat-item">
            <div class="stat-label">总资金 <span class="stat-note">{{ totalsNote }}</span></div>
            <div class="stat-value">{{ fmtMoney(totalBalance) }}</div>
          </div>
        </el-card>
        <el-card shadow="hover" class="stat-card">
          <div class="stat-item">
            <div class="stat-label">可用资金 <span class="stat-note">{{ totalsNote }}</span></div>
            <div class="stat-value">{{ fmtMoney(totalAvailable) }}</div>
          </div>
        </el-card>
      </div>
    </div>

    <!-- 筛选条件 -->
    <div class="filter-container">
      <el-input
        v-model="filters.userId"
        placeholder="用户ID"
        clearable
        style="width: 200px; margin-right: 10px"
        @clear="fetchAccounts"
      />
      <el-select
        v-model="filters.status"
        placeholder="账户状态"
        clearable
        style="width: 150px; margin-right: 10px"
      >
        <el-option label="全部" value=""></el-option>
        <el-option label="活跃" value="active"></el-option>
        <el-option label="冻结" value="frozen"></el-option>
      </el-select>
      <el-button type="primary" icon="el-icon-search" @click="handleSearch">查询</el-button>
      <el-button icon="el-icon-refresh" @click="resetFilters">重置</el-button>
    </div>

    <!-- 账户列表 @yutiansut @quantaxis -->
    <div class="table-container">
      <!-- ✨ 表格高度改为跟随视口, 不再写死 500px @yutiansut @quantaxis
           实测 1366x768: 上方 316px + 底部分页 52px; 原 500px 令表格底部溢出 124px
           var(--qa-content-h) 由 layout/index.vue 统一定义 = 100vh - 56(顶栏) - 40(padding),
           有公告条时自动再减 40px; max(260px, ...) 是极短视口下的兜底 -->
      <!-- 刷新失败但表里还留着上一次的数据 —— 必须持久提示,不能只弹一条 3 秒的 toast。
           交易系统里「陈旧余额 + 无提示」比空表更危险:用户会照着过期数字做决策。
           清空列表也不对(那等于丢掉唯一能看的数据),所以保留数据 + 明示状态。
           @yutiansut @quantaxis -->
      <el-alert
        v-if="loadError && accounts.length > 0"
        :title="'刷新失败，下方为上次成功加载的数据：' + loadError"
        type="warning"
        :closable="false"
        show-icon
        style="margin-bottom: 12px;"
      />
      <!-- ⚠️ 保留 height prop 并固定为 '100%' —— 不要删,也不要写常数 N。
      @yutiansut @quantaxis
      `flex:1` ⇒ `flex-basis:0%`,主轴尺寸由 flex 决定,height 不参与布局
      (实测 tableH 与改前逐位相同);但 `layout.height !== null` 让 Element
      自己做 `fixedBodyHeight = bodyHeight - gutterWidth`、渲染 th.gutter、
      并在 updateColumnsWidth 里扣掉 gutter。删掉它会让 layout.scrollY
      永久为 false,连锁产生冻结列错位/竖条被盖/多余横条/表头滞后四个缺陷。
      常数 N 则是另一个错:所需值随**宽度**跳变(表格上方统计卡换行)。 -->
      <el-table
        class="qa-flex-table"
        :empty-text="$emptyText(loadError, '暂无账户')"
        ref="accountTable"
        :data="accounts"
        @sort-change="handleSortChange"
        border
        stripe
        highlight-current-row
        v-loading="loading"
        :height="'100%'"
        style="width: 100%"
      >
        <el-table-column prop="account_id" label="账户ID" min-width="200" sortable="custom" show-overflow-tooltip></el-table-column>
        <el-table-column prop="owner_user_id" label="所属用户" min-width="150" sortable="custom" show-overflow-tooltip></el-table-column>
        <el-table-column prop="account_name" label="账户名称" min-width="150"></el-table-column>
        <el-table-column prop="account_type" label="账户类型" min-width="80">
          <template slot-scope="scope">
            <el-tag :type="scope.row.account_type === 'Individual' ? 'success' : 'warning'" size="small">
              {{ scope.row.account_type === 'Individual' ? '个人' : '机构' }}
            </el-tag>
          </template>
        </el-table-column>
        <el-table-column prop="balance" label="总权益" min-width="140" align="right" show-overflow-tooltip>
          <template slot-scope="scope">
            <span :class="{ 'positive': scope.row.balance > 0 }" :title="moneyTitle(scope.row.balance)">
              {{ fmtMoney(scope.row.balance) }}
            </span>
          </template>
        </el-table-column>
        <el-table-column prop="available" label="可用资金" min-width="140" align="right" show-overflow-tooltip>
          <template slot-scope="scope">
            {{ fmtMoney(scope.row.available) }}
          </template>
        </el-table-column>
        <el-table-column prop="margin_used" label="占用保证金" min-width="140" align="right" show-overflow-tooltip>
          <template slot-scope="scope">
            {{ fmtMoney(scope.row.margin_used) }}
          </template>
        </el-table-column>
        <el-table-column prop="risk_ratio" label="风险率" min-width="90" align="right" show-overflow-tooltip>
          <template slot-scope="scope">
            <el-tag :type="getRiskTagType(scope.row.risk_ratio)" size="small">
              {{ fmtPct(scope.row.risk_ratio) }}
            </el-tag>
          </template>
        </el-table-column>
        <el-table-column prop="created_at" label="创建时间" min-width="140">
          <template slot-scope="scope">
            {{ formatTimestamp(scope.row.created_at) }}
          </template>
        </el-table-column>
        <el-table-column label="操作" width="200">
          <template slot-scope="scope">
            <el-button size="mini" type="text" @click="showAccountDetail(scope.row)">详情</el-button>
            <el-button size="mini" type="text" @click="showDepositDialog(scope.row)">入金</el-button>
            <el-button size="mini" type="text" @click="showWithdrawDialog(scope.row)">出金</el-button>
          </template>
        </el-table-column>
      </el-table>

      <!-- 分页 -->
      <div class="pagination-container">
        <el-pagination
          background
          layout="total, sizes, prev, pager, next, jumper"
          :total="pagination.total"
          :page-size="pagination.pageSize"
          :current-page="pagination.currentPage"
          :page-sizes="[10, 20, 50, 100]"
          @size-change="handleSizeChange"
          @current-change="handleCurrentChange"
        />
      </div>
    </div>

    <!-- 入金对话框 -->
    <el-dialog
      title="入金"
      :visible.sync="depositDialogVisible"
      width="500px"
      :close-on-click-modal="false"
    >
      <el-form :model="depositForm" :rules="depositRules" ref="depositFormRef" label-width="100px">
        <el-form-item label="用户ID">
          <el-input v-model="depositForm.user_id" disabled></el-input>
        </el-form-item>
        <el-form-item label="入金金额" prop="amount">
          <el-input-number
            v-model="depositForm.amount"
            :min="0.01"
            :precision="2"
            :step="100"
            controls-position="right"
            style="width: 100%"
          ></el-input-number>
        </el-form-item>
        <el-form-item label="入金方式" prop="method">
          <el-select v-model="depositForm.method" placeholder="请选择" style="width: 100%">
            <el-option label="银行转账" value="bank_transfer"></el-option>
            <el-option label="微信支付" value="wechat"></el-option>
            <el-option label="支付宝" value="alipay"></el-option>
            <el-option label="其他" value="other"></el-option>
          </el-select>
        </el-form-item>
        <el-form-item label="备注">
          <el-input v-model="depositForm.remark" type="textarea" :rows="3"></el-input>
        </el-form-item>
      </el-form>
      <div slot="footer">
        <el-button @click="depositDialogVisible = false">取消</el-button>
        <el-button type="primary" @click="handleDeposit" :loading="submitting">确认入金</el-button>
      </div>
    </el-dialog>

    <!-- 出金对话框 -->
    <el-dialog
      title="出金"
      :visible.sync="withdrawDialogVisible"
      width="500px"
      :close-on-click-modal="false"
    >
      <el-form :model="withdrawForm" :rules="withdrawRules" ref="withdrawFormRef" label-width="100px">
        <el-form-item label="用户ID">
          <el-input v-model="withdrawForm.user_id" disabled></el-input>
        </el-form-item>
        <el-form-item label="可用资金">
          <el-input v-model="currentAccountAvailable" disabled></el-input>
        </el-form-item>
        <el-form-item label="出金金额" prop="amount">
          <el-input-number
            v-model="withdrawForm.amount"
            :min="0.01"
            :max="parseFloat(currentAccountAvailable)"
            :precision="2"
            :step="100"
            controls-position="right"
            style="width: 100%"
          ></el-input-number>
        </el-form-item>
        <el-form-item label="出金方式" prop="method">
          <el-select v-model="withdrawForm.method" placeholder="请选择" style="width: 100%">
            <el-option label="银行转账" value="bank_transfer"></el-option>
            <el-option label="微信支付" value="wechat"></el-option>
            <el-option label="支付宝" value="alipay"></el-option>
          </el-select>
        </el-form-item>
        <el-form-item label="银行账号" v-if="withdrawForm.method === 'bank_transfer'">
          <el-input v-model="withdrawForm.bank_account" placeholder="请输入银行账号"></el-input>
        </el-form-item>
      </el-form>
      <div slot="footer">
        <el-button @click="withdrawDialogVisible = false">取消</el-button>
        <el-button type="primary" @click="handleWithdraw" :loading="submitting">确认出金</el-button>
      </div>
    </el-dialog>
  </div>
</template>

<script>
import { listAllAccounts, managementDeposit, managementWithdraw } from '@/api'

// 金额可显示上限 —— 超过即判为坏数据。@yutiansut @quantaxis
// 取 1e15(MAX_SAFE_INTEGER 量级):本系统单账户余额不可能到这个量级,
// 而线上确实存在 1e300 的污染值,`Number.isFinite` 拦不住它。
const MONEY_MAX = 1e15

export default {
  name: 'AccountManagement',
  data() {
    return {
      // 加载失败原因(null = 未失败)
      loadError: null,
      // 当前排序状态(服务端排序) @yutiansut @quantaxis
      sort: { prop: '', order: '' },
      loading: false,
      submitting: false,
      accounts: [],
      filters: {
        userId: '',
        status: ''
      },
      pagination: {
        currentPage: 1,
        pageSize: 20,
        total: 0
      },
      // 后端在分页前算好的全量合计;balance 为 null 表示后端未提供(旧二进制),
      // 此时 computed 会回退到本页求和并在卡片上标注「(本页合计)」
      serverTotals: { balance: null, available: null, excluded: 0 },
      // 入金表单
      depositDialogVisible: false,
      depositForm: {
        user_id: '',
        amount: 0,
        method: '',
        remark: ''
      },
      depositRules: {
        amount: [{ required: true, message: '请输入入金金额', trigger: 'blur' }],
        method: [{ required: true, message: '请选择入金方式', trigger: 'change' }]
      },
      // 出金表单
      withdrawDialogVisible: false,
      withdrawForm: {
        user_id: '',
        amount: 0,
        method: '',
        bank_account: ''
      },
      withdrawRules: {
        amount: [{ required: true, message: '请输入出金金额', trigger: 'blur' }],
        method: [{ required: true, message: '请选择出金方式', trigger: 'change' }]
      },
      currentAccountAvailable: '0'
    }
  },
  computed: {
    totalAccounts() {
      return this.pagination.total
    },
    // ✨ 用后端在**分页之前**算好的全量合计,不要对当前页求和。@yutiansut @quantaxis
    //
    // 原实现 `this.accounts.reduce(...)` 只加了当前页的 20 行,却与旁边
    // 取自 `pagination.total` 的「总账户数 82」并排显示。子代理翻页实测:
    // 第 1 页 4,232,981,133.15、第 2 页 7,365,634,616.77,而账户数始终 82
    // —— 用户会当成 82 个账户的总额。
    //
    // 后端(`management.rs` 的 list_all_accounts)已在分页前求和,并**跳过非有限值**
    // (线上有 balance=1e300 的污染账户,float64 相加会吸收掉其余 81 个账户)。
    // 拿不到后端字段时回退到旧的本页求和 —— 后端未重编时不至于显示空白。
    totalBalance() {
      return this.serverTotals.balance != null
        ? this.serverTotals.balance
        : this.accounts.reduce((sum, acc) => sum + (Number(acc.balance) || 0), 0)
    },
    totalAvailable() {
      return this.serverTotals.available != null
        ? this.serverTotals.available
        : this.accounts.reduce((sum, acc) => sum + (Number(acc.available) || 0), 0)
    },
    // 合计口径提示:回退到本页求和、或后端排除了异常账户时,必须说清楚
    totalsNote() {
      if (this.serverTotals.balance == null) return '(本页合计)'
      if (this.serverTotals.excluded > 0) return `(已排除 ${this.serverTotals.excluded} 个异常账户)`
      return ''
    }
  },
  mounted() {
    this.fetchAccounts()
  },
  methods: {
    // 金额格式化 —— 统一两位小数。@yutiansut @quantaxis
    //
    // ⚠️ 原来这几个统计卡用的是裸 `toLocaleString()`(无 options),
    // 而**同文件下方表格**用的是带 `minimumFractionDigits: 2` 的版本。
    // 结果:1366×768 实测卡片显示 `总资金 4,232,972,133.148`(**三位小数**)、
    // `可用资金 4,156,965,285.352`,而正下方表格里同类金额是两位。
    // 375×667 下这还会让 `.stat-value` 溢出容器(scrollWidth 164px / 槽宽 48px)。
    fmtMoney (v) {
      if (v === null || v === undefined || v === '') return '--'
      const n = Number(v)
      if (!Number.isFinite(n)) return '--'
      // ✨ 有限但量级超限也要挡住 —— `Number.isFinite(1e300)` **是 true**。
      // @yutiansut @quantaxis
      //
      // 线上有个 `balance = 1e300` 的污染账户,渲染出来是 **404 个字符**
      // (301 位数字 + 100 个千分位逗号 + `.00`),把整列撑爆;
      // 排序时它还会霸占第一位,把真实的前两大账户挤到第 2、3。
      // 阈值取 1e15(`Number.MAX_SAFE_INTEGER` 的量级)——
      // 超过它的金额在本系统里必然是坏数据,不是业务值。
      if (Math.abs(n) >= MONEY_MAX) return '--'
      return n.toLocaleString('zh-CN', { minimumFractionDigits: 2, maximumFractionDigits: 2 })
    },
    // 注:本文件的 `fmtMoney` 与全局 `$money` 判据完全相同,保留是因为
    // `moneyTitle` 需要复用同一个 `MONEY_MAX` 常量做「为什么是 --」的说明。
    // 若哪天 `$money` 的阈值变了,这里必须同步 —— 已在 MONEY_MAX 的注释上标注。

    // 比率格式化 —— `|| 0` 会把 NaN 静默显示成 `0.00%`,
    // 而空账户的 `risk_ratio = margin/balance = 0/0 = NaN`
    // (见 memory「非有限浮点静默退化」)。@yutiansut @quantaxis
    fmtPct (v) {
      if (v === null || v === undefined || v === '') return '--'
      const n = Number(v)
      if (!Number.isFinite(n)) return '--'
      return (n * 100).toFixed(2) + '%'
    },

    // `--` 的原因要能被看到,否则用户不知道是「没有」还是「坏了」。
    // 挂在 title 上,配合 `show-overflow-tooltip` 之外的原生 tooltip。
    moneyTitle (v) {
      if (v === null || v === undefined || v === '') return '无数据'
      const n = Number(v)
      if (!Number.isFinite(n)) return `异常值:${String(v)}`
      if (Math.abs(n) >= MONEY_MAX) return `数值超出可显示范围(${n.toExponential(3)})`
      return ''
    },

      // ✨ 查询必须重置页码 —— 否则在第 3 页上改筛选条件,
      //    会带着 page=3 去请求新的结果集,拿到空表。 @yutiansut @quantaxis
    handleSearch() {
      this.pagination.currentPage = 1
      this.fetchAccounts()
    },
    // 列头排序 —— 必须回服务端重查,不能让 Element UI 就地排。@yutiansut @quantaxis
    //
    // ⚠️ 本表是**服务端分页**(total=82 / 每页 20),`:data="accounts"` 只有当前页。
    // 用默认 `sortable="custom"` 时 Element UI 只排这 20 行:实测点「账户ID ↑」后
    // 第一页 20 行里有 **15 行本不该出现在这里**(第1页 ∩ 全库最小20 = 5/20)。
    // 表面上排过序、实际顺序是错的 —— 比「不能排序」更坏,因为它不声张。
    // 故列改用 `sortable="custom"`(只发事件、不就地排),由后端
    // management.rs 的 sort_by/sort_order 在**分页之前**排全量。
    handleSortChange ({ prop, order }) {
      this.sort = { prop: order ? prop : '', order: order || '' }
      this.pagination.currentPage = 1   // 换排序等于换结果集,必须回第一页
      this.fetchAccounts()
    },

    async fetchAccounts() {
      this.loading = true
      try {
        this.loadError = null   // 重试先清除上次失败标记
        const params = {
          page: this.pagination.currentPage,
          page_size: this.pagination.pageSize,
          status: this.filters.status
        }

        // 如果设置了用户ID筛选，添加到查询参数
        if (this.filters.userId && this.filters.userId.trim()) {
          params.user_id = this.filters.userId.trim()
        }

        // 排序交给服务端 —— 见 handleSortChange 的说明
        if (this.sort.prop) {
          params.sort_by = this.sort.prop
          params.sort_order = this.sort.order === 'ascending' ? 'asc' : 'desc'
        }

        // axios 拦截器已处理响应，直接返回 data 内容 @yutiansut @quantaxis
        const data = await listAllAccounts(params)

        // ⚠️ **不要**在这里再按 user_id 过滤一遍。@yutiansut @quantaxis
        //
        // 原来这里有一段"备用方案"的前端过滤 `acc.user_id.includes(filter)`,
        // 但服务端返回的 `user_id` **等于 account_id**
        // (`management.rs:123` 是 `acc.account_cookie`),
        // 真正的用户 ID 在**另一个字段** `owner_user_id`。
        // ⇒ 按用户ID 筛选时,服务端已正确返回 9 行,前端这一步**全部滤光**。
        //
        // 回验代理实测:表格显示「暂无账户」/`vm.accounts.length = 0`,
        // 而分页条同时显示「共 9 条」/`pagination.total = 9`,`loadError` 为 null
        // —— 用户看到的是"查不到",实际是"查到了又被自己丢掉"。
        //
        // 服务端筛选是有效的(`management.rs:94` 声明了 `user_id`),直接用它的结果。
        const accounts = (data && data.accounts) || []

        this.accounts = accounts
        this.pagination.total = (data && data.total) || 0
        // 只在后端确实给了有限数值时才采用 —— 否则保持 null 走回退分支
        const tb = data && data.total_balance
        const ta = data && data.total_available
        this.serverTotals = {
          balance: Number.isFinite(tb) ? tb : null,
          available: Number.isFinite(ta) ? ta : null,
          excluded: (data && data.excluded_non_finite) || 0
        }
      } catch (error) {
        // ✨ 区分「加载失败」与「真的没数据」—— 两者原先在界面上完全同形,
        //    这是「无法显示」反馈的结构性来源。@yutiansut @quantaxis
        this.loadError = this.$errMsg(error, '加载失败')
        this.$message.error('获取账户列表失败: ' + error.message)
      } finally {
        this.loading = false
      }
    },
    resetFilters() {
      this.filters = {
        userId: '',
        status: ''
      }
      this.pagination.currentPage = 1
      this.fetchAccounts()
    },
    handleSizeChange(val) {
      this.pagination.pageSize = val
      this.pagination.currentPage = 1
      this.fetchAccounts()
    },
    handleCurrentChange(val) {
      this.pagination.currentPage = val
      this.fetchAccounts()
    },
    getRiskTagType(riskRatio) {
      if (riskRatio < 0.6) return 'success'
      if (riskRatio < 0.8) return 'warning'
      return 'danger'
    },
    formatTimestamp(timestamp) {
      const date = new Date(timestamp * 1000)
      return date.toLocaleString('zh-CN')
    },
    showAccountDetail(row) {
      // 路由表里注册的是 `account/:accountId`(router/index.js:118),
      // 没有 /admin/account-detail —— 原来跳这个路径直接白屏。
      // 参数用 account_id(后端新补的正确字段名),row.user_id 里装的
      // 其实也是 account_cookie,值一样但名字有歧义。@yutiansut @quantaxis
      this.$router.push({ path: `/account/${row.account_id || row.user_id}` })
    },
    showDepositDialog(row) {
      this.depositForm = {
        user_id: row.user_id,
        amount: 0,
        method: '',
        remark: ''
      }
      this.depositDialogVisible = true
    },
    showWithdrawDialog(row) {
      this.withdrawForm = {
        user_id: row.user_id,
        amount: 0,
        method: '',
        bank_account: ''
      }
      this.currentAccountAvailable = row.available.toString()
      this.withdrawDialogVisible = true
    },
    async handleDeposit() {
      this.$refs.depositFormRef.validate(async (valid) => {
        if (!valid) return

        this.submitting = true
        try {
          await managementDeposit(this.depositForm)
          this.$message.success('入金成功')
          this.depositDialogVisible = false
          this.fetchAccounts()
        } catch (error) {
          this.$message.error('入金失败: ' + error.message)
        } finally {
          this.submitting = false
        }
      })
    },
    async handleWithdraw() {
      this.$refs.withdrawFormRef.validate(async (valid) => {
        if (!valid) return

        this.submitting = true
        try {
          await managementWithdraw(this.withdrawForm)
          this.$message.success('出金成功')
          this.withdrawDialogVisible = false
          this.fetchAccounts()
        } catch (error) {
          this.$message.error('出金失败: ' + error.message)
        } finally {
          this.submitting = false
        }
      })
    }
  }
}
</script>

<style lang="scss" scoped>
// @yutiansut @quantaxis - 账户管理页面深色主题
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

.accounts-container {
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

.header-stats {
  display: flex;
  gap: 20px;
  /* flex 子项默认 min-width:auto,内容再长也不收缩 → 1024 宽下实测切掉 63px,
     `7,809,516,535.913` 渲染成 `7,809,516,535` 且无省略号。@yutiansut @quantaxis */
  margin-bottom: 20px;
}

.stat-card {
  min-width: 0;  /* 见 .header-stats 的说明 */
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

.stat-note {
  font-size: 11px;
  color: #909399;
}
.stat-value {
  font-size: clamp(16px, 1.6vw, 26px);
  font-weight: 700;
  color: $dark-text-primary;
  font-family: 'JetBrains Mono', monospace;
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

.table-container {
  // ⚠️ 本页表格**不在 el-card 里**,而在 .table-container 内,
  //    所以 `.qa-flex-page > .el-card > .el-card__body` 那条链匹配不上。
  //    在这里把它串进 flex 链(不新增全局规则)。@yutiansut @quantaxis
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
  background: $dark-bg-card;
  padding: 20px;
  border-radius: 8px;
  border: 1px solid $dark-border;

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
    }

    td.el-table__cell {
      background: $dark-bg-card !important;
      border-bottom: 1px solid $dark-border !important;
      color: $dark-text-primary !important;
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

    .el-table__fixed-right {
      th.el-table__cell,
      td.el-table__cell {
        background: $dark-bg-card !important;
      }
    }

    .el-table__empty-block {
      background: $dark-bg-card;
    }

    .el-table__empty-text {
      color: $dark-text-muted;
    }
  }
}

.pagination-container {
  margin-top: 20px;
  text-align: right;

  ::v-deep .el-pagination {
    .btn-prev,
    .btn-next,
    .el-pager li {
      background: $dark-bg-tertiary !important;
      color: $dark-text-secondary !important;
      border: none;

      &:hover {
        color: $primary-color !important;
      }

      &.active {
        background: $primary-color !important;
        color: white !important;
      }
    }

    .el-pagination__total,
    .el-pagination__jump {
      color: $dark-text-secondary;
    }

    .el-input__inner {
      background: $dark-bg-tertiary !important;
      border-color: $dark-border !important;
      color: $dark-text-primary !important;
    }
  }
}

.positive {
  color: $success-color !important;
  font-weight: 600;
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

  &--danger {
    background: rgba($danger-color, 0.15);
    color: $danger-color;
  }
}

// 对话框
::v-deep .el-dialog {
  background: $dark-bg-card;
  border: 1px solid $dark-border;
  border-radius: 12px;

  .el-dialog__header {
    background: $dark-bg-secondary;
    border-bottom: 1px solid $dark-border;
    border-radius: 12px 12px 0 0;

    .el-dialog__title {
      color: $dark-text-primary;
    }

    .el-dialog__headerbtn .el-dialog__close {
      color: $dark-text-secondary;
    }
  }

  .el-dialog__body {
    background: $dark-bg-card;
  }

  .el-dialog__footer {
    background: $dark-bg-secondary;
    border-top: 1px solid $dark-border;
    border-radius: 0 0 12px 12px;
  }

  .el-form-item__label {
    color: $dark-text-secondary;
  }

  .el-input__inner,
  .el-textarea__inner {
    background: $dark-bg-tertiary !important;
    border-color: $dark-border !important;
    color: $dark-text-primary !important;
  }

  .el-input-number {
    .el-input__inner {
      background: $dark-bg-tertiary !important;
      border-color: $dark-border !important;
      color: $dark-text-primary !important;
    }

    .el-input-number__decrease,
    .el-input-number__increase {
      background: $dark-bg-secondary;
      border-color: $dark-border;
      color: $dark-text-secondary;
    }
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

// 操作按钮
::v-deep .el-button--text {
  color: $primary-color;

  &:hover {
    color: lighten($primary-color, 10%);
  }
}
</style>
