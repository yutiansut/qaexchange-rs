<template>
  <div class="orders-page">
    <el-card>
      <div slot="header" class="card-header">
        <span>订单管理</span>
        <el-button type="primary" size="small" @click="showSubmitDialog">
          <i class="el-icon-plus"></i> 下单
        </el-button>
      </div>

      <el-form :inline="true" size="small">
        <el-form-item label="交易账户">
          <el-select v-model="queryAccountId" placeholder="选择账户" clearable filterable>
            <el-option
              v-for="account in accounts"
              :key="account.account_id"
              :label="`${account.account_name} (${account.account_id.slice(0, 8)}...)`"
              :value="account.account_id"
            />
          </el-select>
        </el-form-item>
        <el-form-item label="合约">
          <el-select v-model="queryInstrument" placeholder="选择合约" clearable>
            <el-option label="IF2501" value="IF2501" />
            <el-option label="IF2502" value="IF2502" />
            <el-option label="IC2501" value="IC2501" />
            <el-option label="IH2501" value="IH2501" />
          </el-select>
        </el-form-item>
        <el-form-item>
          <el-button type="primary" @click="handleQuery">查询</el-button>
          <el-button @click="handleReset">重置</el-button>
        </el-form-item>
      </el-form>

      <el-table
        :data="orderList"
        border
        stripe
        height="500"
        v-loading="loading"
        style="width: 100%"
      >
        <el-table-column prop="order_id" label="订单ID" width="200" />
        <el-table-column prop="user_id" label="账户ID" width="150" show-overflow-tooltip>
          <template slot-scope="scope">
            {{ getAccountName(scope.row.user_id) }}
          </template>
        </el-table-column>
        <el-table-column prop="instrument_id" label="合约" width="100" />
        <el-table-column prop="direction" label="方向" width="80" align="center">
          <template slot-scope="scope">
            <el-tag :type="scope.row.direction === 'BUY' ? 'danger' : 'success'" size="mini">
              {{ scope.row.direction === 'BUY' ? '买入' : '卖出' }}
            </el-tag>
          </template>
        </el-table-column>
        <el-table-column prop="offset" label="开平" width="80" align="center">
          <template slot-scope="scope">
            {{ scope.row.offset === 'OPEN' ? '开仓' : '平仓' }}
          </template>
        </el-table-column>
        <el-table-column prop="price" label="价格" width="100" align="right">
          <template slot-scope="scope">
            {{ scope.row.price.toFixed(2) }}
          </template>
        </el-table-column>
        <el-table-column prop="volume" label="数量" width="80" align="right" />
        <el-table-column prop="filled_volume" label="成交量" width="80" align="right" />
        <el-table-column prop="status" label="状态" width="100" align="center">
          <template slot-scope="scope">
            <el-tag :type="getStatusType(scope.row.status)" size="mini">
              {{ getStatusText(scope.row.status) }}
            </el-tag>
          </template>
        </el-table-column>
        <el-table-column prop="created_at" label="创建时间" width="160" />
        <el-table-column label="操作" width="150" fixed="right">
          <template slot-scope="scope">
            <el-button
              v-if="scope.row.status === 'Pending'"
              type="text"
              size="small"
              @click="handleCancel(scope.row)"
            >
              撤单
            </el-button>
            <el-button type="text" size="small" @click="handleViewDetail(scope.row)">
              详情
            </el-button>
          </template>
        </el-table-column>
      </el-table>
    </el-card>

    <!-- 下单对话框 -->
    <el-dialog title="提交订单" :visible.sync="submitDialogVisible" width="500px">
      <el-form :model="submitForm" :rules="submitRules" ref="submitForm" label-width="100px">
        <el-form-item label="交易账户" prop="user_id">
          <el-select v-model="submitForm.user_id" filterable style="width: 100%">
            <el-option
              v-for="account in accounts"
              :key="account.account_id"
              :label="`${account.account_name} (可用: ¥${account.available.toLocaleString()})`"
              :value="account.account_id"
            >
              <div style="display: flex; justify-content: space-between;">
                <span>{{ account.account_name }}</span>
                <span style="color: #8492a6; font-size: 12px;">
                  可用: ¥{{ account.available.toLocaleString() }}
                </span>
              </div>
            </el-option>
          </el-select>
        </el-form-item>
        <el-form-item label="合约" prop="instrument_id">
          <el-select v-model="submitForm.instrument_id" style="width: 100%">
            <el-option label="IF2501 - 沪深300股指期货" value="IF2501" />
            <el-option label="IF2502 - 沪深300股指期货" value="IF2502" />
            <el-option label="IC2501 - 中证500股指期货" value="IC2501" />
            <el-option label="IH2501 - 上证50股指期货" value="IH2501" />
          </el-select>
        </el-form-item>
        <el-form-item label="方向" prop="direction">
          <el-radio-group v-model="submitForm.direction">
            <el-radio label="BUY">买入</el-radio>
            <el-radio label="SELL">卖出</el-radio>
          </el-radio-group>
        </el-form-item>
        <el-form-item label="开平" prop="offset">
          <el-radio-group v-model="submitForm.offset">
            <el-radio label="OPEN">开仓</el-radio>
            <el-radio label="CLOSE">平仓</el-radio>
          </el-radio-group>
        </el-form-item>
        <el-form-item label="价格" prop="price">
          <el-input-number
            v-model="submitForm.price"
            :min="0"
            :step="0.2"
            :precision="1"
            style="width: 100%"
          />
        </el-form-item>
        <el-form-item label="数量" prop="volume">
          <el-input-number
            v-model="submitForm.volume"
            :min="1"
            :step="1"
            style="width: 100%"
          />
        </el-form-item>
        <el-form-item label="订单类型" prop="order_type">
          <el-select v-model="submitForm.order_type" style="width: 100%">
            <el-option label="限价单" value="LIMIT" />
            <el-option label="市价单" value="MARKET" />
          </el-select>
        </el-form-item>
      </el-form>
      <div slot="footer">
        <el-button @click="submitDialogVisible = false">取消</el-button>
        <el-button type="primary" @click="handleSubmitOrder" :loading="submitting">
          提交
        </el-button>
      </div>
    </el-dialog>
  </div>
