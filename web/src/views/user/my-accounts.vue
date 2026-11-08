<template>
  <div class="my-accounts-container">
    <el-card class="header-card">
      <div slot="header" class="card-header">
        <h3>我的交易账户</h3>
        <el-button
          type="primary"
          icon="el-icon-plus"
          @click="showCreateDialog = true"
        >
          创建新账户
        </el-button>
      </div>

      <!-- 账户统计 -->
      <el-row :gutter="20" class="stats-row">
        <el-col :xs="24" :sm="12" :lg="6">
          <div class="stat-card">
            <div class="stat-label">账户总数</div>
            <div class="stat-value">{{ accounts.length }}</div>
          </div>
        </el-col>
        <el-col :xs="24" :sm="12" :lg="6">
          <div class="stat-card">
            <div class="stat-label">总资产</div>
            <div class="stat-value">{{ $num(totalBalance, 2) }}</div>
          </div>
        </el-col>
        <el-col :xs="24" :sm="12" :lg="6">
          <div class="stat-card">
            <div class="stat-label">可用资金</div>
            <div class="stat-value">{{ $num(totalAvailable, 2) }}</div>
          </div>
        </el-col>
        <el-col :xs="24" :sm="12" :lg="6">
          <div class="stat-card">
            <div class="stat-label">占用保证金</div>
            <div class="stat-value">{{ $num(totalMargin, 2) }}</div>
          </div>
        </el-col>
      </el-row>
    </el-card>

    <!-- 账户列表 -->
    <el-card class="accounts-card">
      <div v-loading="loading">
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
        <el-table
          :empty-text="$emptyText(loadError, '暂无账户')"
          :data="accounts"
          style="width: 100%"
          :default-sort="{ prop: 'created_at', order: 'descending' }"
        >
          <el-table-column
            prop="account_id"
            label="账户ID"
            min-width="280"
            show-overflow-tooltip
          >
            <template slot-scope="scope">
              <el-tag size="small" type="info">{{ scope.row.account_id }}</el-tag>
            </template>
          </el-table-column>

          <el-table-column
            prop="account_name"
            label="账户名称"
            min-width="150"
          />

          <el-table-column
            prop="account_type"
            label="账户类型"
            min-width="120"
          >
            <template slot-scope="scope">
              <el-tag
                :type="getAccountTypeTag(scope.row.account_type)"
                size="small"
              >
                {{ getAccountTypeLabel(scope.row.account_type) }}
              </el-tag>
            </template>
          </el-table-column>

          <el-table-column
            prop="balance"
            label="总资产"
            min-width="130"
            align="right" show-overflow-tooltip>
            <template slot-scope="scope">
              <span class="money">{{ $num(scope.row.balance, 2) }}</span>
            </template>
          </el-table-column>

          <el-table-column
            prop="available"
            label="可用资金"
            min-width="130"
            align="right" show-overflow-tooltip>
            <template slot-scope="scope">
              <span class="money">{{ $num(scope.row.available, 2) }}</span>
            </template>
          </el-table-column>

          <el-table-column
            prop="margin"
            label="占用保证金"
            min-width="130"
            align="right" show-overflow-tooltip>
            <template slot-scope="scope">
              <span class="money">{{ $num(scope.row.margin, 2) }}</span>
            </template>
          </el-table-column>

          <el-table-column
            prop="risk_ratio"
            label="风险率"
            min-width="120"
            align="center"
          >
            <template slot-scope="scope">
              <el-tag
                :type="getRiskRatioTag(scope.row.risk_ratio)"
                size="small"
              >
                {{ $pct(scope.row.risk_ratio, 1) }}
              </el-tag>
            </template>
          </el-table-column>

          <el-table-column
            prop="created_at"
            label="创建时间"
            min-width="180"
          >
            <template slot-scope="scope">
              {{ formatTimestamp(scope.row.created_at) }}
            </template>
          </el-table-column>

          <el-table-column
            label="操作"
            width="180"
          >
            <template slot-scope="scope">
              <el-button
                type="text"
                size="small"
                @click="handleViewAccount(scope.row)"
              >
                查看详情
              </el-button>
              <el-button
                type="text"
                size="small"
                @click="handleDeposit(scope.row)"
              >
                入金
              </el-button>
              <el-button
                type="text"
                size="small"
                @click="handleWithdraw(scope.row)"
              >
                出金
              </el-button>
            </template>
          </el-table-column>
        </el-table>

        <div v-if="accounts.length === 0" class="empty-state">
          <el-empty description="暂无交易账户">
            <el-button
              type="primary"
              @click="showCreateDialog = true"
            >
              创建第一个账户
            </el-button>
          </el-empty>
        </div>
      </div>
    </el-card>

    <!-- 创建账户对话框 -->
    <el-dialog
      title="创建交易账户"
      :visible.sync="showCreateDialog"
      width="500px"
      :close-on-click-modal="false"
    >
      <el-form
        ref="createForm"
        :model="createForm"
        :rules="createRules"
        label-width="100px"
      >
        <el-form-item label="账户名称" prop="account_name">
          <el-input
            v-model="createForm.account_name"
            placeholder="请输入账户名称"
            clearable
          />
        </el-form-item>

        <el-form-item label="账户类型" prop="account_type">
          <el-select
            v-model="createForm.account_type"
            placeholder="请选择账户类型"
            style="width: 100%"
          >
            <el-option label="个人账户" value="individual" />
            <el-option label="机构账户" value="institutional" />
            <el-option label="做市商账户" value="market_maker" />
          </el-select>
        </el-form-item>

        <el-form-item label="初始资金" prop="init_cash">
          <el-input-number
            v-model="createForm.init_cash"
            :min="10000"
            :max="100000000"
            :step="10000"
            style="width: 100%"
          />
          <div class="form-tip">最低 10,000 元</div>
        </el-form-item>
      </el-form>

      <div slot="footer">
        <el-button @click="showCreateDialog = false">取消</el-button>
        <el-button
          type="primary"
          :loading="creating"
          @click="handleCreateAccount"
        >
          创建
        </el-button>
      </div>
    </el-dialog>

    <!-- 入金对话框 -->
    <el-dialog
      title="账户入金"
      :visible.sync="showDepositDialog"
      width="400px"
    >
      <el-form ref="depositForm" :model="depositForm" :rules="depositRules" label-width="80px">
        <el-form-item label="账户">
          <el-input :value="currentAccount.account_name" disabled />
        </el-form-item>
        <el-form-item label="入金金额" prop="amount">
          <el-input-number
            v-model="depositForm.amount"
            :min="100"
            :step="100"
            style="width: 100%"
          />
        </el-form-item>
      </el-form>
      <div slot="footer">
        <el-button @click="showDepositDialog = false">取消</el-button>
        <el-button type="primary" :loading="submittingCapital"
                     :disabled="submittingCapital" @click="handleDepositConfirm">确定</el-button>
      </div>
    </el-dialog>

    <!-- 出金对话框 -->
    <el-dialog
      title="账户出金"
      :visible.sync="showWithdrawDialog"
      width="400px"
    >
      <el-form ref="withdrawForm" :model="withdrawForm" :rules="withdrawRules" label-width="80px">
        <el-form-item label="账户">
          <el-input :value="currentAccount.account_name" disabled />
        </el-form-item>
        <el-form-item label="可用资金">
          <el-input :value="currentAccount.available" disabled />
        </el-form-item>
        <el-form-item label="出金金额" prop="amount">
          <el-input-number
            v-model="withdrawForm.amount"
            :min="100"
            :max="currentAccount.available"
            :step="100"
            style="width: 100%"
          />
        </el-form-item>
      </el-form>
      <div slot="footer">
        <el-button @click="showWithdrawDialog = false">取消</el-button>
        <el-button type="primary" :loading="submittingCapital"
                     :disabled="submittingCapital" @click="handleWithdrawConfirm">确定</el-button>
      </div>
    </el-dialog>
  </div>
