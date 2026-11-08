<template>
  <div class="accounts-page">
    <el-card>
      <div slot="header" class="card-header">
        <span>账户管理</span>
        <el-button type="primary" size="small" @click="showOpenDialog">
          <i class="el-icon-plus"></i> 开户
        </el-button>
      </div>

      <el-form :inline="true" size="small">
        <el-form-item label="用户ID">
          <el-input v-model="queryForm.userId" placeholder="输入用户ID" clearable />
        </el-form-item>
        <el-form-item>
          <el-button type="primary" @click="handleQuery">查询</el-button>
          <el-button @click="handleReset">重置</el-button>
        </el-form-item>
      </el-form>

      <!-- ✨ 表格高度改为跟随视口, 不再写死 500px @yutiansut @quantaxis
           实测 1366x768: 表格上沿距内容区顶部 161px
           var(--qa-content-h) 由 layout/index.vue 统一定义 = 100vh - 56(顶栏) - 40(padding),
           有公告条时自动再减 40px; max(260px, ...) 是极短视口下的兜底 -->
      <!-- 刷新失败但表里还留着上一次的数据 —— 必须持久提示,不能只弹一条 3 秒的 toast。
           交易系统里「陈旧余额 + 无提示」比空表更危险:用户会照着过期数字做决策。
           清空列表也不对(那等于丢掉唯一能看的数据),所以保留数据 + 明示状态。
           @yutiansut @quantaxis -->
      <el-alert
        v-if="loadError && accountList.length > 0"
        :title="'刷新失败，下方为上次成功加载的数据：' + loadError"
        type="warning"
        :closable="false"
        show-icon
        style="margin-bottom: 12px;"
      />
      <el-table
        :empty-text="$emptyText(loadError, '暂无账户')"
        :data="accountList"
        border
        stripe
        :height="'max(260px, calc(var(--qa-content-h, calc(100vh - 96px)) - 202px))'"
        v-loading="loading"
        style="width: 100%"
      >
        <el-table-column prop="account_id" label="账户ID" min-width="200" show-overflow-tooltip />
        <el-table-column prop="account_name" label="账户名称" min-width="150" />
        <el-table-column prop="account_type" label="账户类型" min-width="100">
          <template slot-scope="{ row }">{{ getAccountTypeLabel(row.account_type) }}</template>
        </el-table-column>
        <!-- show-overflow-tooltip:该列实测被 ellipsis 截断且无提示,hover 也弹不出全文。
             不加宽是因为这张表在 1280 下已横向溢出,加宽会加剧;
             tooltip 让内容 hover 可达而不增加列宽。@yutiansut @quantaxis -->
        <el-table-column prop="balance" label="总权益" min-width="120" align="right" show-overflow-tooltip>
          <template slot-scope="scope">
            ¥{{ formatNumber(scope.row.balance) }}
          </template>
        </el-table-column>
        <el-table-column prop="available" label="可用资金" min-width="120" align="right" show-overflow-tooltip>
          <template slot-scope="scope">
            ¥{{ formatNumber(scope.row.available) }}
          </template>
        </el-table-column>
        <el-table-column prop="margin" label="保证金" min-width="120" align="right" show-overflow-tooltip>
          <template slot-scope="scope">
            ¥{{ formatNumber(scope.row.margin) }}
          </template>
        </el-table-column>
        <el-table-column prop="risk_ratio" label="风险率" min-width="100" align="right" show-overflow-tooltip>
          <template slot-scope="scope">
            <el-tag :type="getRiskType(scope.row.risk_ratio)" size="mini">
              {{ $pct(scope.row.risk_ratio, 1) }}
            </el-tag>
          </template>
        </el-table-column>
        <el-table-column label="操作" width="200">
          <template slot-scope="scope">
            <el-button type="text" size="small" @click="handleDeposit(scope.row)">
              存款
            </el-button>
            <el-button type="text" size="small" @click="handleWithdraw(scope.row)">
              取款
            </el-button>
            <el-button type="text" size="small" @click="handleViewDetail(scope.row)">
              详情
            </el-button>
          </template>
        </el-table-column>
      </el-table>
    </el-card>

    <!-- 开户对话框 -->
    <el-dialog title="开户" :visible.sync="openDialogVisible" width="500px">
      <el-form :model="openForm" :rules="openRules" ref="openForm" label-width="100px">
        <!-- 管理员模式：显示用户选择下拉框 -->
        <el-form-item v-if="isAdmin" label="选择用户" prop="user_id">
          <el-select
            v-model="openForm.user_id"
            filterable
            placeholder="请选择用户"
            style="width: 100%"
            @change="handleUserChange"
          >
            <el-option
              v-for="user in userList"
              :key="user.user_id"
              :label="`${user.username} (${user.real_name || '未实名'})`"
              :value="user.user_id"
            >
              <span style="float: left">{{ user.username }}</span>
              <span style="float: right; color: #8492a6; font-size: 13px">
                {{ user.real_name || user.user_id.slice(0, 8) }}
              </span>
            </el-option>
          </el-select>
        </el-form-item>

        <!-- 普通用户模式：显示当前用户信息（只读） -->
        <el-form-item v-else label="用户">
          <el-input :value="userInfo ? userInfo.username : currentUser" disabled />
        </el-form-item>

        <el-form-item label="用户名称" prop="user_name">
          <el-input v-model="openForm.user_name" placeholder="请输入用户名称" />
        </el-form-item>
        <el-form-item label="初始资金" prop="init_cash">
          <el-input-number
            v-model="openForm.init_cash"
            :min="0"
            :step="10000"
            style="width: 100%"
          />
        </el-form-item>
        <el-form-item label="账户类型" prop="account_type">
          <el-select v-model="openForm.account_type" style="width: 100%">
            <el-option label="个人账户" value="individual" />
            <el-option label="机构账户" value="institutional" />
          </el-select>
        </el-form-item>
        <el-form-item label="密码" prop="password">
          <el-input v-model="openForm.password" type="password" placeholder="请输入密码" />
        </el-form-item>
      </el-form>
      <div slot="footer">
        <el-button @click="openDialogVisible = false">取消</el-button>
        <el-button type="primary" @click="handleOpenAccount" :loading="submitting">
          确定
        </el-button>
      </div>
    </el-dialog>

    <!-- 存款对话框 -->
    <el-dialog title="存款" :visible.sync="depositDialogVisible" width="400px">
      <el-form :model="depositForm" :rules="depositRules" ref="depositForm" label-width="80px">
        <el-form-item label="账户ID">
          <el-input v-model="depositForm.user_id" disabled />
        </el-form-item>
        <el-form-item label="存款金额" prop="amount">
          <el-input-number
            v-model="depositForm.amount"
            :min="0"
            :step="1000"
            style="width: 100%"
          />
        </el-form-item>
      </el-form>
      <div slot="footer">
        <el-button @click="depositDialogVisible = false">取消</el-button>
        <el-button type="primary" @click="handleDepositSubmit" :loading="submitting">
          确定
        </el-button>
      </div>
    </el-dialog>

    <!-- 取款对话框 -->
    <el-dialog title="取款" :visible.sync="withdrawDialogVisible" width="400px">
      <el-form :model="withdrawForm" :rules="withdrawRules" ref="withdrawForm" label-width="80px">
        <el-form-item label="账户ID">
          <el-input v-model="withdrawForm.user_id" disabled />
        </el-form-item>
        <el-form-item label="取款金额" prop="amount">
          <el-input-number
            v-model="withdrawForm.amount"
            :min="0"
            :step="1000"
            style="width: 100%"
          />
        </el-form-item>
      </el-form>
      <div slot="footer">
        <el-button @click="withdrawDialogVisible = false">取消</el-button>
        <el-button type="primary" @click="handleWithdrawSubmit" :loading="submitting">
          确定
        </el-button>
      </div>
    </el-dialog>
  </div>
