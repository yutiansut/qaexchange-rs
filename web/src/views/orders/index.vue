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
import { mapGetters } from 'vuex'

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
        user_id: '',
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
      // 自动选择第一个账户
      if (this.accounts.length > 0) {
        this.submitForm.user_id = this.accounts[0].account_id
      }
      this.submitDialogVisible = true
      this.$nextTick(() => {
        this.$refs.submitForm && this.$refs.submitForm.resetFields()
        // 重新设置账户ID（因为resetFields会清空）
        if (this.accounts.length > 0) {
          this.submitForm.user_id = this.accounts[0].account_id
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
        cancelOrder({
          user_id: row.user_id,  // 使用订单所属的账户ID
          order_id: row.order_id
        })
          .then(() => {
            this.$message.success('撤单成功')
            this.loadOrders()
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
.orders-page {
  padding: 20px;

  .card-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
}
</style>