</template>

<script>
import { getUserAccounts, createUserAccount, deposit, withdraw } from '@/api'
import { mapGetters } from 'vuex'

export default {
  name: 'MyAccounts',

  data() {
    return {
      // 加载失败原因(null = 未失败)
      loadError: null,
      loading: false,
      accounts: [],

      // 创建账户
      showCreateDialog: false,
      creating: false,
      createForm: {
        account_name: '',
        account_type: 'individual',
        init_cash: 100000
      },
      createRules: {
        account_name: [
          { required: true, message: '请输入账户名称', trigger: 'blur' },
          { min: 2, max: 50, message: '长度在 2 到 50 个字符', trigger: 'blur' }
        ],
        account_type: [
          { required: true, message: '请选择账户类型', trigger: 'change' }
        ],
        init_cash: [
          { required: true, message: '请输入初始资金', trigger: 'blur' }
        ]
      },

      // 入金/出金
      showDepositDialog: false,
      showWithdrawDialog: false,
      // ⚠️ 提交中标志 —— 防重复提交。@yutiansut @quantaxis
      //
      // 回验代理实测:入金/出金**没有任何 in-flight 守卫**,
      // 对话框按钮也没有 :loading/:disabled(实测 disabled=false)。
      // **300ms 内点 3 次确定 → 发出 3 笔入金请求**(42/374/707ms)。
      // 出金同形。这是直接的资金风险。
      submittingCapital: false,
      currentAccount: {},
      depositForm: {
        amount: 10000
      },
      withdrawForm: {
        amount: 1000
      },
      // ✨ 补前端校验：清空金额框 el-input-number 写 undefined → JSON 丢键 → 后端 400
      // `missing field amount`。@yutiansut @quantaxis
      depositRules: {
        amount: [{ required: true, message: '请输入入金金额', trigger: 'blur' }]
      },
      withdrawRules: {
        amount: [{ required: true, message: '请输入出金金额', trigger: 'blur' }]
      }
    }
  },

  computed: {
    ...mapGetters(['currentUser']),

    totalBalance() {
      return this.accounts.reduce((sum, acc) => sum + (Number(acc.balance) || 0), 0)
    },

    totalAvailable() {
      return this.accounts.reduce((sum, acc) => sum + (Number(acc.available) || 0), 0)
    },

    totalMargin() {
      return this.accounts.reduce((sum, acc) => sum + (Number(acc.margin) || 0), 0)
    }
  },

  mounted() {
    this.fetchAccounts()
  },

  methods: {
    async fetchAccounts() {
      if (!this.currentUser) {
        this.$message.error('请先登录')
        return
      }

      this.loading = true
      try {
        this.loadError = null   // 重试先清除上次失败标记
        const res = await getUserAccounts(this.currentUser)
        this.accounts = res.accounts || []
      } catch (error) {
        // ✨ 区分「加载失败」与「真的没数据」—— 两者原先在界面上完全同形,
        //    这是「无法显示」反馈的结构性来源。@yutiansut @quantaxis
        this.loadError = this.$errMsg(error, '加载失败')
        this.$message.error('获取账户列表失败：' + (this.$errMsg(error, '未知错误')))
      } finally {
        this.loading = false
      }
    },

    async handleCreateAccount() {
      this.$refs.createForm.validate(async (valid) => {
        if (!valid) return

        this.creating = true
        try {
          await createUserAccount(this.currentUser, this.createForm)
          this.$message.success('账户创建成功')
          this.showCreateDialog = false
          this.resetCreateForm()
          await this.fetchAccounts()
        } catch (error) {
          this.$message.error('创建账户失败：' + (this.$errMsg(error, '未知错误')))
        } finally {
          this.creating = false
        }
      })
    },

    resetCreateForm() {
      this.createForm = {
        account_name: '',
        account_type: 'individual',
        init_cash: 100000
      }
      this.$refs.createForm && this.$refs.createForm.clearValidate()
    },

    handleViewAccount(account) {
      this.$router.push({
        name: 'AccountDetail',
        params: { accountId: account.account_id }
      })
    },

    handleDeposit(account) {
      this.currentAccount = account
      this.depositForm.amount = 10000
      this.showDepositDialog = true
    },

    async handleDepositConfirm() {
      // ✨ 先过表单校验（空金额在此拦下，不再让畸形请求打到后端换 400）。@yutiansut @quantaxis
      const valid = await new Promise(res => this.$refs.depositForm.validate(v => res(v)))
      if (!valid) return
      // 防重复提交:请求未回来之前直接忽略后续点击。@yutiansut @quantaxis
      if (this.submittingCapital) return
      this.submittingCapital = true
      try {
        // ✨ 字段名叫 user_id,但后端要的是 **account_id** @yutiansut @quantaxis
        //
        // handlers.rs:789-802 的逻辑:值不以 `ACC_` 开头就按 user 反查账户,
        // 该用户有多个账户时直接报错要求指定 account_id。
        // 原先传登录 UUID(this.currentUser),对任何拥有 ≥2 个账户的真实用户
        // 必然失败。同项目 accounts/index.vue:335-337 已是正确写法,还带注释。
        await deposit({
          user_id: (this.currentAccount && this.currentAccount.account_id) || this.currentUser,
          amount: this.depositForm.amount
        })
        this.$message.success('入金成功')
        this.showDepositDialog = false
        await this.fetchAccounts()
      } catch (error) {
        this.$message.error('入金失败：' + (this.$errMsg(error, '未知错误')))
      } finally {
        // 必须在 finally 复位 —— 只在成功路径复位的话,一次失败后
        // 用户就再也提交不了了。@yutiansut @quantaxis
        this.submittingCapital = false
      }
    },

    handleWithdraw(account) {
      this.currentAccount = account
      this.withdrawForm.amount = 1000
      this.showWithdrawDialog = true
    },

    async handleWithdrawConfirm() {
      // ✨ 先过表单校验。@yutiansut @quantaxis
      const valid = await new Promise(res => this.$refs.withdrawForm.validate(v => res(v)))
      if (!valid) return
      // 防重复提交:请求未回来之前直接忽略后续点击。@yutiansut @quantaxis
      if (this.submittingCapital) return
      this.submittingCapital = true
      try {
        // ✨ 同上:传 account_id 而非登录 UUID @yutiansut @quantaxis
        await withdraw({
          user_id: (this.currentAccount && this.currentAccount.account_id) || this.currentUser,
          amount: this.withdrawForm.amount
        })
        this.$message.success('出金成功')
        this.showWithdrawDialog = false
        await this.fetchAccounts()
      } catch (error) {
        this.$message.error('出金失败：' + (this.$errMsg(error, '未知错误')))
      } finally {
        this.submittingCapital = false
      }
    },

    getAccountTypeLabel(type) {
      const labels = {
        'Individual': '个人',
        'Institutional': '机构',
        'MarketMaker': '做市商'
      }
      return labels[type] || type
    },

    getAccountTypeTag(type) {
      const tags = {
        'Individual': 'success',
        'Institutional': 'warning',
        'MarketMaker': 'danger'
      }
      return tags[type] || ''
    },

    getRiskRatioTag(ratio) {
      if (ratio >= 0.8) return 'danger'
      if (ratio >= 0.5) return 'warning'
      return 'success'
    },

    formatTimestamp(timestamp) {
      if (!timestamp) return '-'
      const date = new Date(timestamp * 1000)
      return date.toLocaleString('zh-CN')
    }
  }
}
</script>

