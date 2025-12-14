<template>
  <div class="conditional-order-form">
    <h3>条件单</h3>

    <!-- 账户选择器 @yutiansut @quantaxis -->
    <account-selector @account-changed="handleAccountChanged" />

    <!-- 条件单表单 -->
    <form @submit.prevent="handleSubmit" class="form">
      <!-- 合约代码 -->
      <div class="form-group">
        <label for="instrument">合约代码：</label>
        <input
          type="text"
          id="instrument"
          v-model="form.instrument_id"
          placeholder="例如: SHFE.cu2501"
          required
        />
      </div>

      <!-- 条件类型 -->
      <div class="form-group">
        <label>条件类型：</label>
        <div class="condition-type-buttons">
          <button
            type="button"
            class="condition-btn"
            :class="{ active: form.condition_type === 'StopLoss' }"
            @click="form.condition_type = 'StopLoss'"
          >
            止损单
          </button>
          <button
            type="button"
            class="condition-btn"
            :class="{ active: form.condition_type === 'TakeProfit' }"
            @click="form.condition_type = 'TakeProfit'"
          >
            止盈单
          </button>
          <button
            type="button"
            class="condition-btn"
            :class="{ active: form.condition_type === 'PriceTouch' }"
            @click="form.condition_type = 'PriceTouch'"
          >
            触价单
          </button>
        </div>
      </div>

      <!-- 触发条件 -->
      <div class="form-row">
        <div class="form-group">
          <label for="triggerCondition">触发条件：</label>
          <select id="triggerCondition" v-model="form.trigger_condition" required>
            <option value="GreaterOrEqual">价格 >= 触发价</option>
            <option value="LessOrEqual">价格 &lt;= 触发价</option>
          </select>
        </div>
        <div class="form-group">
          <label for="triggerPrice">触发价格：</label>
          <input
            type="number"
            id="triggerPrice"
            v-model.number="form.trigger_price"
            step="0.01"
            placeholder="触发价格"
            required
          />
        </div>
      </div>

      <!-- 方向和开平 -->
      <div class="form-row">
        <div class="form-group">
          <label for="direction">方向：</label>
          <select id="direction" v-model="form.direction" required>
            <option value="BUY">买入</option>
            <option value="SELL">卖出</option>
          </select>
        </div>
        <div class="form-group">
          <label for="offset">开平：</label>
          <select id="offset" v-model="form.offset" required>
            <option value="OPEN">开仓</option>
            <option value="CLOSE">平仓</option>
            <option value="CLOSE_TODAY">平今</option>
            <option value="CLOSE_YESTERDAY">平昨</option>
          </select>
        </div>
      </div>

      <!-- 数量和订单类型 -->
      <div class="form-row">
        <div class="form-group">
          <label for="volume">数量：</label>
          <input
            type="number"
            id="volume"
            v-model.number="form.volume"
            min="1"
            required
          />
        </div>
        <div class="form-group">
          <label for="orderType">订单类型：</label>
          <select id="orderType" v-model="form.order_type" required>
            <option value="MARKET">市价</option>
            <option value="LIMIT">限价</option>
          </select>
        </div>
      </div>

      <!-- 限价（可选） -->
      <div class="form-group" v-if="form.order_type === 'LIMIT'">
        <label for="limitPrice">委托价格：</label>
        <input
          type="number"
          id="limitPrice"
          v-model.number="form.limit_price"
          step="0.01"
          placeholder="委托价格"
          :required="form.order_type === 'LIMIT'"
        />
      </div>

      <!-- 有效期 -->
      <div class="form-group">
        <label for="validUntil">有效期至（可选）：</label>
        <input
          type="datetime-local"
          id="validUntil"
          v-model="validUntilInput"
        />
      </div>

      <!-- 提交按钮 -->
      <div class="form-actions">
        <button
          type="submit"
          class="btn-submit"
          :disabled="!canSubmit"
          :class="{ 'btn-disabled': !canSubmit }"
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

    <!-- 条件单列表 -->
    <div class="conditional-orders" v-if="conditionalOrders.length > 0">
      <h4>我的条件单</h4>
      <table class="orders-table">
        <thead>
          <tr>
            <th>合约</th>
            <th>类型</th>
            <th>触发条件</th>
            <th>方向</th>
            <th>数量</th>
            <th>状态</th>
            <th>操作</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="order in conditionalOrders" :key="order.conditional_order_id">
            <td>{{ order.instrument_id }}</td>
            <td>{{ formatConditionType(order.condition_type) }}</td>
            <td>
              {{ order.trigger_condition === 'GreaterOrEqual' ? '>=' : '<=' }}
              {{ order.trigger_price }}
            </td>
            <td :class="order.direction === 'BUY' ? 'direction-buy' : 'direction-sell'">
              {{ order.direction === 'BUY' ? '买' : '卖' }}
            </td>
            <td>{{ order.volume }}</td>
            <td :class="getStatusClass(order.status)">
              {{ formatStatus(order.status) }}
            </td>
            <td>
              <button
                v-if="order.status === 'Pending'"
                class="btn-cancel"
                @click="handleCancelOrder(order.conditional_order_id)"
              >
                撤销
              </button>
              <span v-else>-</span>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>

