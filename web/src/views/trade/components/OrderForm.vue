<template>
  <div class="order-form">
    <el-form :model="form" :rules="rules" ref="form" label-width="80px">
      <el-form-item label="交易账户" prop="account_id">
        <el-select
          v-model="form.account_id"
          placeholder="请选择交易账户"
          style="width: 100%"
          @change="handleAccountChange"
        >
          <el-option
            v-for="account in accounts"
            :key="account.account_id"
            :label="`${account.account_name} (可用: ¥${account.available.toLocaleString()})`"
            :value="account.account_id"
          >
            <div style="display: flex; justify-content: space-between; align-items: center;">
              <span>{{ account.account_name }}</span>
              <span style="color: #8492a6; font-size: 12px;">
                可用: ¥{{ account.available.toLocaleString() }}
              </span>
            </div>
          </el-option>
        </el-select>
      </el-form-item>

      <el-form-item label="合约">
        <el-input v-model="instrumentId" disabled />
      </el-form-item>

      <el-form-item label="方向">
        <el-tag :type="direction === 'BUY' ? 'danger' : 'success'" size="medium">
          {{ direction === 'BUY' ? '买入' : '卖出' }}
        </el-tag>
      </el-form-item>

      <el-form-item label="开平">
        <el-tag type="info" size="medium">
          {{ offset === 'OPEN' ? '开仓' : '平仓' }}
        </el-tag>
      </el-form-item>

      <el-form-item label="订单类型" prop="order_type">
        <el-radio-group v-model="form.order_type" @change="handleTypeChange">
          <el-radio-button label="LIMIT">限价单</el-radio-button>
          <el-radio-button label="MARKET">市价单</el-radio-button>
        </el-radio-group>
      </el-form-item>

      <el-form-item label="价格" prop="price" v-if="form.order_type === 'LIMIT'">
        <el-input-number
          v-model="form.price"
          :min="0"
          :step="0.2"
          :precision="1"
          style="width: 100%"
          controls-position="right"
        >
          <template slot="prepend">¥</template>
        </el-input-number>
        <div class="price-shortcuts">
          <el-button size="mini" @click="setPriceOffset(-5)">-5</el-button>
          <el-button size="mini" @click="setPriceOffset(-2)">-2</el-button>
          <el-button size="mini" @click="setPriceOffset(-1)">-1</el-button>
          <el-button size="mini" type="primary" plain @click="setCurrentPrice">当前</el-button>
          <el-button size="mini" @click="setPriceOffset(1)">+1</el-button>
          <el-button size="mini" @click="setPriceOffset(2)">+2</el-button>
          <el-button size="mini" @click="setPriceOffset(5)">+5</el-button>
        </div>
      </el-form-item>

      <el-form-item label="数量" prop="volume">
        <el-input-number
          v-model="form.volume"
          :min="1"
          :max="100"
          :step="1"
          style="width: 100%"
          controls-position="right"
        >
          <template slot="append">手</template>
        </el-input-number>
        <div class="volume-shortcuts">
          <el-button size="mini" @click="setVolume(1)">1</el-button>
          <el-button size="mini" @click="setVolume(5)">5</el-button>
          <el-button size="mini" @click="setVolume(10)">10</el-button>
          <el-button size="mini" @click="setVolume(20)">20</el-button>
        </div>
      </el-form-item>

      <el-form-item label="预估金额">
        <div class="estimated-amount">
          <span class="amount">¥{{ estimatedAmount.toLocaleString() }}</span>
          <span class="desc">（合约乘数 × 300）</span>
        </div>
      </el-form-item>

      <el-form-item label="保证金">
        <div class="estimated-margin">
          <span class="amount">¥{{ estimatedMargin.toLocaleString() }}</span>
          <span class="desc">（按15%计算）</span>
        </div>
      </el-form-item>

      <el-form-item>
        <el-button
          type="primary"
          :class="direction === 'BUY' ? 'buy-button' : 'sell-button'"
          style="width: 100%"
          size="large"
          @click="handleSubmit"
          :loading="submitting"
        >
          {{ direction === 'BUY' ? '买入' : '卖出' }}{{ offset === 'OPEN' ? '开仓' : '平仓' }}
        </el-button>
      </el-form-item>
    </el-form>
  </div>
</template>

<script>
import { getUserAccounts } from '@/api'
import { mapGetters } from 'vuex'

