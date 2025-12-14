<template>
  <div class="batch-order-form">
    <h3>批量下单</h3>

    <!-- 账户选择器 @yutiansut @quantaxis -->
    <account-selector @account-changed="handleAccountChanged" />

    <!-- 批量下单表单 -->
    <div class="form">
      <!-- 订单列表 -->
      <div class="orders-section">
        <div class="section-header">
          <h4>订单列表</h4>
          <button type="button" class="btn-add" @click="addOrder">
            + 添加订单
          </button>
        </div>

        <div class="orders-list">
          <div
            v-for="(order, index) in orders"
            :key="index"
            class="order-item"
          >
            <div class="order-header">
              <span class="order-number">订单 #{{ index + 1 }}</span>
              <button
                type="button"
                class="btn-remove"
                @click="removeOrder(index)"
                :disabled="orders.length <= 1"
              >
                删除
              </button>
            </div>

            <div class="order-fields">
              <div class="form-row">
                <div class="form-group">
                  <label>合约代码：</label>
                  <input
                    type="text"
                    v-model="order.instrument_id"
                    placeholder="例如: SHFE.cu2501"
                    required
                  />
                </div>
                <div class="form-group">
                  <label>订单类型：</label>
                  <select v-model="order.order_type">
                    <option value="LIMIT">限价</option>
                    <option value="MARKET">市价</option>
                  </select>
                </div>
              </div>

              <div class="form-row">
                <div class="form-group">
                  <label>方向：</label>
                  <select v-model="order.direction">
                    <option value="BUY">买入</option>
                    <option value="SELL">卖出</option>
                  </select>
                </div>
                <div class="form-group">
                  <label>开平：</label>
                  <select v-model="order.offset">
                    <option value="OPEN">开仓</option>
                    <option value="CLOSE">平仓</option>
                    <option value="CLOSE_TODAY">平今</option>
                    <option value="CLOSE_YESTERDAY">平昨</option>
                  </select>
                </div>
              </div>

              <div class="form-row">
                <div class="form-group">
                  <label>数量：</label>
                  <input
                    type="number"
                    v-model.number="order.volume"
                    min="1"
                    required
                  />
                </div>
                <div class="form-group">
                  <label>价格：</label>
                  <input
                    type="number"
                    v-model.number="order.price"
                    step="0.01"
                    :placeholder="order.order_type === 'MARKET' ? '市价单无需填写' : '委托价格'"
                    :disabled="order.order_type === 'MARKET'"
                  />
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>

      <!-- 提交区域 -->
      <div class="submit-section">
        <div class="summary">
          共 {{ orders.length }} 笔订单，
          买入 {{ buyOrderCount }} 笔，
          卖出 {{ sellOrderCount }} 笔
        </div>

        <div class="form-actions">
          <button
            type="button"
            class="btn-clear"
            @click="clearOrders"
            :disabled="orders.length === 0"
          >
            清空所有
          </button>
          <button
            type="button"
            class="btn-submit"
            @click="handleSubmit"
            :disabled="!canSubmit"
            :class="{ 'btn-disabled': !canSubmit }"
          >
            {{ submitButtonText }}
          </button>
        </div>
      </div>
    </div>

    <!-- 错误提示 -->
    <div v-if="errorMessage" class="error-message">
      {{ errorMessage }}
    </div>

    <!-- 成功提示 -->
    <div v-if="successMessage" class="success-message">
      {{ successMessage }}
    </div>

    <!-- 提交结果 -->
    <div v-if="submitResult" class="submit-result">
      <h4>提交结果</h4>
      <div class="result-summary">
        <span class="result-total">总计: {{ submitResult.total }} 笔</span>
        <span class="result-success">成功: {{ submitResult.success_count }} 笔</span>
        <span class="result-failed">失败: {{ submitResult.failed_count }} 笔</span>
      </div>
      <table class="result-table" v-if="submitResult.results && submitResult.results.length > 0">
        <thead>
          <tr>
            <th>序号</th>
            <th>合约</th>
            <th>状态</th>
            <th>订单号/错误信息</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="(result, index) in submitResult.results" :key="index">
            <td>{{ index + 1 }}</td>
            <td>{{ result.instrument_id || '-' }}</td>
            <td :class="result.success ? 'status-success' : 'status-failed'">
              {{ result.success ? '成功' : '失败' }}
            </td>
            <td>{{ result.success ? result.order_id : result.error_message }}</td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>