<style lang="scss" scoped>
// @yutiansut @quantaxis - 深色主题样式
$dark-bg-primary: #0d1117;
$dark-bg-secondary: #161b22;
$dark-bg-card: #1c2128;
$dark-bg-tertiary: #21262d;
$dark-border: #30363d;
$dark-text-primary: #f0f6fc;
$dark-text-secondary: #8b949e;
$dark-text-muted: #6e7681;
$primary-color: #1890ff;

.my-accounts-container {
  padding: 20px;
  background: $dark-bg-primary;
  // ⚠️ 用 --qa-content-h,不要手写 calc(100vh - N)。@yutiansut @quantaxis
  //    原值 -60px 少算了 content-wrapper 的上下 padding(各 20px):
  //    真实 chrome = top-header 56 + padding 40 = 96px(有公告条时再 +40)。
  //    实测本页在无内容可滚时仍有 41px 的死滚动条
  //    (.content-wrapper 的 scrollHeight - clientHeight = 41)。
  //    `layout/index.vue:546` 早就写明「各页面请一律用 var(--qa-content-h)」。
  min-height: var(--qa-content-h, calc(100vh - 96px));
}

.header-card {
  margin-bottom: 20px;
  background: $dark-bg-card !important;
  border: 1px solid $dark-border !important;

  ::v-deep .el-card__header {
    background: $dark-bg-secondary !important;
    border-bottom: 1px solid $dark-border !important;
    padding: 15px 20px !important;
  }

  ::v-deep .el-card__body {
    background: $dark-bg-card !important;
  }
}

