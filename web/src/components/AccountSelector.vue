<template>
  <div class="account-selector">
    <!-- 账户选择下拉框 -->
    <div class="account-dropdown" v-if="hasAccounts">
      <label for="account-select">交易账户：</label>
      <select
        id="account-select"
        v-model="selectedAccountId"
        @change="handleAccountChange"
        class="account-select"
      >
        <option
          v-for="account in userAccounts"
          :key="account.account_id"
          :value="account.account_id"
        >
          {{ account.account_name || account.account_id }}
          (余额: ¥{{ formatNumber(account.balance) }})
        </option>
      </select>
    </div>

    <!-- 无账户提示 -->
    <div class="no-accounts-warning" v-else>
      <p>⚠️ 您还没有交易账户，请先开户</p>
      <button @click="handleOpenAccount" class="btn-open-account">
        立即开户
      </button>
    </div>

    <!-- 当前账户详情（可选） -->
    <div class="account-details" v-if="currentAccount">
      <div class="detail-item">
        <span class="label">账户类型：</span>
        <span class="value">{{ formatAccountType(currentAccount.account_type) }}</span>
      </div>
      <div class="detail-item">
        <span class="label">可用资金：</span>
        <span class="value">¥{{ formatNumber(currentAccount.available) }}</span>
      </div>
      <div class="detail-item">
        <span class="label">冻结资金：</span>
        <span class="value">¥{{ formatNumber(currentAccount.balance - currentAccount.available) }}</span>
      </div>
      <div class="detail-item">
        <span class="label">保证金：</span>
        <span class="value">¥{{ formatNumber(currentAccount.margin) }}</span>
      </div>
      <div class="detail-item">
        <span class="label">风险度：</span>
        <span class="value" :class="getRiskClass(currentAccount.risk_ratio)">
          {{ (currentAccount.risk_ratio * 100).toFixed(2) }}%
        </span>
      </div>
    </div>
  </div>
</template>

<script>
import { mapState, mapGetters, mapActions } from 'vuex'

export default {
  name: 'AccountSelector',

  data() {
    return {
      selectedAccountId: null
    }
  },

  computed: {
    ...mapState('websocket', {
      userAccounts: state => state.userAccounts
    }),

    ...mapGetters('websocket', [
      'currentAccountId',
      'currentAccount',
      'hasAccounts'
    ])
  },

  watch: {
    // 当 Vuex 中的 currentAccountId 变化时，更新本地选中值
    currentAccountId: {
      immediate: true,
      handler(newValue) {
        this.selectedAccountId = newValue
      }
    }
  },

  methods: {
    ...mapActions('websocket', ['switchAccount']),

    /**
     * 账户切换处理
     */
    handleAccountChange() {
      if (this.selectedAccountId && this.selectedAccountId !== this.currentAccountId) {
        try {
          this.switchAccount(this.selectedAccountId)
          this.$emit('account-changed', this.selectedAccountId)
          this.$message.success('账户切换成功')
        } catch (error) {
          console.error('Failed to switch account:', error)
          this.$message.error('账户切换失败: ' + error.message)
          // 恢复到之前的选择
          this.selectedAccountId = this.currentAccountId
        }
      }
    },

    /**
     * 开户处理
     */
    handleOpenAccount() {
      // 触发开户事件或导航到开户页面
      this.$emit('open-account')
      // 或者使用路由跳转：
      // this.$router.push('/account/open')
    },

    /**
     * 格式化数字
     */
    formatNumber(value) {
      if (value === undefined || value === null) return '0.00'
      return Number(value).toLocaleString('zh-CN', {
        minimumFractionDigits: 2,
        maximumFractionDigits: 2
      })
    },

    /**
     * 格式化账户类型
     */
    formatAccountType(type) {
      const typeMap = {
        'Individual': '个人账户',
        'Institutional': '机构账户',
        'MarketMaker': '做市商账户'
      }
      return typeMap[type] || type || '未知'
    },

    /**
     * 获取风险度样式类
     */
    getRiskClass(ratio) {
      if (ratio === undefined || ratio === null) return 'risk-normal'
      if (ratio >= 0.8) return 'risk-high'
      if (ratio >= 0.5) return 'risk-medium'
      return 'risk-normal'
    }
  }
}
</script>

<style scoped>
.account-selector {
  padding: 15px;
  background: #f5f5f5;
  border-radius: 4px;
  margin-bottom: 20px;
}

.account-dropdown {
  display: flex;
  align-items: center;
  margin-bottom: 15px;
}

.account-dropdown label {
  font-weight: bold;
  margin-right: 10px;
  white-space: nowrap;
}

.account-select {
  flex: 1;
  padding: 8px 12px;
  border: 1px solid #ddd;
  border-radius: 4px;
  font-size: 14px;
  background: white;
  cursor: pointer;
}

.account-select:hover {
  border-color: #409EFF;
}

.account-select:focus {
  outline: none;
  border-color: #409EFF;
  box-shadow: 0 0 0 2px rgba(64, 158, 255, 0.2);
}

.no-accounts-warning {
  text-align: center;
  padding: 20px;
  background: #fff3cd;
  border: 1px solid #ffeaa7;
  border-radius: 4px;
}

.no-accounts-warning p {
  margin: 0 0 15px 0;
  color: #856404;
  font-size: 14px;
}

.btn-open-account {
  padding: 10px 24px;
  background: #409EFF;
  color: white;
  border: none;
  border-radius: 4px;
  cursor: pointer;
  font-size: 14px;
  transition: background 0.3s;
}

.btn-open-account:hover {
  background: #66b1ff;
}

.account-details {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
  gap: 10px;
  padding: 15px;
  background: white;
  border-radius: 4px;
  border: 1px solid #e0e0e0;
}

.detail-item {
  display: flex;
  justify-content: space-between;
  padding: 5px 0;
}

.detail-item .label {
  color: #666;
  font-size: 13px;
}

.detail-item .value {
  color: #333;
  font-weight: 500;
  font-size: 13px;
}

.risk-normal {
  color: #67C23A;
}

.risk-medium {
  color: #E6A23C;
}

.risk-high {
  color: #F56C6C;
  font-weight: bold;
}
</style>