export default {
  name: 'OrderForm',
  props: {
    instrumentId: {
      type: String,
      required: true
    },
    currentPrice: {
      type: Number,
      default: 0
    },
    direction: {
      type: String,
      required: true,
      validator: val => ['BUY', 'SELL'].includes(val)
    },
    offset: {
      type: String,
      required: true,
      validator: val => ['OPEN', 'CLOSE'].includes(val)
    }
  },
  data() {
    return {
      submitting: false,
      accounts: [],
      selectedAccount: null,
      form: {
        account_id: '',
        order_type: 'LIMIT',
        price: 3800,
        volume: 1
      },
      rules: {
        account_id: [{ required: true, message: '请选择交易账户', trigger: 'change' }],
        order_type: [{ required: true, message: '请选择订单类型' }],
        price: [{ required: true, message: '请输入价格' }],
        volume: [
          { required: true, message: '请输入数量' },
          { type: 'number', min: 1, max: 100, message: '数量范围 1-100' }
        ]
      }
    }
  },
  computed: {
    ...mapGetters(['currentUser']),
    estimatedAmount() {
      if (this.form.order_type === 'LIMIT') {
        return this.form.price * this.form.volume * 300
      } else {
        return this.currentPrice * this.form.volume * 300
      }
    },
    estimatedMargin() {
      return this.estimatedAmount * 0.15  // 15% 保证金率
    }
  },
  mounted() {
    this.loadAccounts()
  },
  watch: {
    currentPrice: {
      immediate: true,
      handler(val) {
        if (val && this.form.price === 0) {
          this.form.price = val
        }
      }
    }
  },
  methods: {
    async loadAccounts() {
      if (!this.currentUser) {
        this.$message.warning('请先登录')
        return
      }

      try {
        const res = await getUserAccounts(this.currentUser)
        this.accounts = res.accounts || []

        // 自动选择第一个账户
        if (this.accounts.length > 0 && !this.form.account_id) {
          this.form.account_id = this.accounts[0].account_id
          this.selectedAccount = this.accounts[0]
          // 通知父组件初始账户选择
          this.$emit('account-change', this.form.account_id)
        }
      } catch (error) {
        this.$message.error('加载账户列表失败: ' + (error.message || '未知错误'))
      }
    },

    handleAccountChange(accountId) {
      this.selectedAccount = this.accounts.find(acc => acc.account_id === accountId)
      // 通知父组件账户选择变化
      this.$emit('account-change', accountId)
    },

    handleTypeChange() {
      if (this.form.order_type === 'LIMIT' && this.form.price === 0) {
        this.form.price = this.currentPrice
      }
    },

    setCurrentPrice() {
      this.form.price = this.currentPrice
    },

    setPriceOffset(offset) {
      this.form.price = Math.max(0, this.form.price + offset * 0.2)
    },

    setVolume(volume) {
      this.form.volume = volume
    },

    handleSubmit() {
      this.$refs.form.validate(valid => {
        if (valid) {
          this.submitting = true

          // ✨ 交易所模式：user_id 和 account_id 都使用账户ID @yutiansut @quantaxis
          // 原因：交易所只关心账户，User→Account映射是经纪商业务
          const orderData = {
            user_id: this.form.account_id,    // 交易所模式：使用账户ID
            account_id: this.form.account_id, // 交易账户ID
            direction: this.direction,
            offset: this.offset,
            order_type: this.form.order_type,
            price: this.form.order_type === 'LIMIT' ? this.form.price : 0,
            volume: this.form.volume
          }

          this.$emit('submit', orderData)

          setTimeout(() => {
            this.submitting = false
          }, 1000)
        }
      })
    }
  }
}
</script>

<style lang="scss" scoped>
// @yutiansut @quantaxis - 深色主题订单表单
$dark-bg-tertiary: #21262d;
$dark-border: #30363d;
$dark-text-primary: #f0f6fc;
$dark-text-secondary: #8b949e;
$dark-text-muted: #6e7681;
$primary-color: #1890ff;
$buy-color: #f5222d;
$sell-color: #52c41a;

.order-form {
  padding: 20px 10px;

  // 表单标签颜色
  ::v-deep .el-form-item__label {
    color: $dark-text-secondary !important;
  }

  // 输入框样式
  ::v-deep .el-input__inner,
  ::v-deep .el-select .el-input__inner {
    background: $dark-bg-tertiary !important;
    border-color: $dark-border !important;
    color: $dark-text-primary !important;

    &::placeholder {
      color: $dark-text-muted;
    }

    &:focus {
      border-color: $primary-color !important;
    }
  }

  // 输入框数字控件
  ::v-deep .el-input-number {
    .el-input__inner {
      text-align: left;
      background: $dark-bg-tertiary !important;
      border-color: $dark-border !important;
      color: $dark-text-primary !important;
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

  // 单选按钮组
  ::v-deep .el-radio-group {
    .el-radio-button__inner {
      background: $dark-bg-tertiary;
      border-color: $dark-border;
      color: $dark-text-secondary;
    }

    .el-radio-button__orig-radio:checked + .el-radio-button__inner {
      background: $primary-color;
      border-color: $primary-color;
      color: white;
    }
  }

  .price-shortcuts,
  .volume-shortcuts {
    margin-top: 10px;
    display: flex;
    gap: 5px;

    .el-button {
      background: $dark-bg-tertiary;
      border-color: $dark-border;
      color: $dark-text-secondary;

      &:hover {
        border-color: $primary-color;
        color: $primary-color;
      }

      &.el-button--primary {
        background: $primary-color;
        border-color: $primary-color;
        color: white;
      }
    }
  }

  .estimated-amount,
  .estimated-margin {
    .amount {
      font-size: 18px;
      font-weight: 600;
      color: $dark-text-primary;
      font-family: 'JetBrains Mono', monospace;
    }

    .desc {
      font-size: 12px;
      color: $dark-text-muted;
      margin-left: 10px;
    }
  }

  .buy-button {
    background: $buy-color !important;
    border-color: $buy-color !important;
    font-size: 16px;
    font-weight: 600;

    &:hover {
      background: lighten($buy-color, 10%) !important;
      border-color: lighten($buy-color, 10%) !important;
    }
  }

  .sell-button {
    background: $sell-color !important;
    border-color: $sell-color !important;
    font-size: 16px;
    font-weight: 600;

    &:hover {
      background: lighten($sell-color, 10%) !important;
      border-color: lighten($sell-color, 10%) !important;
    }
  }
}
</style>
