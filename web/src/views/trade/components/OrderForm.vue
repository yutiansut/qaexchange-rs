<template>
  <!-- 紧凑下单面板 @yutiansut @quantaxis
       原布局用 el-form 纵向堆了 9 个 form-item(账户/合约/方向/开平/类型/
       价格/数量/预估金额/保证金/按钮),每项独占一行、label 靠左 ——
       在交易面板里这样纵向拉得过长,信息密度太低。
       改为:① 顶部一行摘要(合约+方向+可用) ② 类型与价格/数量并排
             ③ 快捷键贴在输入框下方 ④ 费用估算合成一行 ⑤ 按钮吸底 -->
  <div class="order-form" :class="direction === 'BUY' ? 'is-buy' : 'is-sell'">
    <!-- ① 摘要条:把「合约 / 方向 / 开平」三个只读 form-item 压成一行 -->
    <div class="of-summary">
      <span class="of-inst">{{ instrumentId || '—' }}</span>
      <span class="of-side" :class="direction === 'BUY' ? 'side-buy' : 'side-sell'">
        {{ direction === 'BUY' ? '买入' : '卖出' }}{{ offset === 'OPEN' ? '开仓' : '平仓' }}
      </span>
      <span class="of-avail" v-if="currentAccount">
        可用 ¥{{ $money(currentAccount.available, 0) }}
      </span>
    </div>

    <el-form :model="form" :rules="rules" ref="form" label-position="top" size="small">
      <!-- ② 账户 + 订单类型 同一行 -->
      <div class="of-row">
        <el-form-item label="交易账户" prop="account_id" class="of-grow">
          <el-select
            v-model="form.account_id"
            placeholder="选择账户"
            style="width: 100%"
            @change="handleAccountChange"
          >
            <el-option
              v-for="account in accounts"
              :key="account.account_id"
              :label="account.account_name"
              :value="account.account_id"
            >
              <span>{{ account.account_name }}</span>
              <span class="of-opt-avail">¥{{ $money(account.available, 0) }}</span>
            </el-option>
          </el-select>
        </el-form-item>
        <el-form-item label="订单类型" prop="order_type" class="of-type">
          <el-radio-group v-model="form.order_type" size="small" @change="handleTypeChange">
            <el-radio-button label="LIMIT">限价</el-radio-button>
            <el-radio-button label="MARKET">市价</el-radio-button>
          </el-radio-group>
        </el-form-item>
      </div>

      <!-- ③ 价格 + 数量 同一行,快捷键紧贴各自输入框下方 -->
      <div class="of-row">
        <el-form-item
          label="价格"
          prop="price"
          class="of-half"
          v-if="form.order_type === 'LIMIT'"
        >
          <el-input-number
            v-model="form.price"
            :min="0"
            :step="0.2"
            :precision="1"
            size="small"
            style="width: 100%"
            controls-position="right"
          />
          <div class="of-chips">
            <button type="button" @click="setPriceOffset(-5)">-5</button>
            <button type="button" @click="setPriceOffset(-2)">-2</button>
            <button type="button" @click="setPriceOffset(-1)">-1</button>
            <button type="button" class="chip-primary" @click="setCurrentPrice">现价</button>
            <button type="button" @click="setPriceOffset(1)">+1</button>
            <button type="button" @click="setPriceOffset(2)">+2</button>
            <button type="button" @click="setPriceOffset(5)">+5</button>
          </div>
        </el-form-item>
        <el-form-item
          label="数量（手）"
          prop="volume"
          :class="form.order_type === 'LIMIT' ? 'of-half' : 'of-grow'"
        >
          <el-input-number
            v-model="form.volume"
            :min="1"
            :max="100"
            :step="1"
            size="small"
            style="width: 100%"
            controls-position="right"
          />
          <div class="of-chips">
            <button type="button" @click="setVolume(1)">1</button>
            <button type="button" @click="setVolume(5)">5</button>
            <button type="button" @click="setVolume(10)">10</button>
            <button type="button" @click="setVolume(20)">20</button>
          </div>
        </el-form-item>
      </div>

      <!-- ④ 费用估算:原来是两个独占一行的 form-item,压成一行两列 -->
      <div class="of-estimate">
        <div class="of-est-item">
          <span class="est-label">预估金额</span>
          <span class="est-value" v-if="estimatedAmount === null">—</span>
          <span class="est-value" v-else>¥{{ estimatedAmount.toLocaleString() }}</span>
        </div>
        <div class="of-est-item">
          <span class="est-label">预估保证金</span>
          <span class="est-value" v-if="estimatedMargin === null">—</span>
          <span class="est-value" v-else>¥{{ estimatedMargin.toLocaleString() }}</span>
        </div>
      </div>

      <!-- ⑤ 提交按钮吸底
           :disabled="!contractMultiplier" —— 规格拿不到时禁止下单:
           用户看不到可信的预估金额/保证金,不该在这种状态下让他提交。
           @yutiansut @quantaxis -->
      <el-button
        type="primary"
        class="of-submit"
        :class="direction === 'BUY' ? 'buy-button' : 'sell-button'"
        @click="handleSubmit"
        :loading="submitting"
        :disabled="!contractMultiplier"
      >
        {{ direction === 'BUY' ? '买入' : '卖出' }}{{ offset === 'OPEN' ? '开仓' : '平仓' }}
      </el-button>
    </el-form>
  </div>
