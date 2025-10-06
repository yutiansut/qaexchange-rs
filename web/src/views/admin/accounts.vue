<template>
  <div class="accounts-container">
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
            <div class="stat-label">总资金</div>
            <div class="stat-value">{{ totalBalance.toLocaleString() }}</div>
          </div>
        </el-card>
        <el-card shadow="hover" class="stat-card">
          <div class="stat-item">
            <div class="stat-label">可用资金</div>
            <div class="stat-value">{{ totalAvailable.toLocaleString() }}</div>
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
      <el-button type="primary" icon="el-icon-search" @click="fetchAccounts">查询</el-button>
      <el-button icon="el-icon-refresh" @click="resetFilters">重置</el-button>
    </div>

    <!-- 账户列表 -->
    <div class="table-container">
      <vxe-table
        ref="accountTable"
        :data="accounts"
        border
        stripe
        resizable
        highlight-hover-row
        :loading="loading"
        :sort-config="{ trigger: 'cell', remote: false }"
        height="500"
      >
        <vxe-table-column field="account_id" title="账户ID" width="200" sortable show-overflow="tooltip"></vxe-table-column>
        <vxe-table-column field="user_id" title="用户ID" width="200" sortable show-overflow="tooltip"></vxe-table-column>
        <vxe-table-column field="account_name" title="账户名称" width="150"></vxe-table-column>
        <vxe-table-column field="account_type" title="账户类型" width="120">
          <template slot-scope="{ row }">
            <el-tag :type="row.account_type === 'Individual' ? 'success' : 'warning'" size="small">
              {{ row.account_type === 'Individual' ? '个人' : '机构' }}
            </el-tag>
          </template>
        </vxe-table-column>
        <vxe-table-column field="balance" title="总权益" width="140" align="right">
          <template slot-scope="{ row }">
            <span :class="{ 'positive': row.balance > 0 }">
              {{ row.balance.toLocaleString('zh-CN', { minimumFractionDigits: 2 }) }}
            </span>
          </template>
        </vxe-table-column>
        <vxe-table-column field="available" title="可用资金" width="140" align="right">
          <template slot-scope="{ row }">
            {{ row.available.toLocaleString('zh-CN', { minimumFractionDigits: 2 }) }}
          </template>
        </vxe-table-column>
        <vxe-table-column field="margin_used" title="占用保证金" width="140" align="right">
          <template slot-scope="{ row }">
            {{ row.margin_used.toLocaleString('zh-CN', { minimumFractionDigits: 2 }) }}
          </template>
        </vxe-table-column>
        <vxe-table-column field="risk_ratio" title="风险率" width="120" align="right">
          <template slot-scope="{ row }">
            <el-tag :type="getRiskTagType(row.risk_ratio)" size="small">
              {{ (row.risk_ratio * 100).toFixed(2) }}%
            </el-tag>
          </template>
        </vxe-table-column>
        <vxe-table-column field="created_at" title="创建时间" width="180">
          <template slot-scope="{ row }">
            {{ formatTimestamp(row.created_at) }}
          </template>
        </vxe-table-column>
        <vxe-table-column title="操作" width="200" fixed="right">
          <template slot-scope="{ row }">
            <el-button size="mini" type="text" @click="showAccountDetail(row)">详情</el-button>
            <el-button size="mini" type="text" @click="showDepositDialog(row)">入金</el-button>
            <el-button size="mini" type="text" @click="showWithdrawDialog(row)">出金</el-button>
          </template>
        </vxe-table-column>
      </vxe-table>

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

export default {
  name: 'AccountManagement',
  data() {
    return {
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
    totalBalance() {
      return this.accounts.reduce((sum, acc) => sum + acc.balance, 0)
    },
    totalAvailable() {
      return this.accounts.reduce((sum, acc) => sum + acc.available, 0)
    }
  },
  mounted() {
    this.fetchAccounts()
  },
  methods: {
    async fetchAccounts() {
      this.loading = true
      try {
        const params = {
          page: this.pagination.currentPage,
          page_size: this.pagination.pageSize,
          status: this.filters.status
        }

        // 如果设置了用户ID筛选，添加到查询参数
        if (this.filters.userId && this.filters.userId.trim()) {
          params.user_id = this.filters.userId.trim()
        }

        const { data } = await listAllAccounts(params)

        // 如果有本地用户ID筛选，进行前端过滤（备用方案）
        let accounts = data.accounts || []
        if (this.filters.userId && this.filters.userId.trim()) {
          const userIdFilter = this.filters.userId.trim().toLowerCase()
          accounts = accounts.filter(acc =>
            acc.user_id && acc.user_id.toLowerCase().includes(userIdFilter)
          )
        }

        this.accounts = accounts
        this.pagination.total = data.total || 0
      } catch (error) {
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
      this.$router.push({ path: `/admin/account-detail/${row.user_id}` })
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

<style scoped>
.accounts-container {
  padding: 20px;
}

.page-header {
  margin-bottom: 20px;
}

.page-header h2 {
  margin: 0 0 20px 0;
  font-size: 24px;
  font-weight: 500;
}

.header-stats {
  display: flex;
  gap: 20px;
  margin-bottom: 20px;
}

.stat-card {
  flex: 1;
}

.stat-item {
  text-align: center;
}

.stat-label {
  font-size: 14px;
  color: #909399;
  margin-bottom: 8px;
}

.stat-value {
  font-size: 24px;
  font-weight: 500;
  color: #303133;
}

.filter-container {
  margin-bottom: 20px;
}

.table-container {
  background: white;
  padding: 20px;
  border-radius: 4px;
}

.pagination-container {
  margin-top: 20px;
  text-align: right;
}

.positive {
  color: #67C23A;
}
</style>