<script>
/**
 * 条件单组件 @yutiansut @quantaxis
 * 支持止损、止盈、触价等条件单类型
 */
import { mapGetters } from 'vuex'
import { createConditionalOrder, getConditionalOrders, cancelConditionalOrder } from '../api'
import AccountSelector from './AccountSelector.vue'

export default {
  name: 'ConditionalOrderForm',

  components: {
    AccountSelector
  },

  data() {
    return {
      form: {
        instrument_id: '',
        direction: 'BUY',
        offset: 'CLOSE',
        volume: 1,
        order_type: 'MARKET',
        limit_price: null,
        condition_type: 'StopLoss',
        trigger_price: null,
        trigger_condition: 'LessOrEqual'
      },
      validUntilInput: '',
      conditionalOrders: [],
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
        this.form.instrument_id.trim() !== '' &&
        this.form.trigger_price > 0 &&
        this.form.volume > 0
      )
    },

    submitButtonText() {
      if (this.submitting) return '提交中...'
      if (!this.currentAccountId) return '请选择账户'
      return '创建条件单'
    }
  },

  watch: {
    currentAccountId: {
      immediate: true,
      handler(newVal) {
        if (newVal) {
          this.loadConditionalOrders()
        }
      }
    },

    // 自动设置默认触发条件
    'form.condition_type': {
      handler(newVal) {
        if (newVal === 'StopLoss') {
          // 止损默认: 价格跌破触发价
          this.form.trigger_condition = 'LessOrEqual'
          this.form.direction = 'SELL'
          this.form.offset = 'CLOSE'
        } else if (newVal === 'TakeProfit') {
          // 止盈默认: 价格涨到触发价
          this.form.trigger_condition = 'GreaterOrEqual'
          this.form.direction = 'SELL'
          this.form.offset = 'CLOSE'
        }
      }
    }
  },

  methods: {
    handleAccountChanged(accountId) {
      console.log('[ConditionalOrderForm] Account changed to:', accountId)
      this.errorMessage = ''
      this.successMessage = ''
      if (accountId) {
        this.loadConditionalOrders()
      }
    },

    async loadConditionalOrders() {
      if (!this.currentAccountId) return
      try {
        this.loading = true
        const res = await getConditionalOrders(this.currentAccountId)
        if (res.data && res.data.success) {
          this.conditionalOrders = res.data.data || []
        }
      } catch (error) {
        console.error('[ConditionalOrderForm] Failed to load orders:', error)
      } finally {
        this.loading = false
      }
    },

    async handleSubmit() {
      this.errorMessage = ''
      this.successMessage = ''

      if (!this.canSubmit) {
        this.errorMessage = '请填写完整的条件单信息'
        return
      }

      // 验证限价单必须填写限价
      if (this.form.order_type === 'LIMIT' && !this.form.limit_price) {
        this.errorMessage = '限价单必须填写委托价格'
        return
      }

      try {
        this.submitting = true

        const data = {
          account_id: this.currentAccountId,
          instrument_id: this.form.instrument_id.trim().toUpperCase(),
          direction: this.form.direction,
          offset: this.form.offset,
          volume: this.form.volume,
          order_type: this.form.order_type,
          condition_type: this.form.condition_type,
          trigger_price: this.form.trigger_price,
          trigger_condition: this.form.trigger_condition
        }

        if (this.form.order_type === 'LIMIT') {
          data.limit_price = this.form.limit_price
        }

        if (this.validUntilInput) {
          data.valid_until = new Date(this.validUntilInput).getTime()
        }

        const res = await createConditionalOrder(data)

        if (res.data && res.data.success) {
          this.successMessage = `条件单创建成功！单号: ${res.data.data.conditional_order_id}`
          await this.loadConditionalOrders()

          // 重置触发价格
          this.form.trigger_price = null
          this.validUntilInput = ''
        } else {
          this.errorMessage = (res.data && res.data.error) || '创建失败'
        }
      } catch (error) {
        console.error('[ConditionalOrderForm] Create failed:', error)
        this.errorMessage = `创建失败: ${error.message || '未知错误'}`
      } finally {
        this.submitting = false
      }
    },

    async handleCancelOrder(orderId) {
      if (!confirm('确定要撤销此条件单吗？')) return

      try {
        const res = await cancelConditionalOrder(orderId)
        if (res.data && res.data.success) {
          this.successMessage = '条件单已撤销'
          await this.loadConditionalOrders()
        } else {
          this.errorMessage = (res.data && res.data.error) || '撤销失败'
        }
      } catch (error) {
        console.error('[ConditionalOrderForm] Cancel failed:', error)
        this.errorMessage = `撤销失败: ${error.message || '未知错误'}`
      }
    },

    formatConditionType(type) {
      const map = {
        'StopLoss': '止损',
        'TakeProfit': '止盈',
        'PriceTouch': '触价'
      }
      return map[type] || type
    },

    formatStatus(status) {
      const map = {
        'Pending': '等待触发',
        'Triggered': '已触发',
        'Cancelled': '已撤销',
        'Expired': '已过期',
        'Failed': '执行失败'
      }
      return map[status] || status
    },

    getStatusClass(status) {
      const map = {
        'Pending': 'status-pending',
        'Triggered': 'status-triggered',
        'Cancelled': 'status-cancelled',
        'Expired': 'status-expired',
        'Failed': 'status-failed'
      }
      return map[status] || ''
    }
  }
}
</script>