</template>

<script>
import { submitOrder, cancelOrder, queryUserOrders, getUserAccounts } from '@/api'
import { mapGetters, mapActions } from 'vuex'

export default {
  name: 'Orders',
  computed: {
    ...mapGetters(['currentUser'])
  },
  data() {
    return {
      loading: false,
      submitting: false,
      orderList: [],
      accounts: [],
      queryAccountId: '',
      queryInstrument: '',
      submitDialogVisible: false,
      submitForm: {
        user_id: '',        // ✨ 交易所模式：与account_id相同 @yutiansut @quantaxis
        account_id: '',     // 交易账户ID
        instrument_id: 'IF2501',
        direction: 'BUY',
        offset: 'OPEN',
        price: 3800,
        volume: 1,
        order_type: 'LIMIT'
      },
      submitRules: {
        user_id: [{ required: true, message: '请选择交易账户', trigger: 'change' }],
        instrument_id: [{ required: true, message: '请选择合约', trigger: 'change' }],
        direction: [{ required: true, message: '请选择方向', trigger: 'change' }],
        offset: [{ required: true, message: '请选择开平', trigger: 'change' }],
        price: [{ required: true, message: '请输入价格', trigger: 'blur' }],
        volume: [{ required: true, message: '请输入数量', trigger: 'blur' }],
        order_type: [{ required: true, message: '请选择订单类型', trigger: 'change' }]
      }
    }
  },
  mounted() {
    this.loadAccounts()
    this.loadOrders()
  },
  methods: {
    // ✨ 映射 Vuex websocket 模块的 actions @yutiansut @quantaxis
    ...mapActions('websocket', ['fetchUserAccounts']),

    async loadAccounts() {
      if (!this.currentUser) {
        return
      }

      try {
        const res = await getUserAccounts(this.currentUser)
        this.accounts = res.accounts || []
      } catch (error) {
        console.error('加载账户列表失败:', error)
      }
    },

    getAccountName(accountId) {
      const account = this.accounts.find(acc => acc.account_id === accountId)
      return account ? `${account.account_name} (${accountId.slice(0, 8)}...)` : accountId
    },
    getStatusType(status) {
      const map = {
        'Pending': 'warning',
        'PartiallyFilled': 'info',
        'Filled': 'success',
        'Cancelled': 'info',
        'Rejected': 'danger'
      }
      return map[status] || 'info'
    },

    getStatusText(status) {
      const map = {
        'Pending': '待成交',
        'PartiallyFilled': '部分成交',
        'Filled': '已成交',
        'Cancelled': '已撤销',
        'Rejected': '已拒绝'
      }
      return map[status] || status
    },

    async loadOrders() {
      if (!this.currentUser) {
        this.$message.warning('请先登录')
        return
      }

      this.loading = true
      try {
        const data = await queryUserOrders(this.currentUser)
        this.orderList = Array.isArray(data) ? data : (data.orders || [])
        this.loading = false
      } catch (error) {
        this.$message.error('加载订单失败: ' + ((error.response && error.response.data && error.response.data.error) || error.message))
        this.orderList = []
        this.loading = false
      }
    },

    handleQuery() {
      if (this.queryAccountId) {
        this.loading = true
        queryUserOrders(this.queryAccountId)
          .then(data => {
            this.orderList = data.orders || []
          })
          .catch(() => {
            this.$message.error('查询失败')
          })
          .finally(() => {
            this.loading = false
          })
      } else {
        this.loadOrders()
      }
    },

    handleReset() {
      this.queryAccountId = ''
      this.queryInstrument = ''
      this.loadOrders()
    },

    showSubmitDialog() {
      // ✨ 交易所模式：自动选择第一个账户，user_id和account_id都设置为账户ID @yutiansut @quantaxis
      if (this.accounts.length > 0) {
        const accountId = this.accounts[0].account_id
        this.submitForm.user_id = accountId
        this.submitForm.account_id = accountId
      }
      this.submitDialogVisible = true
      this.$nextTick(() => {
        this.$refs.submitForm && this.$refs.submitForm.resetFields()
        // 重新设置账户ID（因为resetFields会清空）
        if (this.accounts.length > 0) {
          const accountId = this.accounts[0].account_id
          this.submitForm.user_id = accountId
          this.submitForm.account_id = accountId
        }
      })
    },

    handleSubmitOrder() {
      this.$refs.submitForm.validate(valid => {
        if (valid) {
          this.submitting = true
          submitOrder(this.submitForm)
            .then(() => {
              this.$message.success('订单提交成功')
              this.submitDialogVisible = false
              this.loadOrders()
            })
            .catch(() => {
              this.$message.error('订单提交失败')
            })
            .finally(() => {
              this.submitting = false
            })
        }
      })
    },

    handleCancel(row) {
      this.$confirm('确认撤销该订单?', '提示', {
        type: 'warning'
      }).then(() => {
        // ✨ 交易所模式：user_id 和 account_id 都使用账户ID @yutiansut @quantaxis
        // 后端 verify_account_ownership 会检测 user_id == account_id，自动跳过所有权验证
        cancelOrder({
          user_id: row.user_id,     // 账户ID (交易所模式)
          account_id: row.user_id,  // 必须传递，否则后端会走 get_default_account 失败
          order_id: row.order_id
        })
          .then(() => {
            this.$message.success('撤单成功')
            this.loadOrders()

            // ✨ 撤单后刷新账户数据，更新可用资金显示 @yutiansut @quantaxis
            const userInfo = this.$store.state.userInfo
            const userId = userInfo && userInfo.user_id
            if (userId) {
              this.fetchUserAccounts(userId).catch(err => {
                console.warn('[Orders] Failed to refresh account after cancel:', err)
              })
            }
          })
          .catch(() => {
            this.$message.error('撤单失败')
          })
      })
    },

    handleViewDetail(row) {
      this.$alert(JSON.stringify(row, null, 2), '订单详情', {
        confirmButtonText: '确定'
      })
    }
  }
}
</script>

