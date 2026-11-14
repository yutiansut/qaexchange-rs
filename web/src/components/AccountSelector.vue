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
        <!-- ⚠️ 减法**本身就是一个数值消费点**,不能直接喂给 formatNumber。@yutiansut @quantaxis
             `null - null === 0`(显示成"冻结 0",与真的 0 无法区分),
             `undefined - undefined === NaN`(显示 `¥NaN`)。
             回验代理实测:去掉 store getter 的守卫后,这一行由 `¥0.00` 变成 `¥NaN`
             —— 是我上一轮改动引入的回归。改为**两个操作数都在才相减**。 -->
        <span class="value">¥{{ frozenText }}</span>
      </div>
      <div class="detail-item">
        <span class="label">保证金：</span>
        <span class="value">¥{{ formatNumber(currentAccount.margin) }}</span>
      </div>
      <div class="detail-item">
        <span class="label">风险度：</span>
        <span class="value" :class="getRiskClass(currentAccount.risk_ratio)">
          {{ ((Number(currentAccount.risk_ratio) || 0) * 100).toFixed(2) }}%
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
    // 冻结资金 = 余额 - 可用。两个操作数**都在**才算,否则 `--`。@yutiansut @quantaxis
    frozenText() {
      const b = this.currentAccount && this.currentAccount.balance
      const a = this.currentAccount && this.currentAccount.available
      if (b === null || b === undefined || a === null || a === undefined) return '--'
      const v = Number(b) - Number(a)
      return Number.isFinite(v) ? this.formatNumber(v) : '--'
    },

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
      // ⚠️ 取不到值时返回 `--`,**不是 `0.00`**。@yutiansut @quantaxis
      //
      // 本组件显示的是**余额 / 可用 / 保证金**(:18/:39/:43/:47),
      // 挂在 trade/batch、trade/conditional、trade/transfer 三个下单页上。
      // 字段缺失时显示 `¥0.00`,用户会以为**账户真的没钱**而不是"还没加载"——
      // 在下单页上这个误读的代价很直接。
      //
      // 同仓 `WebSocketTest.vue:843` 与 `websocket/examples/vue-component.vue:270`
      // 的同名函数返回的是 `-`,只有这里返回 `0.00` ——
      // **三套 formatNumber 行为不一致**,这里对齐到「不编造」的那一档。
      // 非有限值(NaN / ±Infinity)也归 `--` —— 回验代理实测 `balance = "N/A"` 时
      // 下拉框会渲染 `¥NaN`。@yutiansut @quantaxis
      //
      // ✨ 现已委托全局 `$money`(main.js):同一判空判据,另加**量级上限 1e15**
      //    (`Number.isFinite(1e300)` 为真,只判 isFinite 挡不住污染账户)。
      return this.$money(value)
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

<style lang="scss" scoped>
// @yutiansut @quantaxis - 账户选择器暗色主题
$dark-bg-primary: #0d1117;
$dark-bg-secondary: #161b22;
$dark-bg-card: #1c2128;
$dark-bg-tertiary: #21262d;
$dark-border: #30363d;
$dark-text-primary: #f0f6fc;
$dark-text-secondary: #8b949e;
$dark-text-muted: #6e7681;
$primary-color: #1890ff;

.account-selector {
  padding: 15px;
  background: $dark-bg-secondary !important;
  border-radius: 4px;
  margin-bottom: 20px;
  border: 1px solid $dark-border !important;
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
  color: $dark-text-primary !important;
}

.account-select {
  flex: 1;
  // ⚠️ flex 子项的 `min-width` 默认是 `auto`,对原生 `<select>` 而言
  //    min-content 宽度 = **最长 option 的宽度**,因此它**无法收缩**。
  //    实测 400px 视口:本组件溢出内容盒 115px,而 `.content-wrapper` 的
  //    `overflow-x` 是 hidden(layout/index.vue:928),溢出部分**永远滚不到** ——
  //    transfer / conditional-orders / batch-orders / trade 等所有嵌入本组件的
  //    页面都受影响,账户下拉框有一截够不着。
  //    这与仓库早前为 `.main-container` 记录并修过的 `min-width:auto` 同根。
  //    @yutiansut @quantaxis
  min-width: 0;
  padding: 8px 12px;
  border: 1px solid $dark-border !important;
  border-radius: 4px;
  font-size: 14px;
  background: $dark-bg-tertiary !important;
  color: $dark-text-primary !important;
  cursor: pointer;
}

.account-select option {
  background: $dark-bg-card !important;
  color: $dark-text-primary !important;
}

.account-select:hover {
  border-color: $primary-color !important;
}

.account-select:focus {
  outline: none;
  border-color: $primary-color !important;
  box-shadow: 0 0 0 2px rgba(24, 144, 255, 0.2);
}

.no-accounts-warning {
  text-align: center;
  padding: 20px;
  background: rgba(230, 162, 60, 0.15) !important;
  border: 1px solid rgba(230, 162, 60, 0.3) !important;
  border-radius: 4px;
}

.no-accounts-warning p {
  margin: 0 0 15px 0;
  color: #e6a23c !important;
  font-size: 14px;
}

.btn-open-account {
  padding: 10px 24px;
  background: $primary-color;
  color: white;
  border: none;
  border-radius: 4px;
  cursor: pointer;
  font-size: 14px;
  transition: background 0.3s;
}

.btn-open-account:hover {
  background: #40a9ff;
}

.account-details {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
  gap: 10px;
  padding: 15px;
  background: $dark-bg-card !important;
  border-radius: 4px;
  border: 1px solid $dark-border !important;
}

.detail-item {
  display: flex;
  justify-content: space-between;
  padding: 5px 0;
}

.detail-item .label {
  color: $dark-text-secondary !important;
  font-size: 13px;
}

.detail-item .value {
  color: $dark-text-primary !important;
  font-weight: 500;
  font-size: 13px;
  font-family: 'JetBrains Mono', monospace;
}

.risk-normal {
  color: #67C23A !important;
}

.risk-medium {
  color: #E6A23C !important;
}

.risk-high {
  color: #F56C6C !important;
  font-weight: bold;
}
</style>
