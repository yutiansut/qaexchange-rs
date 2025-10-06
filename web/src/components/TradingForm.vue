<template>
  <div class="trading-form">
    <h3>下单交易</h3>

    <!-- ✨ Phase 10: 账户选择器 -->
    <account-selector @account-changed="handleAccountChanged" />

    <!-- 交易表单 -->
    <form @submit.prevent="handleSubmit" class="form">
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
          </select>
        </div>
      </div>

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
          <label for="price_type">价格类型：</label>
          <select id="price_type" v-model="form.price_type" required>
            <option value="LIMIT">限价</option>
            <option value="MARKET">市价</option>
            <option value="ANY">任意价</option>
          </select>
        </div>
      </div>

      <div class="form-group" v-if="form.price_type === 'LIMIT'">
        <label for="limit_price">限价：</label>
        <input
          type="number"
          id="limit_price"
          v-model.number="form.limit_price"
          step="0.01"
          :required="form.price_type === 'LIMIT'"
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
  </div>
</template>

<script>
import { mapState, mapGetters, mapActions } from 'vuex'
import AccountSelector from './AccountSelector.vue'

export default {
  name: 'TradingForm',

  components: {
    AccountSelector
  },

  data() {
    return {
      form: {
        instrument_id: '',
        direction: 'BUY',
        offset: 'OPEN',
        volume: 1,
        price_type: 'LIMIT',
        limit_price: null
      },
      errorMessage: '',
      successMessage: '',
      submitting: false
    }
  },

  computed: {
    ...mapGetters('websocket', [
      'isConnected',
      'currentAccountId',
      'currentAccount',
      'hasAccounts'
    ]),

    /**
     * 是否可以提交
     */
    canSubmit() {
      return (
        this.isConnected &&
        this.hasAccounts &&
        this.currentAccountId &&
        !this.submitting &&
        this.form.instrument_id.trim() !== ''
      )
    },

    /**
     * 提交按钮文本
     */
    submitButtonText() {
      if (this.submitting) return '提交中...'
      if (!this.isConnected) return 'WebSocket 未连接'
      if (!this.hasAccounts) return '请先开户'
      if (!this.currentAccountId) return '请选择账户'
      return '提交订单'
    }
  },

  methods: {
    ...mapActions('websocket', ['insertOrder']),

    /**
     * 账户切换处理
     */
    handleAccountChanged(accountId) {
      console.log('[TradingForm] Account changed to:', accountId)
      this.errorMessage = ''
      this.successMessage = ''
    },

    /**
     * 提交订单
     */
    async handleSubmit() {
      // 清除之前的消息
      this.errorMessage = ''
      this.successMessage = ''

      // 验证
      if (!this.canSubmit) {
        this.errorMessage = this.submitButtonText
        return
      }

      // 验证限价单必须填写限价
      if (this.form.price_type === 'LIMIT' && !this.form.limit_price) {
        this.errorMessage = '限价单必须填写限价'
        return
      }

      try {
        this.submitting = true

        // 构造订单对象（account_id 会在 Vuex action 中自动添加）
        const order = {
          instrument_id: this.form.instrument_id.trim().toUpperCase(),
          direction: this.form.direction,
          offset: this.form.offset,
          volume: this.form.volume,
          price_type: this.form.price_type
        }

        // 如果是限价单，添加限价
        if (this.form.price_type === 'LIMIT') {
          order.limit_price = this.form.limit_price
        }

        // ✨ Phase 10: 提交订单（account_id 会自动从 currentAccountId 填充）
        const orderId = await this.insertOrder(order)

        this.successMessage = `订单提交成功！订单号: ${orderId}`
        console.log('[TradingForm] Order submitted:', orderId)

        // 清空表单（可选）
        // this.resetForm()

      } catch (error) {
        console.error('[TradingForm] Order submission failed:', error)
        this.errorMessage = `订单提交失败: ${error.message || '未知错误'}`
      } finally {
        this.submitting = false
      }
    },

    /**
     * 重置表单
     */
    resetForm() {
      this.form = {
        instrument_id: '',
        direction: 'BUY',
        offset: 'OPEN',
        volume: 1,
        price_type: 'LIMIT',
        limit_price: null
      }
    }
  }
}
</script>

<style scoped>
.trading-form {
  max-width: 600px;
  margin: 0 auto;
  padding: 20px;
}

.trading-form h3 {
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
  margin-bottom: 5px;
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

.btn-submit:active:not(:disabled) {
  background: #3a8ee6;
}

.btn-disabled {
  background: #c0c4cc;
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
  background: #f0f9ff;
  border: 1px solid #c6f6d5;
  border-radius: 4px;
  color: #67c23a;
  font-size: 14px;
}
</style>