<style lang="scss" scoped>
// @yutiansut @quantaxis - 专业量化交易系统订单页面样式
$dark-bg-primary: #0d1117;
$dark-bg-secondary: #161b22;
$dark-bg-card: #1c2128;
$dark-bg-tertiary: #21262d;
$dark-border: #30363d;
$dark-text-primary: #f0f6fc;
$dark-text-secondary: #8b949e;
$primary-color: #1890ff;
$buy-color: #f5222d;
$sell-color: #52c41a;
$warning-color: #faad14;

.orders-page {
  min-height: 100%;
  background: $dark-bg-primary;
  padding: 0;

  // 卡片样式
  ::v-deep .el-card {
    background: $dark-bg-card;
    border: 1px solid $dark-border;
    border-radius: 12px;

    .el-card__header {
      background: $dark-bg-secondary;
      border-bottom: 1px solid $dark-border;
      padding: 16px 20px;
    }

    .el-card__body {
      padding: 20px;
    }
  }

  .card-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    color: $dark-text-primary;
    font-size: 16px;
    font-weight: 600;

    .el-button {
      background: linear-gradient(135deg, $primary-color 0%, #096dd9 100%);
      border: none;
      font-weight: 500;

      &:hover {
        opacity: 0.9;
        transform: translateY(-1px);
      }
    }
  }

  // 筛选表单
  ::v-deep .el-form {
    margin-bottom: 20px;
    padding: 16px;
    background: $dark-bg-secondary;
    border-radius: 8px;
    border: 1px solid $dark-border;

    .el-form-item {
      margin-bottom: 0;
      margin-right: 16px;

      .el-form-item__label {
        color: $dark-text-secondary;
        font-size: 13px;
      }
    }

    .el-input__inner,
    .el-select .el-input__inner {
      background: $dark-bg-tertiary;
      border: 1px solid $dark-border;
      color: $dark-text-primary;

      &:focus {
        border-color: $primary-color;
      }

      &::placeholder {
        color: $dark-text-secondary;
      }
    }

    .el-select-dropdown {
      background: $dark-bg-card;
      border: 1px solid $dark-border;
    }

    .el-button--primary {
      background: $primary-color;
      border-color: $primary-color;
    }

    .el-button--default {
      background: $dark-bg-tertiary;
      border-color: $dark-border;
      color: $dark-text-secondary;

      &:hover {
        border-color: $primary-color;
        color: $primary-color;
      }
    }
  }

  // 表格样式
  ::v-deep .el-table {
    background: transparent;
    border-radius: 8px;
    overflow: hidden;

    &::before {
      display: none;
    }

    th.el-table__cell {
      background: $dark-bg-secondary;
      border-bottom: 1px solid $dark-border;
      color: $dark-text-secondary;
      font-weight: 600;
      font-size: 12px;
      text-transform: uppercase;
      letter-spacing: 0.5px;
      padding: 14px 0;
    }

    td.el-table__cell {
      background: $dark-bg-card;
      border-bottom: 1px solid $dark-border;
      color: $dark-text-primary;
      padding: 12px 0;
      font-family: 'JetBrains Mono', monospace;
      font-size: 13px;
    }

    tr:hover td.el-table__cell {
      background: $dark-bg-tertiary !important;
    }

    .el-table__body-wrapper {
      &::-webkit-scrollbar {
        width: 8px;
        height: 8px;
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

    // 固定列
    .el-table__fixed-right {
      background: transparent;

      th.el-table__cell,
      td.el-table__cell {
        background: $dark-bg-card;
      }
    }

    // 空数据
    .el-table__empty-block {
      background: $dark-bg-card;
    }

    .el-table__empty-text {
      color: $dark-text-secondary;
    }
  }

  // 标签样式
  ::v-deep .el-tag {
    border: none;
    font-weight: 600;
    font-family: 'JetBrains Mono', monospace;
    font-size: 11px;

    &--danger {
      background: rgba($buy-color, 0.15);
      color: $buy-color;
    }

    &--success {
      background: rgba($sell-color, 0.15);
      color: $sell-color;
    }

    &--warning {
      background: rgba($warning-color, 0.15);
      color: $warning-color;
    }

    &--info {
      background: rgba($dark-text-secondary, 0.15);
      color: $dark-text-secondary;
    }
  }

  // 操作按钮
  ::v-deep .el-button--text {
    color: $primary-color;
    font-size: 13px;

    &:hover {
      color: lighten($primary-color, 10%);
    }
  }
}

// 下单对话框
::v-deep .el-dialog {
  background: $dark-bg-card;
  border-radius: 12px;
  border: 1px solid $dark-border;

  .el-dialog__header {
    background: $dark-bg-secondary;
    border-bottom: 1px solid $dark-border;
    padding: 16px 20px;
    border-radius: 12px 12px 0 0;

    .el-dialog__title {
      color: $dark-text-primary;
      font-weight: 600;
    }

    .el-dialog__headerbtn .el-dialog__close {
      color: $dark-text-secondary;

      &:hover {
        color: $dark-text-primary;
      }
    }
  }

  .el-dialog__body {
    padding: 24px;
    background: $dark-bg-card;
  }

  .el-dialog__footer {
    background: $dark-bg-secondary;
    border-top: 1px solid $dark-border;
    padding: 16px 20px;
    border-radius: 0 0 12px 12px;

    .el-button--default {
      background: $dark-bg-tertiary;
      border-color: $dark-border;
      color: $dark-text-secondary;

      &:hover {
        border-color: $primary-color;
        color: $primary-color;
      }
    }

    .el-button--primary {
      background: linear-gradient(135deg, $primary-color 0%, #096dd9 100%);
      border: none;
    }
  }

  .el-form-item__label {
    color: $dark-text-secondary;
  }

  .el-input__inner,
  .el-select .el-input__inner {
    background: $dark-bg-tertiary;
    border: 1px solid $dark-border;
    color: $dark-text-primary;

    &:focus {
      border-color: $primary-color;
    }
  }

  .el-input-number {
    .el-input__inner {
      background: $dark-bg-tertiary;
      border: 1px solid $dark-border;
      color: $dark-text-primary;
    }

    .el-input-number__decrease,
    .el-input-number__increase {
      background: $dark-bg-secondary;
      border-color: $dark-border;
      color: $dark-text-secondary;

      &:hover {
        color: $primary-color;
      }
    }
  }

  .el-radio {
    color: $dark-text-secondary;

    &.is-checked .el-radio__label {
      color: $primary-color;
    }

    .el-radio__inner {
      background: $dark-bg-tertiary;
      border-color: $dark-border;
    }

    &.is-checked .el-radio__inner {
      background: $primary-color;
      border-color: $primary-color;
    }
  }

  // 选项下拉样式
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

// 下拉菜单全局
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
    }
  }
}
</style>
