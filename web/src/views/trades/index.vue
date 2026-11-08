<template>
  <div class="trades-page qa-flex-page">
    <el-card>
      <div slot="header" class="card-header">
        <span>成交记录</span>
        <el-button type="primary" size="small" @click="loadTrades">
          <i class="el-icon-refresh"></i> 刷新
        </el-button>
      </div>

      <el-form :inline="true" size="small">
        <el-form-item label="合约">
          <el-select v-model="queryInstrument" placeholder="选择合约" clearable>
            <el-option label="IF2501" value="IF2501" />
            <el-option label="IF2502" value="IF2502" />
            <el-option label="IC2501" value="IC2501" />
            <el-option label="IH2501" value="IH2501" />
          </el-select>
        </el-form-item>
        <el-form-item label="日期">
          <el-date-picker
            v-model="queryDate"
            type="daterange"
            range-separator="至"
            start-placeholder="开始日期"
            end-placeholder="结束日期"
            value-format="yyyy-MM-dd"
          />
        </el-form-item>
        <el-form-item>
          <el-button type="primary" @click="handleQuery">查询</el-button>
          <el-button @click="handleReset">重置</el-button>
          <el-button @click="handleExport">导出</el-button>
        </el-form-item>
      </el-form>

      <el-row :gutter="20" style="margin-bottom: 20px">
        <el-col :xs="24" :sm="12" :lg="6">
          <el-card shadow="hover">
            <div class="stat-item">
              <div class="stat-label">今日成交</div>
              <div class="stat-value">{{ todayStats.count }}</div>
            </div>
          </el-card>
        </el-col>
        <el-col :xs="24" :sm="12" :lg="6">
          <el-card shadow="hover">
            <div class="stat-item">
              <div class="stat-label">成交金额</div>
              <div class="stat-value">{{ todayStats.volume === null ? "—" : "¥" + formatNumber(todayStats.volume) }}</div>
            </div>
          </el-card>
        </el-col>
        <el-col :xs="24" :sm="12" :lg="6">
          <el-card shadow="hover">
            <div class="stat-item">
              <div class="stat-label">买入笔数</div>
              <div class="stat-value">{{ todayStats.buy_count }}</div>
            </div>
          </el-card>
        </el-col>
        <el-col :xs="24" :sm="12" :lg="6">
          <el-card shadow="hover">
            <div class="stat-item">
              <div class="stat-label">卖出笔数</div>
              <div class="stat-value">{{ todayStats.sell_count }}</div>
            </div>
          </el-card>
        </el-col>
      </el-row>

      <!-- ✨ 表格高度改为跟随视口, 不再写死 500px @yutiansut @quantaxis
           实测 1366x768: 筛选表单+统计卡片占 304px; 原 500px 令表格底部溢出 112px
           var(--qa-content-h) 由 layout/index.vue 统一定义 = 100vh - 56(顶栏) - 40(padding),
           有公告条时自动再减 40px; max(260px, ...) 是极短视口下的兜底 -->
      <!-- ⚠️ 保留 height prop 并固定为 '100%' —— 不要删,也不要写常数 N。
      @yutiansut @quantaxis
      `flex:1` ⇒ `flex-basis:0%`,主轴尺寸由 flex 决定,height 不参与布局
      (实测 tableH 与改前逐位相同);但 `layout.height !== null` 让 Element
      自己做 `fixedBodyHeight = bodyHeight - gutterWidth`、渲染 th.gutter、
      并在 updateColumnsWidth 里扣掉 gutter。删掉它会让 layout.scrollY
      永久为 false,连锁产生冻结列错位/竖条被盖/多余横条/表头滞后四个缺陷。
      常数 N 则是另一个错:所需值随**宽度**跳变(表格上方统计卡换行)。 -->
      <el-table
        class="qa-flex-table"
        :empty-text="$emptyText(loadError, '暂无成交')"
        :data="tradeList"
        border
        stripe
        :height="'100%'"
        :loading="loading"
        show-overflow
      >
        <el-table-column prop="trade_id" label="成交编号" min-width="160" show-overflow-tooltip/>
        <el-table-column prop="order_id" label="订单编号" min-width="200" show-overflow-tooltip/>
        <el-table-column prop="instrument_id" label="合约" min-width="100" show-overflow-tooltip/>
        <el-table-column prop="direction" label="方向" width="80" align="center">
          <template slot-scope="scope">
            <el-tag :type="scope.row.direction === 'BUY' ? 'danger' : 'success'" size="mini">
              {{ scope.row.direction === 'BUY' ? '买入' : '卖出' }}
            </el-tag>
          </template>
        </el-table-column>
        <!-- ⚠️ 三态渲染,不能用 `is_taker ? A : B`。@yutiansut @quantaxis
             后端从账户 QIFI 的 dailytrades 取成交时,**没有 taker 信息**
             (QIFI Trade 规范里没有这个字段,它是撮合实现细节),此时返回 null。
             用二分法会把每一笔都显示成「被动成交」—— 那是编造,
             与本轮删掉的绿色对勾、假趋势徽章同类。null 一律显示 `--`。 -->
        <el-table-column prop="is_taker" label="主动/被动" min-width="80" align="center">
          <template slot-scope="scope">
            <span v-if="scope.row.is_taker === null || scope.row.is_taker === undefined"
                  style="color:#909399">--</span>
            <el-tag v-else :type="scope.row.is_taker ? 'warning' : 'info'" size="mini">
              {{ scope.row.is_taker ? '主动成交' : '被动成交' }}
            </el-tag>
          </template>
        </el-table-column>
        <el-table-column prop="price" label="成交价" min-width="100" align="right" show-overflow-tooltip>
          <template slot-scope="scope">
            {{ $num(scope.row.price, 2) }}
          </template>
        </el-table-column>
        <el-table-column prop="volume" label="成交量" width="80" align="right" show-overflow-tooltip/>
        <!-- show-overflow-tooltip:余量仅 7.2px(实测最大 `¥9,468,000.00` 需 121.8px / 单元格 129px),
                 不足一个数字宽。当前靠 pre_trade_check.rs:76 的
                 `max_order_amount = 10_000_000` 兜着;那是配置项,一旦调高立刻溢出。
             @yutiansut @quantaxis -->
        <el-table-column prop="trade_amount" label="成交额" min-width="130" align="right" show-overflow-tooltip>
          <template slot-scope="scope">
            <span v-if="scope.row.trade_amount === null">—</span>
            <span v-else>¥{{ formatNumber(scope.row.trade_amount) }}</span>
          </template>
        </el-table-column>
        <el-table-column prop="commission" label="手续费" min-width="100" align="right" show-overflow-tooltip>
          <template slot-scope="scope">
            ¥{{ $num(scope.row.commission, 2) }}
          </template>
        </el-table-column>
        <el-table-column prop="trade_time" label="成交时间" min-width="160" />
        <el-table-column label="操作" width="100" fixed="right">
          <template slot-scope="scope">
            <el-button type="text" size="small" @click="handleViewDetail(scope.row)">
              详情
            </el-button>
          </template>
        </el-table-column>
      </el-table>
    </el-card>
  </div>