</template>

<script>
import { openAccount, queryAccount, deposit, withdraw, listUsers, getUserAccounts } from '@/api'
import { extractErrorMessage } from '@/api/request'
import { mapGetters } from 'vuex'

export default {
  name: 'Accounts',
  computed: {
    ...mapGetters(['currentUser', 'isAdmin', 'userInfo'])
  },
  data() {
    return {
      // 加载失败原因(null = 未失败)
      loadError: null,
      loading: false,
      submitting: false,
      accountList: [],
      queryForm: {
        userId: ''
      },
      openDialogVisible: false,
      openForm: {
        user_id: '',
        user_name: '',
        init_cash: 1000000,
        account_type: 'individual',
        password: ''
      },
      openRules: {
        user_id: [{ required: true, message: '请选择用户', trigger: 'change' }],
        user_name: [{ required: true, message: '请输入用户名称', trigger: 'blur' }],
        init_cash: [{ required: true, message: '请输入初始资金', trigger: 'blur' }],
        account_type: [{ required: true, message: '请选择账户类型', trigger: 'change' }],
        password: [{ required: true, message: '请输入密码', trigger: 'blur' }]
      },
      depositDialogVisible: false,
      depositForm: {
        user_id: '',
        amount: 0
      },
      // ✨ 补前端校验：清空金额框时 el-input-number 会写 undefined，JSON 丢键 → 后端 400
      // `missing field amount`。与管理端 admin/accounts.vue 的 depositRules 同构。@yutiansut @quantaxis
      depositRules: {
        amount: [{ required: true, message: '请输入存款金额', trigger: 'blur' }]
      },
      withdrawDialogVisible: false,
      withdrawForm: {
        user_id: '',
        amount: 0
      },
      withdrawRules: {
        amount: [{ required: true, message: '请输入取款金额', trigger: 'blur' }]
      },
      userList: []  // 用户列表（管理员使用）
    }
  },
  mounted() {
    this.loadAccounts()
  },
  methods: {
      // ✨ 委托全局 `$money`(main.js):判空 → isFinite → **量级上限 1e15**。
      // 全仓曾有 13 个各写各的金额格式化器,**没有一个带量级闸**,
      // 于是 `balance = 1e300` 的污染账户在 6 个页面渲染成 404 个字符
      // (`Number.isFinite(1e300)` 为真,只判 isFinite 挡不住)。@yutiansut @quantaxis
    formatNumber(num) {
      return this.$money(num)
    },

    getRiskType(ratio) {
      if (ratio >= 0.8) return 'danger'
      if (ratio >= 0.5) return 'warning'
      return 'success'
    },

    async loadAccounts() {
      if (!this.currentUser) {
        this.$message.warning('请先登录')
        return
      }

      this.loading = true
      try {
        this.loadError = null   // 重试先清除上次失败标记
        const res = await getUserAccounts(this.currentUser)
        this.accountList = res.accounts || []
        this.loading = false
      } catch (error) {
        // ✨ 区分「加载失败」与「真的没数据」—— 两者原先在界面上完全同形,
        //    这是「无法显示」反馈的结构性来源。@yutiansut @quantaxis
        this.loadError = this.$errMsg(error, '加载失败')
        this.$message.error('加载账户信息失败: ' + extractErrorMessage(error))
        this.loading = false
      }
    },

    handleQuery() {
      if (this.queryForm.userId) {
        this.loading = true
        getUserAccounts(this.queryForm.userId)
          .then(res => {
            this.accountList = res.accounts || []
          })
          .catch(err => {
            // ⚠️ 同 orders/index.vue:417 —— 只弹 toast,不置 loadError。
            // 本页 :30 已有「刷新失败,下方为上次成功加载的数据」横幅,
            // 正是为这种情况准备的,但这条路径从不触发它。@yutiansut @quantaxis
            // 保留 accountList(横幅会说明数据是旧的),比清空更有用。
            this.loadError = extractErrorMessage(err)
            this.$message.error('查询失败: ' + this.loadError)
          })
          .finally(() => {
            this.loading = false
          })
      } else {
        this.loadAccounts()
      }
    },

    handleReset() {
      this.queryForm.userId = ''
      this.loadAccounts()
    },

    async showOpenDialog() {
      this.openDialogVisible = true

      // 如果是管理员，加载用户列表
      if (this.isAdmin) {
        try {
          // axios 拦截器已处理响应 @yutiansut @quantaxis
          const data = await listUsers()
          this.userList = (data && data.users) || []
        } catch (error) {
          this.$message.error('加载用户列表失败: ' + (this.$errMsg(error, '未知错误')))
        }
      } else {
        // 普通用户自动填充当前用户ID
        this.openForm.user_id = this.currentUser
      }

      this.$nextTick(() => {
        this.$refs.openForm.resetFields()
        // 重新设置用户ID（因为resetFields会清空）
        if (!this.isAdmin) {
          this.openForm.user_id = this.currentUser
        }
      })
    },

    handleUserChange(userId) {
      // 管理员选择用户后，可以自动填充一些信息
      const selectedUser = this.userList.find(u => u.user_id === userId)
      if (selectedUser && selectedUser.username) {
        this.openForm.user_name = selectedUser.username
      }
    },

    handleOpenAccount() {
      this.$refs.openForm.validate(valid => {
        if (valid) {
          this.submitting = true
          openAccount(this.openForm)
            .then(() => {
              this.$message.success('开户成功')
              this.openDialogVisible = false
              this.loadAccounts()
            })
            .catch(() => {
              this.$message.error('开户失败')
            })
            .finally(() => {
              this.submitting = false
            })
        }
      })
    },

    handleDeposit(row) {
      // 注意：后端API字段名是user_id，但实际传的是account_id
      this.depositForm.user_id = row.account_id
      this.depositForm.amount = 10000
      this.depositDialogVisible = true
    },

    // ✨ 账户类型本地化(与 my-accounts:516 同一映射,/accounts 原来直出裸英文枚举)。@yutiansut @quantaxis
    getAccountTypeLabel(type) {
      const labels = { 'Individual': '个人', 'Institutional': '机构', 'MarketMaker': '做市商' }
      return labels[type] || type
    },

    handleDepositSubmit() {
      this.$refs.depositForm.validate(valid => {
        if (!valid) return
        this.submitting = true
        deposit(this.depositForm)
          .then(() => {
            this.$message.success('存款成功')
            this.depositDialogVisible = false
            this.loadAccounts()
          })
          .catch(err => {
            // ✨ 透出后端原因（如「金额必须大于 0」），别再吞成光秃秃的「存款失败」。@yutiansut @quantaxis
            this.$message.error(extractErrorMessage(err, '存款失败'))
          })
          .finally(() => {
            this.submitting = false
          })
      })
    },

    handleWithdraw(row) {
      // 注意：后端API字段名是user_id，但实际传的是account_id
      this.withdrawForm.user_id = row.account_id
      this.withdrawForm.amount = 10000
      this.withdrawDialogVisible = true
    },

    handleWithdrawSubmit() {
      this.$refs.withdrawForm.validate(valid => {
        if (!valid) return
        this.submitting = true
        withdraw(this.withdrawForm)
          .then(() => {
            this.$message.success('取款成功')
            this.withdrawDialogVisible = false
            this.loadAccounts()
          })
          .catch(err => {
            this.$message.error(extractErrorMessage(err, '取款失败'))
          })
          .finally(() => {
            this.submitting = false
          })
      })
    },

    // 详情弹窗:用 <pre> 保留缩进 + 限高滚动。@yutiansut @quantaxis
    //
    // 原写法 `this.$alert(JSON.stringify(row, null, 2), ...)` 有两个问题:
    // ① `\n` 与缩进空格在 HTML 里被折叠 —— 精心生成的两空格缩进**完全丢失**,
    //    渲染成一整段跑马文(实测截图 r_positions_detail-row0 / d_accounts_detail-alert)。
    // ② element-ui 的 `.el-message-box{overflow:hidden}`,而
    //    `.el-message-box__wrapper` 的 overflow-y 实测是 **visible** 不是 auto,
    //    且 styles/index.scss 全文 `max-height` 出现次数为 0 ——
    //    内容一旦超高既不能滚动也不会截断,**确定按钮会被挤出视口**。
    //    当前消息体约 340 字符尚有约 4 倍余量,属潜在风险,这里一并兜掉。
    //
    // 用 `dangerouslyUseHTMLString` 就必须自己转义 —— 字段值来自后端,
    // 直接插 HTML 等于把接口数据当模板执行。escapeHtml 覆盖 & < > " ' 五个字符。
    handleViewDetail(row) {
      const esc = s => String(s)
        .replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;').replace(/'/g, '&#39;')
      const pre = '<pre style="max-height:60vh;overflow:auto;margin:0;' +
                  'white-space:pre-wrap;word-break:break-all;font-size:12px;' +
                  'line-height:1.5;text-align:left">' +
                  esc(JSON.stringify(row, null, 2)) + '</pre>'
      this.$alert(pre, '账户详情', {
        confirmButtonText: '确定',
        dangerouslyUseHTMLString: true,
        customClass: 'qa-detail-alert'
      }).catch(() => {
        // $alert 在点 X / Esc 关闭时会 reject('cancel'),不接就产生一条未捕获的
        // unhandledrejection console 错误(点「确定」不会)。回验代理用旧代码
        // 复现过 —— 这是改动前就存在的,顺手补上。@yutiansut @quantaxis
      })
    }
  }
}
</script>

<style lang="scss" scoped>
.accounts-page {
  padding: 20px;

  .card-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
}
</style>