.card-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.card-header h3 {
  margin: 0;
  font-size: 18px;
  font-weight: 600;
  color: $dark-text-primary !important;
}

.stats-row {
  margin-top: 20px;
}

.stat-card {
  text-align: center;
  padding: 20px;
  background: $dark-bg-tertiary !important;
  border-radius: 8px;
  border: 1px solid $dark-border;
}

.stat-label {
  font-size: 14px;
  color: $dark-text-secondary !important;
  margin-bottom: 8px;
}

.stat-value {
  font-size: 24px;
  font-weight: 600;
  color: $dark-text-primary !important;
  font-family: 'JetBrains Mono', monospace;
}

.accounts-card {
  margin-top: 20px;
  background: $dark-bg-card !important;
  border: 1px solid $dark-border !important;

  ::v-deep .el-card__body {
    background: $dark-bg-card !important;
    padding: 20px !important;
  }

  // 表格样式
  ::v-deep .el-table {
    background: transparent !important;
    color: $dark-text-primary !important;

    &::before {
      background-color: $dark-border !important;
    }

    // 表头
    th.el-table__cell {
      background: $dark-bg-secondary !important;
      color: $dark-text-secondary !important;
      border-bottom: 1px solid $dark-border !important;
      font-weight: 600;
    }

    // 表格行
    tr {
      background: $dark-bg-card !important;
    }

    // 单元格
    td.el-table__cell {
      background: $dark-bg-card !important;
      color: $dark-text-primary !important;
      border-bottom: 1px solid $dark-border !important;
    }

    // hover 行
    .el-table__row:hover > td.el-table__cell {
      background: $dark-bg-tertiary !important;
    }

    // 固定列
    .el-table__fixed,
    .el-table__fixed-right {
      background: $dark-bg-card !important;

      &::before {
        background-color: $dark-border !important;
      }

      .el-table__fixed-body-wrapper {
        background: $dark-bg-card !important;
      }
    }

    // 空数据
    .el-table__empty-block {
      background: $dark-bg-card !important;
    }

    .el-table__empty-text {
      color: $dark-text-muted !important;
    }
  }

  // 文本链接按钮
  ::v-deep .el-button--text {
    color: $primary-color !important;

    &:hover {
      color: lighten($primary-color, 15%) !important;
    }
  }
}