</template>

<script>
import { queryUserTrades, getInstrumentSpecs } from '@/api'
import { extractErrorMessage } from '@/api/request'
import { mapGetters } from 'vuex'

export default {
  name: 'Trades',
  computed: {
    ...mapGetters(['currentUser'])
  },
  data() {
    return {
      // 加载失败原因(null = 未失败)—— 供表格 empty-text 区分两种空
      loadError: null,
      instrumentMultipliers: {},   // instrument_id → contract_multiplier
      loading: false,
      tradeList: [],
      queryInstrument: '',
      queryDate: null,
      todayStats: {
        count: 0,
        volume: 0,
        buy_count: 0,
        sell_count: 0
      }
    }
  },
  watch: {
    currentUser() {
      this.loadTrades()
    }
  },
  async mounted() {
    // ⚠️ 必须 await —— 两者原先并发,而 `trade_amount` 在 loadTrades() 里
    // 一次性算死写进 tradeList,乘数后到也不会重算。
    // 实测:specs 先返回时 IC2501 成交额 ¥1,200,000(对);
    //      trades 先返回时 ¥1,800,000(+50%),而此时 instrumentMultipliers
    //      里明明已经是 {IC2501:200}。**纯竞态,不需要任何接口失败。**
    // @yutiansut @quantaxis
    await this.loadMultipliers()
    if (this.currentUser) {
      this.loadTrades()
    }
  },
  methods: {
    // 取各合约的真实乘数
    async loadMultipliers() {
      try {
        const res = await getInstrumentSpecs()
        const list = Array.isArray(res) ? res : (res && res.data) || []
        const m = {}
        list.forEach(x => { m[x.instrument_id] = x.contract_multiplier })
        this.instrumentMultipliers = m
      } catch (e) {
        // 拿不到就置空 —— **不要**沿用 300。IC2501 真实乘数是 200,
        // 用 300 会让成交额高报 50% 并汇总进统计卡。
        // 静默给出错值比显示「—」危险得多。@yutiansut @quantaxis
        this.instrumentMultipliers = {}
        this.$message.error('合约乘数加载失败,成交额暂不可计算')
        console.warn('[trades] 合约乘数加载失败:', e && e.message)
      }
    },
      // ✨ 委托全局 `$money`(main.js):判空 → isFinite → **量级上限 1e15**。
      // 全仓曾有 13 个各写各的金额格式化器,**没有一个带量级闸**,
      // 于是 `balance = 1e300` 的污染账户在 6 个页面渲染成 404 个字符
      // (`Number.isFinite(1e300)` 为真,只判 isFinite 挡不住)。@yutiansut @quantaxis
    formatNumber(num) {
      return this.$money(num)
    },

    async loadTrades() {
      if (!this.currentUser) {
        this.$message.warning('请先登录')
        return
      }

      this.loading = true
      try {
        this.loadError = null   // 每次重试先清除上次的失败标记
        // ✨ 把合约与日期筛选**真的传给接口**。@yutiansut @quantaxis
        //
        // 原来是 `queryUserTrades(this.currentUser)` —— 不带任何参数,
        // 而页面上的合约下拉框(:13)与日期选择器(:22)都绑了 v-model、
        // 重置时也清空,**看起来在工作,实际不传**。
        // 回验代理网络层实测:选 IF2502 点查询,请求里一个参数都没有。
        // 后端 `handlers.rs:1389` 的 `match_instrument` + `match_date` 本来就支持。
        const params = {}
        if (this.queryInstrument) params.instrument_id = this.queryInstrument
        if (this.queryDate && this.queryDate.length === 2) {
          params.start_date = this.queryDate[0]
          params.end_date = this.queryDate[1]
        }
        const data = await queryUserTrades(this.currentUser, params)
        const trades = data.trades || []

        // 转换后端数据格式以匹配前端展示
        // @yutiansut @quantaxis - 使用后端返回的 user_direction 字段
        this.tradeList = trades.map(trade => {
        // ✨ 乘数按合约取,不要写死 300 @yutiansut @quantaxis
        //
        // IC2501 的真实乘数是 **200**(main.rs:542),写死 300 会让
        // 该合约的成交额高报 50%,并汇总进「成交金额」统计卡。
        // 这是「自己重算权威量」在本项目的第 6 处(前 5 处见 lessons.md L45)。
        // 乘数缺失时给 null,展示层显示「—」;不要回退到任何数字
        const multiplier = this.instrumentMultipliers[trade.instrument_id] || null

          return {
            trade_id: trade.trade_id,
            order_id: trade.user_order_id, // 使用后端返回的用户订单ID
            instrument_id: trade.instrument_id,
            direction: trade.user_direction, // 直接使用后端返回的用户方向
            offset: 'UNKNOWN', // Backend doesn't provide this, would need order details
            price: trade.price,
            volume: trade.volume,
            trade_amount: multiplier ? trade.price * trade.volume * multiplier : null,
            // ⚠️ 手续费用后端返回的真值,不要自己算。
            // 原来这里写 `* multiplier * 0.0001` —— 那是全系统第三个费率:
            //   qars 真实扣费  preset.calc_commission(IF coeff = 2.301e-05)
            //   通知 payload   0.0003
            //   本处(编的)     0.0001
            // 实测 IF 3798×2 手:真实扣 52.44,而这里显示 227.88(高 4.3 倍)。
            // 后端 TradeRecord / UserTradeView 已补 commission 字段。
            // 保留回退:字段缺失时用旧估算并告警,避免旧版后端上表格空白。
            // @yutiansut @quantaxis
            commission: (trade.commission !== undefined && trade.commission !== null)
              ? trade.commission
              : (console.warn('[trades] 后端未返回 commission,回退估算'),
                 trade.price * trade.volume * multiplier * 0.0001),
            trade_time: new Date(trade.timestamp / 1000000).toLocaleString('zh-CN'),
            is_taker: trade.is_taker, // 是否为主动方
            opposite_order_id: trade.opposite_order_id // 对手方订单ID
          }
        })

        this.calculateStats()
      } catch (error) {
        // ✨ 记录失败原因 —— 否则表格只显示默认「暂无数据」,
        //    与「真的没有数据」完全同形,用户无从判断。@yutiansut @quantaxis
        this.loadError = this.$errMsg(error, '加载失败')
        this.$message.error('加载成交记录失败: ' + extractErrorMessage(error))
        this.tradeList = []
      } finally {
        this.loading = false
      }
    },

    calculateStats() {
      this.todayStats.count = this.tradeList.length
      // ⚠️ 乘数缺失时**整卡置 null 显示「—」**,不要 `|| 0` 累加成 ¥0.00。
      //    实测:specs 接口 500 时「今日成交 2」旁边是「成交金额 ¥0.00」——
      //    一个静默错值,比空着更误导(用户会以为真的成交了 0 元)。
      //    与 `admin/risk.vue:24-60` 的「—」写法保持一致。@yutiansut @quantaxis
      const amounts = this.tradeList.map(t => t.trade_amount)
      this.todayStats.volume = amounts.some(a => a === null || a === undefined)
        ? null
        : amounts.reduce((sum, a) => sum + a, 0)
      this.todayStats.buy_count = this.tradeList.filter(t => t.direction === 'BUY').length
      this.todayStats.sell_count = this.tradeList.filter(t => t.direction === 'SELL').length
    },

    handleQuery() {
      this.loadTrades()
    },

    handleReset() {
      this.queryInstrument = ''
      this.queryDate = null
      this.loadTrades()
    },

    handleExport() {
      this.$message.info('导出功能开发中...')
    },

    // 详情弹窗:用 <pre> 保留缩进 + 限高滚动。@yutiansut @quantaxis
    //
    // 原写法 `this.$alert(JSON.stringify(row, null, 2), ...)` 有两个问题:
    // ① `\n` 与缩进空格在 HTML 里被折叠 —— 精心生成的两空格缩进**完全丢失**,
    //    渲染成一整段跑马文(实测截图 r_positions_detail-row0 / d_accounts_detail-alert)。
    // ② element-ui 的 `.el-message-box{overflow:hidden}`,而
    //    `.el-message-box__wrapper` 的 overflow-y 实测是 **visible** 不是 auto,
    //    且 styles/index.scss 全文 `max-height` 出现次数为 0 ——
    //    内容一旦超高既不能滚动也不会截断,**确定按钮会被挤出视口**。
    //    当前消息体约 340 字符尚有约 4 倍余量,属潜在风险,这里一并兜掉。
    //
    // 用 `dangerouslyUseHTMLString` 就必须自己转义 —— 字段值来自后端,
    // 直接插 HTML 等于把接口数据当模板执行。escapeHtml 覆盖 & < > " ' 五个字符。
    handleViewDetail(row) {
      const esc = s => String(s)
        .replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;').replace(/'/g, '&#39;')
      const pre = '<pre style="max-height:60vh;overflow:auto;margin:0;' +
                  'white-space:pre-wrap;word-break:break-all;font-size:12px;' +
                  'line-height:1.5;text-align:left">' +
                  esc(JSON.stringify(row, null, 2)) + '</pre>'
      this.$alert(pre, '成交详情', {
        confirmButtonText: '确定',
        dangerouslyUseHTMLString: true,
        customClass: 'qa-detail-alert'
      }).catch(() => {
        // $alert 在点 X / Esc 关闭时会 reject('cancel'),不接就产生一条未捕获的
        // unhandledrejection console 错误(点「确定」不会)。回验代理用旧代码
        // 复现过 —— 这是改动前就存在的,顺手补上。@yutiansut @quantaxis
      })
    }
  }
}
</script>