<script>
/**
 * 批量下单组件 @yutiansut @quantaxis
 * 支持一次提交多笔订单
 */
import { mapGetters } from 'vuex'
import { batchSubmitOrders } from '../api'
import AccountSelector from './AccountSelector.vue'

export default {
  name: 'BatchOrderForm',

  components: {
    AccountSelector
  },

  data() {
    return {
      orders: [this.createEmptyOrder()],
      errorMessage: '',
      successMessage: '',
      submitting: false,
      submitResult: null
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
        this.orders.length > 0 &&
        this.orders.every(o =>
          o.instrument_id.trim() !== '' &&
          o.volume > 0 &&
          (o.order_type === 'MARKET' || o.price > 0)
        )
      )
    },

    submitButtonText() {
      if (this.submitting) return '提交中...'
      if (!this.currentAccountId) return '请选择账户'
      return `提交 ${this.orders.length} 笔订单`
    },

    buyOrderCount() {
      return this.orders.filter(o => o.direction === 'BUY').length
    },

    sellOrderCount() {
      return this.orders.filter(o => o.direction === 'SELL').length
    }
  },

  methods: {
    createEmptyOrder() {
      return {
        instrument_id: '',
        direction: 'BUY',
        offset: 'OPEN',
        volume: 1,
        price: null,
        order_type: 'LIMIT'
      }
    },

    handleAccountChanged(accountId) {
      console.log('[BatchOrderForm] Account changed to:', accountId)
      this.errorMessage = ''
      this.successMessage = ''
      this.submitResult = null
    },

    addOrder() {
      this.orders.push(this.createEmptyOrder())
    },

    removeOrder(index) {
      if (this.orders.length > 1) {
        this.orders.splice(index, 1)
      }
    },

    clearOrders() {
      if (confirm('确定要清空所有订单吗？')) {
        this.orders = [this.createEmptyOrder()]
        this.submitResult = null
      }
    },

    async handleSubmit() {
      this.errorMessage = ''
      this.successMessage = ''
      this.submitResult = null

      if (!this.canSubmit) {
        this.errorMessage = '请检查订单信息是否完整'
        return
      }

      // 验证订单
      for (let i = 0; i < this.orders.length; i++) {
        const order = this.orders[i]
        if (order.order_type === 'LIMIT' && (!order.price || order.price <= 0)) {
          this.errorMessage = `订单 #${i + 1}: 限价单必须填写价格`
          return
        }
      }

      try {
        this.submitting = true

        const data = {
          account_id: this.currentAccountId,
          orders: this.orders.map(o => ({
            instrument_id: o.instrument_id.trim().toUpperCase(),
            direction: o.direction,
            offset: o.offset,
            volume: o.volume,
            price: o.order_type === 'MARKET' ? 0 : o.price,
            order_type: o.order_type
          }))
        }

        const res = await batchSubmitOrders(data)

        if (res.data && res.data.success) {
          this.submitResult = res.data.data
          this.successMessage = `批量下单完成！成功 ${res.data.data.success_count} 笔，失败 ${res.data.data.failed_count} 笔`

          // 如果全部成功，清空订单列表
          if (res.data.data.failed_count === 0) {
            this.orders = [this.createEmptyOrder()]
          }
        } else {
          this.errorMessage = (res.data && res.data.error) || '批量下单失败'
        }
      } catch (error) {
        console.error('[BatchOrderForm] Batch submit failed:', error)
        this.errorMessage = `批量下单失败: ${error.message || '未知错误'}`
      } finally {
        this.submitting = false
      }
    }
  }
}
</script>

