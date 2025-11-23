<template>
  <div class="close-form" v-loading="loadingPositions">
    <el-alert
      title="平仓说明"
      type="info"
      description="请先查看持仓列表，选择要平仓的合约和方向"
      :closable="false"
      style="margin-bottom: 20px"
    />

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

      <el-form-item label="平仓方向" prop="direction">
        <el-radio-group v-model="form.direction">
          <el-radio-button label="LONG">平多（卖出）</el-radio-button>
          <el-radio-button label="SHORT">平空（买入）</el-radio-button>
        </el-radio-group>
      </el-form-item>

      <el-form-item label="可平量">
        <el-input :value="availableVolume" disabled>
          <template slot="append">手</template>
        </el-input>
        <p v-if="availableVolume === 0" class="available-hint">
          当前账户在该方向没有可平仓位
        </p>
      </el-form-item>

      <el-form-item label="平仓类型" prop="order_type">
        <el-radio-group v-model="form.order_type" @change="handleTypeChange">
          <el-radio-button label="MARKET">市价平仓</el-radio-button>
          <el-radio-button label="LIMIT">限价平仓</el-radio-button>
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
          <el-button size="mini" @click="setPriceOffset(-2)">-2</el-button>
          <el-button size="mini" @click="setPriceOffset(-1)">-1</el-button>
          <el-button size="mini" type="primary" plain @click="setCurrentPrice">当前</el-button>
          <el-button size="mini" @click="setPriceOffset(1)">+1</el-button>
          <el-button size="mini" @click="setPriceOffset(2)">+2</el-button>
        </div>
      </el-form-item>

      <el-form-item label="平仓量" prop="volume">
        <el-input-number
          v-model="form.volume"
          :min="1"
          :max="availableVolume"
          :step="1"
          style="width: 100%"
          controls-position="right"
        >
          <template slot="append">手</template>
        </el-input-number>
        <div class="volume-shortcuts">
          <el-button size="mini" @click="setVolume(1)">1</el-button>
          <el-button size="mini" @click="setVolumePercent(0.5)">50%</el-button>
          <el-button size="mini" type="primary" @click="setVolumePercent(1)">全部</el-button>
        </div>
      </el-form-item>

      <el-form-item>
        <el-button
          type="warning"
          style="width: 100%"
          size="large"
          :disabled="availableVolume === 0"
          @click="handleSubmit"
          :loading="submitting"
        >
          确认平仓
        </el-button>
      </el-form-item>
    </el-form>
  </div>
</template>

<script>
import { getUserAccounts, queryAccountPosition } from '@/api'
import { mapGetters } from 'vuex'

export default {
  name: 'CloseForm',
  props: {
    instrumentId: {
      type: String,
      required: true
    },
    currentPrice: {
      type: Number,
      default: 0
    }
  },
  data() {
    return {
      submitting: false,
      availableVolume: 0,
      loadingPositions: false,
      accounts: [],
      selectedAccount: null,
      positions: [],
      form: {
        account_id: '',
        direction: 'LONG',
        order_type: 'MARKET',
        price: 0,
        volume: 1
      },
      rules: {
        account_id: [{ required: true, message: '请选择交易账户', trigger: 'change' }],
        direction: [{ required: true, message: '请选择平仓方向' }],
        order_type: [{ required: true, message: '请选择平仓类型' }],
        volume: [
          { required: true, message: '请输入平仓量' },
          { type: 'number', min: 1, message: '平仓量至少为1' }
        ]
      }
    }
  },
  computed: {
    ...mapGetters(['currentUser'])
  },
  mounted() {
    this.loadAccounts()
  },
  watch: {
    instrumentId() {
      this.calculateAvailableVolume()
    },
    'form.direction'() {
      this.calculateAvailableVolume()
    },
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
          this.$emit('account-change', this.form.account_id)
          await this.fetchPositions(this.form.account_id)
        }
      } catch (error) {
        this.$message.error('加载账户列表失败: ' + (error.message || '未知错误'))
      }
    },

    async fetchPositions(accountId) {
      if (!accountId) {
        this.positions = []
        this.availableVolume = 0
        return
      }
      this.loadingPositions = true
      try {
        const res = await queryAccountPosition(accountId)
        this.positions = res || []
      } catch (error) {
        this.positions = []
        console.error('加载账户持仓失败', error)
      } finally {
        this.loadingPositions = false
        this.calculateAvailableVolume()
      }
    },

    async handleAccountChange(accountId) {
      this.selectedAccount = this.accounts.find(acc => acc.account_id === accountId)
      this.$emit('account-change', accountId)
      await this.fetchPositions(accountId)
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
      this.form.volume = Math.min(volume, this.availableVolume || 0)
    },

    setVolumePercent(percent) {
      if (!this.availableVolume) {
        this.form.volume = 0
        return
      }
      this.form.volume = Math.max(1, Math.floor(this.availableVolume * percent))
    },

    calculateAvailableVolume() {
      if (!this.instrumentId) {
        this.availableVolume = 0
        this.form.volume = 0
        return
      }
      const position = this.positions.find(p => p.instrument_id === this.instrumentId)
      if (!position) {
        this.availableVolume = 0
        this.form.volume = 0
        return
      }
      const volume = this.form.direction === 'LONG' ? position.volume_long : position.volume_short
      this.availableVolume = Math.max(0, Math.floor(volume))
      if (this.availableVolume === 0) {
        this.form.volume = 0
      } else if (this.form.volume > this.availableVolume) {
        this.form.volume = this.availableVolume
      }
    },

    handleSubmit() {
      this.$refs.form.validate(valid => {
        if (!valid || this.availableVolume === 0) {
          if (this.availableVolume === 0) {
            this.$message.warning('当前没有可平仓位')
          }
          return
        }

        this.submitting = true

        const orderData = {
          user_id: this.currentUser,
          account_id: this.form.account_id,
          direction: this.form.direction === 'LONG' ? 'SELL' : 'BUY',
          offset: 'CLOSE',
          order_type: this.form.order_type,
          price: this.form.order_type === 'LIMIT' ? this.form.price : 0,
          volume: this.form.volume
        }

        this.$emit('submit', orderData)

        setTimeout(() => {
          this.submitting = false
        }, 1000)
      })
    }
  }
}
</script>

<style lang="scss" scoped>
.close-form {
  padding: 20px 10px;

  .price-shortcuts,
  .volume-shortcuts {
    margin-top: 10px;
    display: flex;
    gap: 5px;
  }

  .available-hint {
    margin-top: 6px;
    color: #f56c6c;
    font-size: 12px;
  }

  ::v-deep .el-input-number {
    .el-input__inner {
      text-align: left;
    }
  }
}
</style>