</template>

<script>
import { getUserAccounts, getInstrumentSpecs } from '@/api'
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
      instrumentSpecs: {},   // instrument_id → {multiplier, margin_rate}
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
    // 当前选中账户 —— 摘要条显示可用资金用 @yutiansut @quantaxis
    currentAccount() {
      return this.accounts.find(a => a.account_id === this.form.account_id) || null
    },

    ...mapGetters(['currentUser']),
    // ✨ 乘数与保证金率从后端取,不要硬编码 @yutiansut @quantaxis
    //
    // 原实现写死 `× 300` 与 `× 0.15`,两个都错:
    //   · IC2501 的合约乘数是 **200**(main.rs:542),不是 300;
    //   · 实测真实保证金率 IF=10% / IH=5%,不是 15%。
    // 叠加后预估保证金偏差 +50% ~ +200% ——
    // 而这是用户**下单前**看到的数字,会直接误导仓位决策。
    // 同一个病后端也犯过三次(monitoring / models / 周期报告),见 lessons.md L45。
    contractMultiplier() {
      const spec = this.instrumentSpecs[this.instrumentId]
      // ⚠️ 拿不到规格就返回 null,**不要**编一个数字。@yutiansut @quantaxis
      // 实测:IC2501 价 6000 × 1 手,真值(mult=200/rate=0.12)预估金额
      // ¥1,200,000、保证金 ¥144,000;规格接口 500 时回退到 300/0.15 变成
      // ¥1,800,000、¥270,000 —— **+50% / +87.5%**,而这是用户**下单前**
      // 用来决定仓位的数字,唯一提示还是一条与之无关的红条。
      return (spec && spec.multiplier) || null
    },
    marginRate() {
      const spec = this.instrumentSpecs[this.instrumentId]
      return (spec && spec.margin_rate) || null
    },
    estimatedAmount() {
      const px = this.form.order_type === 'LIMIT' ? this.form.price : this.currentPrice
      if (!this.contractMultiplier) return null
      return px * this.form.volume * this.contractMultiplier
    },
    estimatedMargin() {
      if (this.estimatedAmount === null || !this.marginRate) return null
      return this.estimatedAmount * this.marginRate
    }
  },
  mounted() {
    this.loadContractSpecs()
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
    // 取合约规格(乘数/保证金率) @yutiansut @quantaxis
    async loadContractSpecs() {
      try {
        const res = await getInstrumentSpecs()
        const list = Array.isArray(res) ? res : (res && res.data) || []
        const map = {}
        list.forEach(x => {
          map[x.instrument_id] = {
            multiplier: x.contract_multiplier,
            margin_rate: x.margin_rate
          }
        })
        this.instrumentSpecs = map
      } catch (e) {
        this.instrumentSpecs = {}
        this.$message.error('合约规格加载失败,预估金额与保证金暂不可用')
        console.warn('[OrderForm] 合约规格加载失败:', e && e.message)
      }
    },
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
        this.$message.error('加载账户列表失败: ' + (this.$errMsg(error, '未知错误')))
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

          // ⚠️ 复位必须由**父组件在请求结束后**触发,不能用固定定时器。
          // @yutiansut @quantaxis
          //
          // 原写法是 `setTimeout(… , 1000)` —— 与真实请求毫无关系:
          // 请求慢于 1s 时按钮提前恢复(实测能重复提交),快于 1s 时白转。
          // 回验代理实测:loading 在点击后 443ms 亮起、**1444ms 熄灭**
          // (正好 1001ms),而请求 **3000ms 才发出**;
          // 两次点击间隔 1.5s 产生了 **2 个 XHR**,都被后端接受。
          // ⇒ 后端只要慢过 1 秒,主买卖按钮就有重复下单窗口。
          //
          // 改为把 `done` 回调随事件传出,父组件在 `finally` 里调它 ——
          // 守卫与请求生命周期严格绑定。父组件若忘了调,按钮会一直转,
          // 那是**明显可见**的故障,比静默重复下单安全。
          this.$emit('submit', orderData, () => {
            this.submitting = false
          })
        }
      })
    }
  }
}
</script>

