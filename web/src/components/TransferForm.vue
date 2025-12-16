<template>
  <div class="transfer-form">
    <h3>银期转账</h3>

    <!-- 账户选择器 @yutiansut @quantaxis -->
    <account-selector @account-changed="handleAccountChanged" />

    <!-- 转账表单 -->
    <form @submit.prevent="handleSubmit" class="form">
      <!-- 转账方向 -->
      <div class="form-group">
        <label>转账方向：</label>
        <div class="direction-buttons">
          <button
            type="button"
            class="direction-btn"
            :class="{ active: transferDirection === 'in' }"
            @click="transferDirection = 'in'"
          >
            银行转期货（入金）
          </button>
          <button
            type="button"
            class="direction-btn"
            :class="{ active: transferDirection === 'out' }"
            @click="transferDirection = 'out'"
          >
            期货转银行（出金）
          </button>
        </div>
      </div>

      <!-- 银行选择 -->
      <div class="form-group">
        <label for="bank">签约银行：</label>
        <select id="bank" v-model="form.bank_id" required>
          <option value="" disabled>请选择银行</option>
          <option v-for="bank in banks" :key="bank.id" :value="bank.id">
            {{ bank.name }}
          </option>
        </select>
      </div>

      <!-- 转账金额 -->
      <div class="form-group">
        <label for="amount">转账金额（元）：</label>
        <input
          type="number"
          id="amount"
          v-model.number="form.amount"
          min="1"
          step="0.01"
          placeholder="请输入转账金额"
          required
        />
      </div>

      <!-- 银行密码 -->
      <div class="form-group">
        <label for="bankPassword">银行密码：</label>
        <input
          type="password"
          id="bankPassword"
          v-model="form.bank_password"
          placeholder="请输入银行密码"
          required
        />
      </div>

      <!-- 期货密码 -->
      <div class="form-group">
        <label for="futurePassword">期货资金密码：</label>
        <input
          type="password"
          id="futurePassword"
          v-model="form.future_password"
          placeholder="请输入期货资金密码"
          required
        />
      </div>

      <!-- 提交按钮 -->
      <div class="form-actions">
        <button
          type="submit"
          class="btn-submit"
          :disabled="!canSubmit"
          :class="[
            { 'btn-disabled': !canSubmit },
            transferDirection === 'in' ? 'btn-deposit' : 'btn-withdraw'
          ]"
        >
          {{ submitButtonText }}
        </button>
      </div>
    </form>

    <!-- 错误提示 -->
    <div v-if="errorMessage" class="error-message">
      {{ errorMessage }}
    </div>

    <!-- 成功提示 -->
    <div v-if="successMessage" class="success-message">
      {{ successMessage }}
    </div>

    <!-- 转账记录 -->
    <div class="transfer-records" v-if="transferRecords.length > 0">
      <h4>转账记录</h4>
      <table class="records-table">
        <thead>
          <tr>
            <th>时间</th>
            <th>银行</th>
            <th>类型</th>
            <th>金额</th>
            <th>状态</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="record in transferRecords" :key="record.id">
            <td>{{ formatTime(record.datetime) }}</td>
            <td>{{ record.bank_name }}</td>
            <td :class="record.amount > 0 ? 'type-deposit' : 'type-withdraw'">
              {{ record.amount > 0 ? '入金' : '出金' }}
            </td>
            <td :class="record.amount > 0 ? 'amount-positive' : 'amount-negative'">
              {{ record.amount > 0 ? '+' : '' }}{{ record.amount.toFixed(2) }}
            </td>
            <td :class="record.error_id === 0 ? 'status-success' : 'status-failed'">
              {{ record.error_id === 0 ? '成功' : record.error_msg }}
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>

<script>
/**
 * 银期转账组件 @yutiansut @quantaxis
 * 支持银行转期货（入金）和期货转银行（出金）
 */