.money {
  font-family: 'JetBrains Mono', 'Monaco', monospace;
  font-weight: 500;
  color: $dark-text-primary !important;
}

.empty-state {
  padding: 40px 0;
  text-align: center;

  ::v-deep .el-empty__description p {
    color: $dark-text-muted !important;
  }
}

.form-tip {
  font-size: 12px;
  color: $dark-text-muted !important;
  margin-top: 5px;
}

// 标签样式
::v-deep .el-tag {
  border: none !important;

  &.el-tag--info {
    background: rgba($primary-color, 0.15) !important;
    color: $primary-color !important;
  }

  &.el-tag--success {
    background: rgba(82, 196, 26, 0.15) !important;
    color: #52c41a !important;
  }

  &.el-tag--warning {
    background: rgba(250, 173, 20, 0.15) !important;
    color: #faad14 !important;
  }

  &.el-tag--danger {
    background: rgba(245, 34, 45, 0.15) !important;
    color: #f5222d !important;
  }
}

// 对话框样式
::v-deep .el-dialog {
  background: $dark-bg-card !important;
  border: 1px solid $dark-border !important;
  border-radius: 8px !important;

  .el-dialog__header {
    background: $dark-bg-secondary !important;
    border-bottom: 1px solid $dark-border !important;
    padding: 15px 20px !important;
    border-radius: 8px 8px 0 0 !important;

    .el-dialog__title {
      color: $dark-text-primary !important;
    }

    .el-dialog__headerbtn .el-dialog__close {
      color: $dark-text-secondary !important;

      &:hover {
        color: $dark-text-primary !important;
      }
    }
  }

  .el-dialog__body {
    background: $dark-bg-card !important;
    padding: 20px !important;
  }

  .el-dialog__footer {
    background: $dark-bg-card !important;
    border-top: 1px solid $dark-border !important;
    padding: 15px 20px !important;
  }
}

// 表单样式
::v-deep .el-form {
  .el-form-item__label {
    color: $dark-text-secondary !important;
  }

  .el-input__inner {
    background: $dark-bg-tertiary !important;
    border-color: $dark-border !important;
    color: $dark-text-primary !important;

    &::placeholder {
      color: $dark-text-muted !important;
    }

    &:focus {
      border-color: $primary-color !important;
    }

    &[disabled] {
      background: $dark-bg-secondary !important;
      color: $dark-text-muted !important;
    }
  }

  // 数字输入框
  .el-input-number {
    .el-input__inner {
      text-align: left;
    }

    .el-input-number__decrease,
    .el-input-number__increase {
      background: $dark-bg-tertiary !important;
      border-color: $dark-border !important;
      color: $dark-text-secondary !important;

      &:hover {
        color: $primary-color !important;
      }
    }
  }
}

// 下拉选择框
::v-deep .el-select-dropdown {
  background: $dark-bg-secondary !important;
  border-color: $dark-border !important;

  .el-select-dropdown__item {
    color: $dark-text-primary !important;

    &:hover, &.hover {
      background: $dark-bg-tertiary !important;
    }

    &.selected {
      color: $primary-color !important;
      font-weight: 600;
    }
  }
}

// 按钮样式
::v-deep .el-button {
  &.el-button--default {
    background: $dark-bg-tertiary !important;
    border-color: $dark-border !important;
    color: $dark-text-primary !important;

    &:hover {
      border-color: $primary-color !important;
      color: $primary-color !important;
    }
  }
}
</style>