<style scoped>
.conditional-order-form {
  max-width: 700px;
  margin: 0 auto;
  padding: 20px;
}

.conditional-order-form h3 {
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

.form-row {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 15px;
}

/* 条件类型按钮 */
.condition-type-buttons {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 10px;
}

.condition-btn {
  padding: 10px;
  border: 2px solid #ddd;
  background: white;
  border-radius: 4px;
  font-size: 13px;
  cursor: pointer;
  transition: all 0.3s;
}

.condition-btn:hover {
  border-color: #409EFF;
}

.condition-btn.active {
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
  background: #409EFF;
  color: white;
  border: none;
  border-radius: 4px;
  font-size: 16px;
  font-weight: 500;
  cursor: pointer;
  transition: background 0.3s;
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

/* 条件单列表 */
.conditional-orders {
  margin-top: 30px;
}

.conditional-orders h4 {
  margin: 0 0 15px 0;
  color: #333;
  font-size: 16px;
}

.orders-table {
  width: 100%;
  border-collapse: collapse;
  background: white;
  border-radius: 4px;
  overflow: hidden;
  border: 1px solid #e0e0e0;
}

.orders-table th,
.orders-table td {
  padding: 10px 8px;
  text-align: left;
  border-bottom: 1px solid #eee;
  font-size: 13px;
}

.orders-table th {
  background: #f5f7fa;
  color: #666;
  font-weight: 500;
}

.direction-buy {
  color: #f56c6c;
}

.direction-sell {
  color: #67c23a;
}

.status-pending {
  color: #e6a23c;
}

.status-triggered {
  color: #67c23a;
}

.status-cancelled {
  color: #909399;
}

.status-expired {
  color: #909399;
}

.status-failed {
  color: #f56c6c;
}

.btn-cancel {
  padding: 4px 12px;
  background: #f56c6c;
  color: white;
  border: none;
  border-radius: 3px;
  font-size: 12px;
  cursor: pointer;
}

.btn-cancel:hover {
  background: #f78989;
}
</style>