import { mapGetters } from 'vuex'
import { doTransfer, getBanks, getTransferRecords } from '../api'
import AccountSelector from './AccountSelector.vue'

export default {
  name: 'TransferForm',

  components: {
    AccountSelector
  },

  data() {
    return {
      transferDirection: 'in', // 'in' = 入金, 'out' = 出金
      form: {
        bank_id: '',
        amount: null,
        bank_password: '',
        future_password: ''
      },
      banks: [],
      transferRecords: [],
      errorMessage: '',
      successMessage: '',
      submitting: false,
      loading: false
    }
  },

  computed: {
    ...mapGetters('websocket', [
      'isConnected',
      'currentAccountId',
      'hasAccounts'
    ]),

    canSubmit() {
      return (
        this.currentAccountId &&
        !this.submitting &&
        this.form.bank_id &&
        this.form.amount > 0 &&
        this.form.bank_password &&
        this.form.future_password
      )
    },

    submitButtonText() {
      if (this.submitting) return '处理中...'
      if (!this.currentAccountId) return '请选择账户'
      return this.transferDirection === 'in' ? '确认入金' : '确认出金'
    }
  },

  watch: {
    currentAccountId: {
      immediate: true,
      handler(newVal) {
        if (newVal) {
          this.loadBanks()
          this.loadTransferRecords()
        }
      }
    }
  },

  methods: {
    /**
     * 账户切换处理
     */
    handleAccountChanged(accountId) {
      console.log('[TransferForm] Account changed to:', accountId)
      this.errorMessage = ''
      this.successMessage = ''
      if (accountId) {
        this.loadBanks()
        this.loadTransferRecords()
      }
    },

    /**
     * 加载签约银行列表 @yutiansut @quantaxis
     * 注意：request.js 拦截器已返回 res.data，不需要再 .data
     */
    async loadBanks() {
      if (!this.currentAccountId) return
      try {
        const res = await getBanks(this.currentAccountId)
        // request 拦截器已返回 data，直接取 banks
        this.banks = res.banks || []
        console.log('[TransferForm] Loaded banks:', this.banks)
      } catch (error) {
        console.error('[TransferForm] Failed to load banks:', error)
      }
    },

    /**
     * 加载转账记录 @yutiansut @quantaxis
     */
    async loadTransferRecords() {
      if (!this.currentAccountId) return
      try {
        const res = await getTransferRecords(this.currentAccountId)
        // request 拦截器已返回 data，直接取 records
        this.transferRecords = res.records || []
      } catch (error) {
        console.error('[TransferForm] Failed to load transfer records:', error)
      }
    },

    /**
     * 提交转账
     */
    async handleSubmit() {
      this.errorMessage = ''
      this.successMessage = ''

      if (!this.canSubmit) {
        this.errorMessage = '请填写完整的转账信息'
        return
      }

      try {
        this.submitting = true

        // 根据转账方向设置金额正负
        const amount = this.transferDirection === 'in'
          ? Math.abs(this.form.amount)
          : -Math.abs(this.form.amount)

        const res = await doTransfer({
          account_id: this.currentAccountId,
          bank_id: this.form.bank_id,
          amount: amount,
          bank_password: this.form.bank_password,
          future_password: this.form.future_password
        })

        // request 拦截器已处理 success 检查，成功时直接返回 data
        this.successMessage = this.transferDirection === 'in'
          ? `入金成功！金额: ${this.form.amount.toFixed(2)} 元，当前余额: ¥${res.balance.toFixed(2)}`
          : `出金成功！金额: ${this.form.amount.toFixed(2)} 元，当前余额: ¥${res.balance.toFixed(2)}`

        // 刷新转账记录
        await this.loadTransferRecords()

        // 清空表单
        this.form.amount = null
        this.form.bank_password = ''
        this.form.future_password = ''
      } catch (error) {
        console.error('[TransferForm] Transfer failed:', error)
        this.errorMessage = `转账失败: ${error.message || '未知错误'}`
      } finally {
        this.submitting = false
      }
    },

    /**
     * 格式化时间
     */
    formatTime(timestamp) {
      if (!timestamp) return '-'
      const date = new Date(timestamp)
      return date.toLocaleString('zh-CN')
    }
  }
}
</script>