<style lang="scss" scoped>
// @yutiansut @quantaxis - 专业量化交易系统成交记录页面样式
$dark-bg-primary: #0d1117;
$dark-bg-secondary: #161b22;
$dark-bg-card: #1c2128;
$dark-bg-tertiary: #21262d;
$dark-border: #30363d;
$dark-text-primary: #f0f6fc;
$dark-text-secondary: #8b949e;
$primary-color: #1890ff;
$buy-color: #f5222d;
$sell-color: #52c41a;

.trades-page {
  min-height: 100%;
  background: $dark-bg-primary;
  padding: 0;

  // 主卡片
  ::v-deep > .el-card {
    background: $dark-bg-card;
    border: 1px solid $dark-border;
    border-radius: 12px;

    .el-card__header {
      background: $dark-bg-secondary;
      border-bottom: 1px solid $dark-border;
      padding: 16px 20px;
    }

    .el-card__body {
      padding: 20px;
    }
  }

  .card-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    color: $dark-text-primary;
    font-size: 16px;
    font-weight: 600;

    .el-button {
      background: linear-gradient(135deg, $primary-color 0%, #096dd9 100%);
      border: none;
      font-weight: 500;

      &:hover {
        opacity: 0.9;
        transform: translateY(-1px);
      }
    }
  }

  // 筛选表单
  ::v-deep .el-form {
    margin-bottom: 20px;
    padding: 16px;
    background: $dark-bg-secondary;
    border-radius: 8px;
    border: 1px solid $dark-border;

    .el-form-item {
      margin-bottom: 0;
      margin-right: 16px;

      .el-form-item__label {
        color: $dark-text-secondary;
        font-size: 13px;
      }
    }

    .el-input__inner,
    .el-select .el-input__inner {
      background: $dark-bg-tertiary;
      border: 1px solid $dark-border;
      color: $dark-text-primary;

      &:focus {
        border-color: $primary-color;
      }

      &::placeholder {
        color: $dark-text-secondary;
      }
    }

    .el-date-editor {
      .el-range-input {
        background: transparent;
        color: $dark-text-primary;

        &::placeholder {
          color: $dark-text-secondary;
        }
      }

      .el-range-separator {
        color: $dark-text-secondary;
      }
    }

    .el-button--primary {
      background: $primary-color;
      border-color: $primary-color;
    }

    .el-button--default {
      background: $dark-bg-tertiary;
      border-color: $dark-border;
      color: $dark-text-secondary;

      &:hover {
        border-color: $primary-color;
        color: $primary-color;
      }
    }
  }

  // 统计卡片区域
  .el-row {
    margin-bottom: 20px;
  }

  ::v-deep .el-col .el-card {
    background: $dark-bg-secondary;
    border: 1px solid $dark-border;
    border-radius: 10px;
    transition: all 0.3s ease;

    &:hover {
      border-color: rgba($primary-color, 0.5);
      transform: translateY(-2px);
      box-shadow: 0 8px 25px rgba(0, 0, 0, 0.3);
    }

    .el-card__body {
      padding: 20px;
    }
  }

  .stat-item {
    text-align: center;

    .stat-label {
      font-size: 13px;
      color: $dark-text-secondary;
      margin-bottom: 12px;
      text-transform: uppercase;
      letter-spacing: 0.5px;
    }

    .stat-value {
      font-size: 26px;
      font-weight: 700;
      color: $dark-text-primary;
      font-family: 'JetBrains Mono', monospace;
    }
  }

  // 表格样式
  ::v-deep .el-table {
    background: transparent;
    border-radius: 8px;
    overflow: hidden;

    &::before {
      display: none;
    }

    th.el-table__cell {
      background: $dark-bg-secondary;
      border-bottom: 1px solid $dark-border;
      color: $dark-text-secondary;
      font-weight: 600;
      font-size: 12px;
      text-transform: uppercase;
      letter-spacing: 0.5px;
      padding: 14px 0;
    }

    td.el-table__cell {
      background: $dark-bg-card;
      border-bottom: 1px solid $dark-border;
      color: $dark-text-primary;
      padding: 12px 0;
      font-family: 'JetBrains Mono', monospace;
      font-size: 13px;
    }

    tr:hover td.el-table__cell {
      background: $dark-bg-tertiary !important;
    }

    .el-table__body-wrapper {
      &::-webkit-scrollbar {
        width: var(--qa-sbw, 8px);
        height: var(--qa-sbw, 8px);
      }

      &::-webkit-scrollbar-track {
        background: $dark-bg-secondary;
      }

      &::-webkit-scrollbar-thumb {
        background: $dark-border;
        border-radius: 4px;

        &:hover {
          background: #484f58;
        }
      }
    }

    .el-table__fixed-right {
      background: transparent;

      th.el-table__cell,
      td.el-table__cell {
        background: $dark-bg-card;
      }
    }

    .el-table__empty-block {
      background: $dark-bg-card;
    }

    .el-table__empty-text {
      color: $dark-text-secondary;
    }
  }

  // 标签样式
  ::v-deep .el-tag {
    border: none;
    font-weight: 600;
    font-family: 'JetBrains Mono', monospace;
    font-size: 11px;

    &--danger {
      background: rgba($buy-color, 0.15);
      color: $buy-color;
    }

    &--success {
      background: rgba($sell-color, 0.15);
      color: $sell-color;
    }
  }

  // 操作按钮
  ::v-deep .el-button--text {
    color: $primary-color;
    font-size: 13px;

    &:hover {
      color: lighten($primary-color, 10%);
    }
  }
}

