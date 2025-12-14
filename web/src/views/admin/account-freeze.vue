<template>
  <div class="account-freeze-container">
    <!-- 页面标题 -->
    <div class="page-header">
      <h2>账户状态管理</h2>
      <div class="header-actions">
        <el-button icon="el-icon-refresh" @click="loadAccounts">刷新</el-button>
      </div>
    </div>

    <!-- 搜索栏 -->
    <el-card class="search-card">
      <el-form :inline="true" :model="searchForm">
        <el-form-item label="账户ID">
          <el-input
            v-model="searchForm.account_id"
            placeholder="请输入账户ID"
            clearable
            style="width: 200px"
            @keyup.enter.native="searchAccount"
          ></el-input>
        </el-form-item>
        <el-form-item>
          <el-button type="primary" icon="el-icon-search" @click="searchAccount">查询</el-button>
        </el-form-item>
      </el-form>
    </el-card>

    <!-- 账户状态信息 -->
    <el-card v-if="accountStatus" class="status-card">
      <div slot="header">
        <span>账户状态详情</span>
      </div>
      <el-descriptions :column="2" border>
        <el-descriptions-item label="账户ID">{{ accountStatus.account_id }}</el-descriptions-item>
        <el-descriptions-item label="当前状态">
          <el-tag :type="getStatusTag(accountStatus.status)" size="medium">
            {{ getStatusLabel(accountStatus.status) }}
          </el-tag>
        </el-descriptions-item>
        <el-descriptions-item label="允许交易">
          <el-tag :type="accountStatus.can_trade ? 'success' : 'danger'" size="small">
            {{ accountStatus.can_trade ? '是' : '否' }}
          </el-tag>
        </el-descriptions-item>
        <el-descriptions-item label="允许出金">
          <el-tag :type="accountStatus.can_withdraw ? 'success' : 'danger'" size="small">
            {{ accountStatus.can_withdraw ? '是' : '否' }}
          </el-tag>
        </el-descriptions-item>
        <el-descriptions-item label="冻结类型" v-if="accountStatus.freeze_type">
          {{ getFreezeTypeLabel(accountStatus.freeze_type) }}
        </el-descriptions-item>
        <el-descriptions-item label="冻结原因" v-if="accountStatus.freeze_reason">
          {{ accountStatus.freeze_reason }}
        </el-descriptions-item>
        <el-descriptions-item label="冻结时间" v-if="accountStatus.frozen_at">
          {{ formatTime(accountStatus.frozen_at) }}
        </el-descriptions-item>
        <el-descriptions-item label="冻结操作人" v-if="accountStatus.frozen_by">
          {{ accountStatus.frozen_by }}
        </el-descriptions-item>
      </el-descriptions>

      <!-- 操作按钮 -->
      <div class="action-buttons">
        <el-button
          v-if="accountStatus.status === 'Active'"
          type="warning"
          icon="el-icon-lock"
          @click="showFreezeDialog"
        >冻结账户</el-button>
        <el-button
          v-if="accountStatus.status === 'Frozen'"
          type="success"
          icon="el-icon-unlock"
          @click="showUnfreezeDialog"
        >解冻账户</el-button>
      </div>
    </el-card>

    <!-- 冻结弹窗 -->
    <el-dialog title="冻结账户" :visible.sync="freezeDialogVisible" width="500px">
      <el-alert
        title="警告：冻结账户将限制用户的交易或出金权限"
        type="warning"
        :closable="false"
        show-icon
        style="margin-bottom: 20px;"
      ></el-alert>
      <el-form :model="freezeForm" :rules="freezeRules" ref="freezeForm" label-width="100px">
        <el-form-item label="冻结类型" prop="freeze_type">
          <el-select v-model="freezeForm.freeze_type" placeholder="请选择冻结类型" style="width: 100%">
            <el-option label="仅限交易" value="TradingOnly">
              <span>仅限交易</span>
              <span style="color: #909399; font-size: 12px;"> - 禁止下单和撤单，允许出金</span>
            </el-option>
            <el-option label="仅限出金" value="WithdrawOnly">
              <span>仅限出金</span>
              <span style="color: #909399; font-size: 12px;"> - 禁止出金，允许交易</span>
            </el-option>
            <el-option label="全部冻结" value="Full">
              <span>全部冻结</span>
              <span style="color: #909399; font-size: 12px;"> - 禁止所有操作</span>
            </el-option>
          </el-select>
        </el-form-item>
        <el-form-item label="冻结原因" prop="reason">
          <el-input
            type="textarea"
            v-model="freezeForm.reason"
            :rows="3"
            placeholder="请输入冻结原因"
          ></el-input>
        </el-form-item>
      </el-form>
      <div slot="footer">
        <el-button @click="freezeDialogVisible = false">取消</el-button>
        <el-button type="warning" @click="submitFreeze" :loading="submitting">确认冻结</el-button>
      </div>
    </el-dialog>

    <!-- 解冻弹窗 -->
    <el-dialog title="解冻账户" :visible.sync="unfreezeDialogVisible" width="500px">
      <el-form :model="unfreezeForm" :rules="unfreezeRules" ref="unfreezeForm" label-width="100px">
        <el-form-item label="解冻原因" prop="reason">
          <el-input
            type="textarea"
            v-model="unfreezeForm.reason"
            :rows="3"
            placeholder="请输入解冻原因"
          ></el-input>
        </el-form-item>
      </el-form>
      <div slot="footer">
        <el-button @click="unfreezeDialogVisible = false">取消</el-button>
        <el-button type="success" @click="submitUnfreeze" :loading="submitting">确认解冻</el-button>
      </div>
    </el-dialog>
  </div>