<style scoped>
.transfer-form {
  max-width: 600px;
  margin: 0 auto;
  padding: 20px;
}

.transfer-form h3 {
  margin: 0 0 20px 0;
  color: #333;
  font-size: 20px;
}

.form {
  background: white;
  padding: 20px;
  border-radius: 4px;
  border: 1px solid #e0e0e0;
}

.form-group {
  margin-bottom: 15px;
}

.form-group label {
  display: block;
  margin-bottom: 8px;
  color: #666;
  font-size: 14px;
  font-weight: 500;
}

.form-group input,
.form-group select {
  width: 100%;
  padding: 10px;
  border: 1px solid #ddd;
  border-radius: 4px;
  font-size: 14px;
  box-sizing: border-box;
}

.form-group input:focus,
.form-group select:focus {
  outline: none;
  border-color: #409EFF;
  box-shadow: 0 0 0 2px rgba(64, 158, 255, 0.2);
}

/* 转账方向按钮 */
.direction-buttons {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 10px;
}

.direction-btn {
  padding: 12px 16px;
  border: 2px solid #ddd;
  background: white;
  border-radius: 4px;
  font-size: 14px;
  cursor: pointer;
  transition: all 0.3s;
}

.direction-btn:hover {
  border-color: #409EFF;
}

.direction-btn.active {
  border-color: #409EFF;
  background: rgba(64, 158, 255, 0.1);
  color: #409EFF;
}

.form-actions {
  margin-top: 20px;
}

.btn-submit {
  width: 100%;
  padding: 12px;
  color: white;
  border: none;
  border-radius: 4px;
  font-size: 16px;
  font-weight: 500;
  cursor: pointer;
  transition: background 0.3s;
}

.btn-deposit {
  background: #67c23a;
}

.btn-deposit:hover:not(:disabled) {
  background: #85ce61;
}

.btn-withdraw {
  background: #e6a23c;
}

.btn-withdraw:hover:not(:disabled) {
  background: #ebb563;
}

.btn-disabled {
  background: #c0c4cc !important;
  cursor: not-allowed;
}

.error-message {
  margin-top: 15px;
  padding: 12px;
  background: #fef0f0;
  border: 1px solid #fde2e2;
  border-radius: 4px;
  color: #f56c6c;
  font-size: 14px;
}

.success-message {
  margin-top: 15px;
  padding: 12px;
  background: #f0f9eb;
  border: 1px solid #e1f3d8;
  border-radius: 4px;
  color: #67c23a;
  font-size: 14px;
}

/* 转账记录表格 */
.transfer-records {
  margin-top: 30px;
}

.transfer-records h4 {
  margin: 0 0 15px 0;
  color: #333;
  font-size: 16px;
}

.records-table {
  width: 100%;
  border-collapse: collapse;
  background: white;
  border-radius: 4px;
  overflow: hidden;
  border: 1px solid #e0e0e0;
}

.records-table th,
.records-table td {
  padding: 12px;
  text-align: left;
  border-bottom: 1px solid #eee;
}

.records-table th {
  background: #f5f7fa;
  color: #666;
  font-weight: 500;
  font-size: 13px;
}

.records-table td {
  font-size: 14px;
}

.type-deposit {
  color: #67c23a;
}

.type-withdraw {
  color: #e6a23c;
}

.amount-positive {
  color: #67c23a;
  font-weight: 500;
}

.amount-negative {
  color: #e6a23c;
  font-weight: 500;
}

.status-success {
  color: #67c23a;
}

.status-failed {
  color: #f56c6c;
}
</style>