// 日期选择器弹出框
::v-deep .el-picker-panel {
  background: $dark-bg-card;
  border: 1px solid $dark-border;
  color: $dark-text-primary;

  .el-date-range-picker__content {
    .el-date-range-picker__header {
      color: $dark-text-primary;

      div {
        color: $dark-text-primary;
      }

      button {
        color: $dark-text-secondary;

        &:hover {
          color: $primary-color;
        }
      }
    }

    .el-date-table {
      th {
        color: $dark-text-secondary;
      }

      td {
        &.available {
          color: $dark-text-primary;

          &:hover {
            color: $primary-color;
          }
        }

        &.in-range {
          background: rgba($primary-color, 0.2);
        }

        &.start-date,
        &.end-date {
          .el-date-table-cell__text {
            background: $primary-color;
          }
        }

        &.today {
          .el-date-table-cell__text {
            color: $primary-color;
          }
        }
      }
    }
  }

  .el-date-range-picker__time-header {
    border-color: $dark-border;

    .el-input__inner {
      background: $dark-bg-tertiary;
      border-color: $dark-border;
      color: $dark-text-primary;
    }
  }
}

// 下拉菜单全局
::v-deep .el-select-dropdown {
  background: $dark-bg-card !important;
  border: 1px solid $dark-border !important;

  .el-select-dropdown__item {
    color: $dark-text-primary;

    &:hover {
      background: $dark-bg-tertiary;
    }

    &.selected {
      color: $primary-color;
    }
  }
}
</style>