</template>

<script>
/**
 * 账户冻结管理页面 @yutiansut @quantaxis
 */
import { getAccountStatus, freezeAccount, unfreezeAccount } from '@/api'

export default {
  name: 'AccountFreeze',

  data() {
    return {
      loading: false,
      submitting: false,
      searchForm: {
        account_id: ''
      },
      accountStatus: null,
      freezeDialogVisible: false,
      unfreezeDialogVisible: false,
      freezeForm: {
        freeze_type: 'Full',
        reason: ''
      },
      unfreezeForm: {
        reason: ''
      },
      freezeRules: {
        freeze_type: [{ required: true, message: '请选择冻结类型', trigger: 'change' }],
        reason: [{ required: true, message: '请输入冻结原因', trigger: 'blur' }]
      },
      unfreezeRules: {
        reason: [{ required: true, message: '请输入解冻原因', trigger: 'blur' }]
      },
      statusMap: {
        Active: '正常',
        Frozen: '已冻结',
        Suspended: '已暂停',
        Closed: '已关闭'
      },
      freezeTypeMap: {
        TradingOnly: '仅限交易',
        WithdrawOnly: '仅限出金',
        Full: '全部冻结'
      }
    }
  },

  methods: {
    loadAccounts() {
      if (this.searchForm.account_id) {
        this.searchAccount()
      }
    },

    async searchAccount() {
      if (!this.searchForm.account_id) {
        this.$message.warning('请输入账户ID')
        return
      }
      this.loading = true
      try {
        const res = await getAccountStatus(this.searchForm.account_id)
        if (res.success) {
          this.accountStatus = res.data
        } else {
          this.accountStatus = null
          this.$message.error(res.error || '账户不存在')
        }
      } catch (err) {
        console.error('查询账户状态失败:', err)
        this.accountStatus = null
        this.$message.error('查询账户状态失败')
      } finally {
        this.loading = false
      }
    },

    showFreezeDialog() {
      this.freezeForm = {
        freeze_type: 'Full',
        reason: ''
      }
      this.freezeDialogVisible = true
    },

    showUnfreezeDialog() {
      this.unfreezeForm = {
        reason: ''
      }
      this.unfreezeDialogVisible = true
    },

    async submitFreeze() {
      this.$refs.freezeForm.validate(async (valid) => {
        if (!valid) return

        this.submitting = true
        try {
          const data = {
            admin_token: 'demo_admin_token', // 实际应从登录状态获取
            account_id: this.accountStatus.account_id,
            freeze_type: this.freezeForm.freeze_type,
            reason: this.freezeForm.reason
          }
          const res = await freezeAccount(data)
          if (res.success) {
            this.$message.success('账户冻结成功')
            this.freezeDialogVisible = false
            this.searchAccount() // 刷新状态
          } else {
            this.$message.error(res.error || '冻结失败')
          }
        } catch (err) {
          console.error('冻结账户失败:', err)
          this.$message.error('冻结账户失败')
        } finally {
          this.submitting = false
        }
      })
    },

    async submitUnfreeze() {
      this.$refs.unfreezeForm.validate(async (valid) => {
        if (!valid) return

        this.submitting = true
        try {
          const data = {
            admin_token: 'demo_admin_token', // 实际应从登录状态获取
            account_id: this.accountStatus.account_id,
            reason: this.unfreezeForm.reason
          }
          const res = await unfreezeAccount(data)
          if (res.success) {
            this.$message.success('账户解冻成功')
            this.unfreezeDialogVisible = false
            this.searchAccount() // 刷新状态
          } else {
            this.$message.error(res.error || '解冻失败')
          }
        } catch (err) {
          console.error('解冻账户失败:', err)
          this.$message.error('解冻账户失败')
        } finally {
          this.submitting = false
        }
      })
    },

    formatTime(timestamp) {
      if (!timestamp) return '-'
      const date = new Date(timestamp)
      return date.toLocaleString('zh-CN')
    },

    getStatusLabel(status) {
      return this.statusMap[status] || status
    },

    getStatusTag(status) {
      const tagMap = {
        Active: 'success',
        Frozen: 'danger',
        Suspended: 'warning',
        Closed: 'info'
      }
      return tagMap[status] || ''
    },

    getFreezeTypeLabel(type) {
      return this.freezeTypeMap[type] || type
    }
  }
}
</script>

<style scoped>
.account-freeze-container {
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
  color: #303133;
}

.header-actions {
  display: flex;
  gap: 10px;
}

.search-card {
  margin-bottom: 20px;
}

.status-card {
  margin-bottom: 20px;
}

.action-buttons {
  margin-top: 20px;
  text-align: center;
}
</style>