<style scoped>
.batch-order-form {
  max-width: 800px;
  margin: 0 auto;
  padding: 20px;
}

.batch-order-form h3 {
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

/* 订单列表区域 */
.orders-section {
  margin-bottom: 20px;
}

.section-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 15px;
}

.section-header h4 {
  margin: 0;
  color: #333;
  font-size: 16px;
}

.btn-add {
  padding: 8px 16px;
  background: #409EFF;
  color: white;
  border: none;
  border-radius: 4px;
  font-size: 14px;
  cursor: pointer;
}

.btn-add:hover {
  background: #66b1ff;
}

.orders-list {
  display: flex;
  flex-direction: column;
  gap: 15px;
}

.order-item {
  background: #f9f9f9;
  padding: 15px;
  border-radius: 4px;
  border: 1px solid #e0e0e0;
}

.order-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 12px;
}

.order-number {
  font-weight: 500;
  color: #409EFF;
}

.btn-remove {
  padding: 4px 12px;
  background: #f56c6c;
  color: white;
  border: none;
  border-radius: 3px;
  font-size: 12px;
  cursor: pointer;
}

.btn-remove:hover:not(:disabled) {
  background: #f78989;
}

.btn-remove:disabled {
  background: #c0c4cc;
  cursor: not-allowed;
}

.order-fields {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.form-row {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 15px;
}

.form-group {
  display: flex;
  flex-direction: column;
}

.form-group label {
  margin-bottom: 5px;
  color: #666;
  font-size: 13px;
}

.form-group input,
.form-group select {
  padding: 8px;
  border: 1px solid #ddd;
  border-radius: 4px;
  font-size: 13px;
}

.form-group input:focus,
.form-group select:focus {
  outline: none;
  border-color: #409EFF;
}

.form-group input:disabled {
  background: #f5f7fa;
  cursor: not-allowed;
}

/* 提交区域 */
.submit-section {
  border-top: 1px solid #e0e0e0;
  padding-top: 15px;
}

.summary {
  margin-bottom: 15px;
  color: #666;
  font-size: 14px;
}

.form-actions {
  display: flex;
  gap: 10px;
  justify-content: flex-end;
}

.btn-clear {
  padding: 12px 24px;
  background: white;
  color: #666;
  border: 1px solid #ddd;
  border-radius: 4px;
  font-size: 14px;
  cursor: pointer;
}

.btn-clear:hover:not(:disabled) {
  background: #f5f7fa;
}

.btn-submit {
  padding: 12px 32px;
  background: #409EFF;
  color: white;
  border: none;
  border-radius: 4px;
  font-size: 14px;
  font-weight: 500;
  cursor: pointer;
}

.btn-submit:hover:not(:disabled) {
  background: #66b1ff;
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

/* 提交结果 */
.submit-result {
  margin-top: 20px;
  background: white;
  padding: 20px;
  border-radius: 4px;
  border: 1px solid #e0e0e0;
}

.submit-result h4 {
  margin: 0 0 15px 0;
  color: #333;
  font-size: 16px;
}

.result-summary {
  display: flex;
  gap: 20px;
  margin-bottom: 15px;
  font-size: 14px;
}

.result-total {
  color: #666;
}

.result-success {
  color: #67c23a;
}

.result-failed {
  color: #f56c6c;
}

.result-table {
  width: 100%;
  border-collapse: collapse;
}

.result-table th,
.result-table td {
  padding: 10px;
  text-align: left;
  border-bottom: 1px solid #eee;
  font-size: 13px;
}

.result-table th {
  background: #f5f7fa;
  color: #666;
  font-weight: 500;
}

.status-success {
  color: #67c23a;
}

.status-failed {
  color: #f56c6c;
}
</style>