<style lang="scss" scoped>
@import '@/styles/variables.scss';
/* 紧凑下单面板样式 @yutiansut @quantaxis
   目标:在不改变任何交易语义的前提下提高信息密度 ——
   原布局 9 个纵向 form-item 约需 620px 高,现约 340px。 */
.order-form {
  --of-accent: #f56c6c;

  &.is-sell { --of-accent: #67c23a; }

  /* ① 摘要条:合约 / 方向 / 可用资金 */
  .of-summary {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
    padding: 8px 10px;
    margin-bottom: 12px;
    background: $dark-bg-tertiary;
    border-left: 3px solid var(--of-accent);
    border-radius: 3px;
    font-size: 13px;

    .of-inst { color: $dark-text-primary; font-weight: 600; font-size: 15px; letter-spacing: .3px; }
    .of-side {
      padding: 1px 7px;
      border-radius: 2px;
      color: #fff;
      font-size: 12px;
      &.side-buy  { background: #f56c6c; }
      &.side-sell { background: #67c23a; }
    }
    .of-avail { margin-left: auto; color: $dark-text-secondary; font-size: 12px; }
  }

  /* ② 行容器:让同类字段并排,窄屏自动堆叠 */
  .of-row {
    display: flex;
    gap: 12px;
    flex-wrap: wrap;
    .of-grow { flex: 1 1 200px; min-width: 0; }
    .of-half { flex: 1 1 calc(50% - 6px); min-width: 0; }
    .of-type { flex: 0 0 auto; }
  }

  /* form-item 压紧:label 置顶且小字号 */
  ::v-deep .el-form-item { margin-bottom: 12px; }
  ::v-deep .el-form-item__label {
    padding: 0 0 2px;
    line-height: 1.4;
    font-size: 12px;
    color: #909399;
  }
  ::v-deep .el-form-item__content { line-height: 1.4; }

  /* ③ 快捷键:用原生 button,避免 el-button 的 margin 把行撑开 */
  .of-chips {
    display: flex;
    gap: 4px;
    margin-top: 5px;
    button {
      flex: 1;
      min-width: 0;
      padding: 2px 0;
      font-size: 11px;
      line-height: 1.6;
      color: $dark-text-secondary;
      background: $dark-bg-tertiary;
      border: 1px solid $dark-border;
      border-radius: 2px;
      cursor: pointer;
      transition: all .15s;
      &:hover { color: var(--of-accent); border-color: var(--of-accent); }
      &.chip-primary {
        color: $primary-color;
        border-color: rgba($primary-color, .5);
        background: rgba($primary-color, .12);
      }
    }
  }

  /* ④ 费用估算:一行两列 */
  .of-estimate {
    display: flex;
    gap: 12px;
    padding: 8px 10px;
    margin-bottom: 12px;
    background: $dark-bg-tertiary;
    border: 1px solid $dark-border;
    border-radius: 3px;
    .of-est-item {
      flex: 1;
      display: flex;
      justify-content: space-between;
      align-items: baseline;
      min-width: 0;
      .est-label { color: $dark-text-secondary; font-size: 12px; }
      .est-value {
        color: $dark-text-primary;
        font-weight: 600;
        font-size: 13px;
        font-variant-numeric: tabular-nums;
        white-space: nowrap;
      }
    }
  }

  /* ⑤ 提交按钮 */
  .of-submit {
    width: 100%;
    height: 40px;
    font-size: 15px;
    font-weight: 600;
    letter-spacing: 2px;
    &.buy-button  { background: #f56c6c; border-color: #f56c6c; }
    &.sell-button { background: #67c23a; border-color: #67c23a; }
  }

  .of-opt-avail { float: right; color: #8492a6; font-size: 12px; }
}

/* ⚠️ 这里原本有一段 html[data-theme='dark'] 的兜底 —— 已删除。
   本项目**不使用 data-theme 切换**:深色就是默认样式,变量定义在
   styles/variables.scss:19-26。写 data-theme 选择器永远不会命中,
   结果是浅色值直接生效(实测 .of-estimate 呈 rgb(250,250,250))。
   @yutiansut @quantaxis */

/* 窄屏:价格与数量各占整行 */
@media (max-width: 768px) {
  .order-form .of-row .of-half { flex: 1 1 100%; }
}
</style>