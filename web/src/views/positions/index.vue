<template>
  <div class="positions-page qa-flex-page">
    <el-card>
      <div slot="header" class="card-header">
        <span>持仓管理</span>
        <div>
          <!-- ✨ 账户筛选 —— 本页此前把**该用户名下所有账户**的持仓混在一张表里。
               `queryPosition(this.currentUser)` 传的是用户 UUID,后端返回跨账户记录,
               而页面从不过滤(`account_id` 只在建行时存下来给平仓弹窗用)。
               子代理实测同一时刻同一账户:本页 Σ占用保证金 13,082,435.40,
               而 /margin、/admin-* 与详情页均为 103,935.00 —— **差 126 倍**;
               浮动盈亏 −29,753,124.00 vs +10,200.00,**连符号都相反**;
               IF2501 多头出现**两行**(分属两个账户)却无从分辨。
               页面内部是自洽的(卡片 == 列求和),所以只查「卡片对不对得上表格」
               的检查会漏掉它 —— 错的是**行集本身**。
               取 /orders 的既有做法(下拉筛选),不自造。@yutiansut @quantaxis -->
          <el-select
            v-model="accountFilter"
            size="small"
            clearable
            placeholder="全部账户"
            style="width: 220px; margin-right: 8px"
          >
            <el-option
              v-for="a in accountOptions"
              :key="a"
              :label="a.slice(0, 12) + '…'"
              :value="a"
            />
          </el-select>
          <el-button type="primary" size="small" @click="loadPositions">
            <i class="el-icon-refresh"></i> 刷新
          </el-button>
        </div>
      </div>

      <el-row :gutter="20" style="margin-bottom: 20px">
        <el-col :xs="24" :sm="12" :lg="6">
          <el-card shadow="hover">
            <div class="stat-item">
              <div class="stat-label">总持仓市值</div>
              <div class="stat-value">¥{{ formatNumber(summary.total_value) }}</div>
            </div>
          </el-card>
        </el-col>
        <el-col :xs="24" :sm="12" :lg="6">
          <el-card shadow="hover">
            <div class="stat-item">
              <div class="stat-label">浮动盈亏</div>
              <div class="stat-value" :class="summary.total_profit >= 0 ? 'profit' : 'loss'">
                {{ summary.total_profit >= 0 ? '+' : '' }}¥{{ formatNumber(summary.total_profit) }}
              </div>
            </div>
          </el-card>
        </el-col>
        <el-col :xs="24" :sm="12" :lg="6">
          <el-card shadow="hover">
            <div class="stat-item">
              <div class="stat-label">持仓品种数</div>
              <div class="stat-value">{{ filteredPositions.length }}</div>
            </div>
          </el-card>
        </el-col>
        <el-col :xs="24" :sm="12" :lg="6">
          <el-card shadow="hover">
            <div class="stat-item">
              <div class="stat-label">盈亏比</div>
              <div class="stat-value" :class="summary.profit_ratio >= 0 ? 'profit' : 'loss'">
                {{ $pct(summary.profit_ratio, 2) }}
              </div>
            </div>
          </el-card>
        </el-col>
      </el-row>

      <!-- 表格高度由 `qa-flex-page` + `qa-flex-table` 弹性接管(styles/index.scss)。
           ⚠️ **必须保留 `:height="'100%'"`,不能删掉 height prop**。@yutiansut @quantaxis
           `flex:1` ⇒ `flex-basis:0%`,主轴尺寸由 flex 决定,`height:100%` 不参与布局;
           但它让 Element 的 `layout.height !== null`,于是 Element 自己会:
             · `fixedBodyHeight = bodyHeight - gutterWidth`(table-layout.js:112)—— 冻结列不错位
             · 渲染 `th.gutter` 并在 `updateColumnsWidth` 里扣掉 8px(:150)—— 表头不滞后
           删掉 height prop 会让 `layout.scrollY` **永久为 false**(:44 提前 return),
           实测后果:冻结列滚到底错位 8px、竖条被盖、宽屏凭空多一条 8px 横向滚动条、
           滚到最右时表头比表体少滚 8px 导致所有列错开。
           ⚠️ 也不要退回 `:height="calc(--qa-content-h - N)"` 的常数写法 ——
           受控实测证明所需 N 随**宽度**跳变(表格上方统计卡换行)。 -->
      <el-table
        class="qa-flex-table"
        :height="'100%'"
        :empty-text="$emptyText(loadError, '暂无持仓')"
        :data="filteredPositions"
        border
        stripe
        :loading="loading"
        show-overflow
      >
        <el-table-column prop="account_id" label="账户" min-width="120" show-overflow-tooltip>
          <template slot-scope="scope">
            <span>{{ (scope.row.account_id || '').slice(0, 12) }}…</span>
          </template>
        </el-table-column>
        <el-table-column prop="instrument_id" label="合约代码" min-width="120" show-overflow-tooltip/>
        <el-table-column prop="direction" label="方向" width="80" align="center">
          <template slot-scope="scope">
            <el-tag :type="scope.row.direction === 'LONG' ? 'danger' : 'success'" size="mini">
              {{ scope.row.direction === 'LONG' ? '多头' : '空头' }}
            </el-tag>
          </template>
        </el-table-column>
        <el-table-column prop="volume" label="持仓量" min-width="100" align="right" show-overflow-tooltip/>
        <el-table-column prop="available" label="可平量" min-width="100" align="right" show-overflow-tooltip/>
        <el-table-column prop="open_price" label="开仓均价" min-width="120" align="right" show-overflow-tooltip>
          <template slot-scope="scope">
            {{ $num(scope.row.open_price, 2) }}
          </template>
        </el-table-column>
        <el-table-column prop="last_price" label="最新价" min-width="120" align="right" show-overflow-tooltip>
          <template slot-scope="scope">
            {{ $num(scope.row.last_price, 2) }}
          </template>
        </el-table-column>
        <!-- show-overflow-tooltip:该列实测被 ellipsis 截断且无提示,hover 也弹不出全文。
             不加宽是因为这张表在 1280 下已横向溢出,加宽会加剧;
             tooltip 让内容 hover 可达而不增加列宽。@yutiansut @quantaxis -->
        <el-table-column prop="position_value" label="持仓市值" min-width="130" align="right" show-overflow-tooltip>
          <template slot-scope="scope">
            ¥{{ formatNumber(scope.row.position_value) }}
          </template>
        </el-table-column>
        <el-table-column prop="profit" label="浮动盈亏" min-width="130" align="right" show-overflow-tooltip>
          <template slot-scope="scope">
            <span :class="scope.row.profit >= 0 ? 'profit-text' : 'loss-text'">
              {{ scope.row.profit >= 0 ? '+' : '' }}¥{{ formatNumber(scope.row.profit) }}
            </span>
          </template>
        </el-table-column>
        <el-table-column prop="profit_ratio" label="盈亏比" min-width="100" align="right" show-overflow-tooltip>
          <template slot-scope="scope">
            <span :class="scope.row.profit_ratio >= 0 ? 'profit-text' : 'loss-text'">
              {{ scope.row.profit_ratio >= 0 ? '+' : '' }}{{ $pct(scope.row.profit_ratio, 2) }}
            </span>
          </template>
        </el-table-column>
        <el-table-column prop="margin" label="占用保证金" min-width="130" align="right" show-overflow-tooltip>
          <template slot-scope="scope">
            ¥{{ formatNumber(scope.row.margin) }}
          </template>
        </el-table-column>
        <el-table-column label="操作" width="150">
          <template slot-scope="scope">
            <el-button
              type="text"
              size="small"
              @click="handleClosePosition(scope.row)"
            >
              平仓
            </el-button>
            <el-button type="text" size="small" @click="handleViewDetail(scope.row)">
              详情
            </el-button>
          </template>
        </el-table-column>
      </el-table>
    </el-card>

    <!-- 平仓对话框 -->
    <el-dialog title="平仓" :visible.sync="closeDialogVisible" width="400px">
      <el-form :model="closeForm" :rules="closeRules" ref="closeForm" label-width="100px">
        <el-form-item label="合约">
          <el-input v-model="closeForm.instrument_id" disabled />
        </el-form-item>
        <el-form-item label="方向">
          <el-input v-model="closeForm.direction_text" disabled />
        </el-form-item>
        <el-form-item label="可平量">
          <el-input v-model="closeForm.available" disabled />
        </el-form-item>
        <el-form-item label="平仓量" prop="volume">
          <el-input-number
            v-model="closeForm.volume"
            :min="1"
            :max="closeForm.available"
            :step="1"
            style="width: 100%"
          />
        </el-form-item>
        <el-form-item label="平仓类型">
          <el-radio-group v-model="closeForm.close_type">
            <el-radio label="MARKET">市价平仓</el-radio>
            <el-radio label="LIMIT">限价平仓</el-radio>
          </el-radio-group>
        </el-form-item>
        <el-form-item label="限价" v-if="closeForm.close_type === 'LIMIT'">
          <el-input-number
            v-model="closeForm.price"
            :min="0"
            :step="0.2"
            :precision="1"
            style="width: 100%"
          />
        </el-form-item>
      </el-form>
      <div slot="footer">
        <el-button @click="closeDialogVisible = false">取消</el-button>
        <el-button type="primary" @click="handleSubmitClose" :loading="submitting">
          确定平仓
        </el-button>
      </div>
    </el-dialog>
  </div>
</template>

<script>
import { queryPosition, submitOrder, getTick } from '@/api'
import { mapGetters } from 'vuex'

export default {
  name: 'Positions',
  data() {
    return {
      // 加载失败原因(null = 未失败)—— 供表格 empty-text 区分两种空
      loadError: null,
      loading: false,
      submitting: false,
      positionList: [],
      // 账户筛选(空 = 全部);载入后默认取 store 的 currentAccountId
      accountFilter: '',
      closeDialogVisible: false,
      closeForm: {
        account_id: '',  // 添加account_id字段
        instrument_id: '',
        direction: '',
        direction_text: '',
        available: 0,
        volume: 1,
        close_type: 'MARKET',
        price: 0
      },
      // ✨ 补前端校验：清空平仓量 el-input-number 写 undefined → JSON 丢键 → 后端 400
      // `missing field volume`。@yutiansut @quantaxis
      closeRules: {
        volume: [{ required: true, message: '请输入平仓量', trigger: 'blur' }]
      },
      summary: {
        total_value: 0,
        total_profit: 0,
        profit_ratio: 0
      }
    }
  },
  computed: {
    ...mapGetters(['currentUser']),
    // 数据里实际出现过的账户(不额外请求账户列表)
    accountOptions() {
      const seen = []
      this.positionList.forEach(p => {
        if (p.account_id && seen.indexOf(p.account_id) === -1) seen.push(p.account_id)
      })
      return seen
    },
    // 表格与统计卡都以此为准 —— 卡片必须与**看得见的行**一致
    filteredPositions() {
      if (!this.accountFilter) return this.positionList
      return this.positionList.filter(p => p.account_id === this.accountFilter)
    },
    // ⚠️ `positions` 在 **websocket 命名空间**里 —— 直接放进 root 的 mapGetters
    //    会得到 `undefined`,控制台只留一行 `[vuex] unknown getter: positions`,
    //    下面那个 watch **一次都不会触发**(实测 XHR delta = 0、
    //    `_lastPosReload` 恒为 0)。我第一版就是这么写的,等于死代码。
    //    data 里用的是 `positionList`,不撞名,可直接用同名。@yutiansut @quantaxis
    ...mapGetters('websocket', ['positions']),
    // ⚠️ **必须 watch 原始值,不能 watch 对象** @yutiansut @quantaxis
    //
    // Vue 2.7 `Watcher.prototype.run`(vue.runtime.esm.js:3536):
    //     if (value !== this.value || isObject(value) || this.deep) { cb() }
    // —— **只要值是对象就无条件触发**,引用相不相等根本不看。
    // 我一度以为把 getter 的 `|| {}` 换成模块级冻结常量就能压住,
    // 回验实测证明**完全无效**:页面静置 30 秒仍是 15 次 `/position/user`,
    // 间隔精确 2000ms(节流把它变成了一台节拍器)。
    // 控制实验(同页挂三个 watcher,10 次 SET_SNAPSHOT):
    //     watch 对象值      → 触发 10 次
    //     watch keys 数量   → 触发  0 次
    //     watch JSON 字符串 → 触发  0 次
    //
    // 另外 `jsonMergePatch.js:41` 的 applyMergePatch 对 target 做 deepClone,
    // 所以只要 positions 键存在,每条消息都会产出全新对象 —— 身份路线双重失效。
    //
    // 签名只取「合约 + 多空量」:持仓明细的其余字段(浮盈、最新价)每个 tick 都在变,
    // 纳入签名等于没节流;而本 watch 的目的是「持仓结构变了就重拉」。
    positionsSig () {
      const p = this.positions || {}
      const keys = Object.keys(p).sort()
      let sig = ''
      for (const k of keys) {
        const v = p[k] || {}
        // ⚠️ 后端推的是**聚合后**的 volume_long/short,**没有 today/his**
        //    (`trade_gateway.rs:1636`:`vl = volume_long_today + volume_long_his`
        //     在 Rust 侧就加好了)。我第一版签名只读 today/his ⇒ 对真实推送恒为 0
        //     ⇒ 签名退化成「合约键集合的指纹」⇒ 只有**新合约首次出现**才触发一次,
        //     此后成交/加仓/平仓一次都不重拉。风暴治住了,功能治死了。
        //     回验实测:volume_long 5→10→2,签名恒定 `IF2512:0,0,0,0;`,XHR = 0。
        //     @yutiansut @quantaxis
        //
        //    这里兼容两种形状:优先用聚合值,没有再退回 today+his。
        const vl = v.volume_long != null
          ? v.volume_long
          : (v.volume_long_today || 0) + (v.volume_long_his || 0)
        const vs = v.volume_short != null
          ? v.volume_short
          : (v.volume_short_today || 0) + (v.volume_short_his || 0)
        // 冻结量也要进签名:挂平仓单只动冻结,而表格有「可平量 = 持仓 − 冻结」列。
        // 后端已同步补推这两个字段(trade_gateway.rs:1648)。
        sig += k + ':' + vl + ',' + vs + ',' +
               (v.volume_long_frozen || 0) + ',' + (v.volume_short_frozen || 0) + ';'
      }
      return sig
    }
  },
  watch: {
    currentUser() {
      this.loadPositions()
    },
    // 换账户筛选后统计卡必须跟着变 —— 否则卡片仍是全账户合计,
    // 与表格里看得见的行对不上,又变成另一种「自洽的错值」。@yutiansut @quantaxis
    accountFilter() {
      this.calculateSummary()
    },
    // WS 只当**变更信号**,数据仍走原有的 HTTP `queryPosition` @yutiansut @quantaxis
    //
    // 这样接的理由:DIFF 推的 positions 与 `queryPosition` 返回的形状不同
    // (后者在 loadPositions 里还要逐合约 getTick 补 last_price)。
    // 若直接拿 DIFF 数据渲染,等于在页面里维护第二套字段映射 —— 又一处双实现分叉。
    // 用它触发已验证过的加载路径,是最小改动。
    //
    // 必须节流:`onWebSocketMessage`(store/modules/websocket.js:446)在**每条**
    // WS 消息上提交整个 snapshot,不限流会把每个 tick 变成一次 HTTP 重载。
    positionsSig() {
      const now = Date.now()
      if (now - this._lastPosReload < 2000) {
        // 落在窗口内:安排一次尾部刷新,保证最后一次变更不会丢
        if (!this._posReloadTimer) {
          this._posReloadTimer = setTimeout(() => {
            this._posReloadTimer = null
            this._lastPosReload = Date.now()
            if (this.currentUser) this.loadPositions()
          }, 2000 - (now - this._lastPosReload))
        }
        return
      }
      this._lastPosReload = now
      if (this.currentUser) this.loadPositions()
    }
  },
  mounted() {
    // 节流状态挂在实例上而非 data —— 不需要响应式,进 data 会白白触发依赖收集
    this._lastPosReload = 0
    this._posReloadTimer = null
    if (this.currentUser) {
      this.loadPositions()
    }
  },
  beforeDestroy() {
    if (this._posReloadTimer) {
      clearTimeout(this._posReloadTimer)
      this._posReloadTimer = null
    }
  },
  methods: {
      // ✨ 委托全局 `$money`(main.js):判空 → isFinite → **量级上限 1e15**。
      // 全仓曾有 13 个各写各的金额格式化器,**没有一个带量级闸**,
      // 于是 `balance = 1e300` 的污染账户在 6 个页面渲染成 404 个字符
      // (`Number.isFinite(1e300)` 为真,只判 isFinite 挡不住)。@yutiansut @quantaxis
    formatNumber(num) {
      return this.$money(num)
    },

    async loadPositions() {
      if (!this.currentUser) {
        this.$message.warning('请先登录')
        return
      }

      this.loading = true
      try {
        // queryPosition 通过 axios 拦截器已经返回 res.data，直接就是数组
        // ⚠️ 清除标记必须在**主请求之前**,不能放进下面的逐合约循环 ——
        //    `uniqueInstruments` 为空(持仓为空)时那个循环根本不执行,
        //    于是「重试成功但返回空持仓」会残留上一次的错误,
        //    表格明明请求成功却仍显示「加载失败:...500」。@yutiansut @quantaxis
        this.loadError = null
        const rawPositions = await queryPosition(this.currentUser) || []

        const uniqueInstruments = [...new Set(rawPositions.map(pos => pos.instrument_id))]
        const priceMap = {}
        await Promise.all(uniqueInstruments.map(async instrument => {
          try {
            const tick = await getTick(instrument)
            priceMap[instrument] = tick.last_price || tick.bid_price || tick.ask_price || 0
          } catch (error) {
            priceMap[instrument] = 0
          }
        }))

        // 将后台返回的多空合并格式转换为前端需要的格式
        // 后台格式：{ instrument_id, volume_long, volume_short, cost_long, cost_short, profit_long, profit_short }
        // 前端格式：{ instrument_id, direction, volume, available, open_price, last_price, position_value, profit, profit_ratio, margin }
        const positions = []

        rawPositions.forEach(pos => {
          // ✨ 后端现在直接给 last_price,优先用它;其次才用 tick 接口拉的 priceMap
          const lastPrice = pos.last_price || priceMap[pos.instrument_id] || pos.cost_long || pos.cost_short || 0

          // 如果有多头持仓
          if (pos.volume_long > 0) {
            // ✨ 优先用后端给的真实值,拿不到才回退到旧估算 @yutiansut @quantaxis
            //
            // 后端 PositionInfo 现在透出 margin_long/margin_short/last_price/
            // position_cost_long/position_cost_short(qars QA_Position 本来就有)。
            // 旧写法 `volume * lastPrice * 300` 与 `volume * cost * 300 * 0.15`
            // 里的 ×300 只对 IF/IC/IH 成立,15% 更是凭空假设 ——
            // 真实保证金率来自 MarketPreset,每个品种不同。
            // 保留回退分支是为了后端未重编时不炸(字段为 undefined)。
            const positionValue = (pos.position_cost_long != null && pos.position_cost_long > 0)
              ? pos.position_cost_long + (Number(pos.profit_long) || 0)
              : pos.volume_long * lastPrice * 300
            // @yutiansut @quantaxis: 可平量 = 持仓量 - 冻结量
            const frozenLong = pos.volume_long_frozen || 0
            positions.push({
              account_id: pos.account_id,  // 保存account_id（用于平仓）
              instrument_id: pos.instrument_id,
              direction: 'LONG',
              volume: pos.volume_long,
              available: Math.max(0, pos.volume_long - frozenLong), // 可平量 = 持仓 - 冻结
              frozen: frozenLong,
              open_price: pos.cost_long,
              last_price: lastPrice,
              position_value: positionValue,
              profit: pos.profit_long,
              profit_ratio: positionValue > 0 ? pos.profit_long / positionValue : 0,
              margin: (pos.margin_long != null)
                ? pos.margin_long
                : pos.volume_long * pos.cost_long * 300 * 0.15 // 回退:旧的 15% 估算
            })
          }

          // 如果有空头持仓
          if (pos.volume_short > 0) {
            // ✨ 同多头分支:优先用后端真实值
            const positionValue = (pos.position_cost_short != null && pos.position_cost_short > 0)
              ? pos.position_cost_short + (Number(pos.profit_short) || 0)
              : pos.volume_short * lastPrice * 300
            // @yutiansut @quantaxis: 可平量 = 持仓量 - 冻结量
            const frozenShort = pos.volume_short_frozen || 0
            positions.push({
              account_id: pos.account_id,  // 保存account_id（用于平仓）
              instrument_id: pos.instrument_id,
              direction: 'SHORT',
              volume: pos.volume_short,
              available: Math.max(0, pos.volume_short - frozenShort), // 可平量 = 持仓 - 冻结
              frozen: frozenShort,
              open_price: pos.cost_short,
              last_price: lastPrice,
              position_value: positionValue,
              profit: pos.profit_short,
              profit_ratio: positionValue > 0 ? pos.profit_short / positionValue : 0,
              margin: (pos.margin_short != null)
                ? pos.margin_short
                : pos.volume_short * pos.cost_short * 300 * 0.15 // 回退:旧的 15% 估算
            })
          }
        })

        this.positionList = positions
        // ✨ 默认选中「当前账户」,而不是一上来就把所有账户混在一起。
        // 只在该账户确实出现在本次数据里时才选 —— 否则会得到一张空表,
        // 比混在一起更糟。用户可以手动清空筛选看全部。@yutiansut @quantaxis
        if (!this.accountFilter) {
          const cur = this.$store.state.websocket && this.$store.state.websocket.currentAccountId
          if (cur && this.accountOptions.indexOf(cur) !== -1) {
            this.accountFilter = cur
          }
        }
        this.calculateSummary()
      } catch (error) {
        // ✨ 记录失败原因 —— 否则表格只显示默认「暂无数据」,
        //    与「真的没有数据」完全同形,用户无从判断。@yutiansut @quantaxis
        this.loadError = this.$errMsg(error, '加载失败')
        const errorMsg = (error.response && error.response.data && error.response.data.error && error.response.data.error.message) || error.message
        this.$message.error('加载持仓失败: ' + errorMsg)
        this.positionList = []
      } finally {
        this.loading = false
      }
    },

    calculateSummary() {
      this.summary.total_value = this.filteredPositions.reduce((sum, p) => sum + (Number(p.position_value) || 0), 0)
      this.summary.total_profit = this.filteredPositions.reduce((sum, p) => sum + (Number(p.profit) || 0), 0)

        // ✨ 用每项已算好的 position_value,不要再自乘乘数 @yutiansut @quantaxis
        //
        // 原实现 `p.volume * p.open_price * 300` 硬编码乘数,
        // 对 IC 系合约(真实乘数 200)分母偏大 50% → 盈亏比偏小 50%。
        // 而 positionList 的每项在构建时已经用后端的 position_cost_long/short
        // 算出了 position_value(见上方 mapPositions),直接求和即可。
        const total_cost = this.filteredPositions.reduce((sum, p) => {
          return sum + (Number(p.position_value) || 0)
      }, 0)

      this.summary.profit_ratio = total_cost > 0 ? this.summary.total_profit / total_cost : 0
    },

    handleClosePosition(row) {
      this.closeForm = {
        account_id: row.account_id,  // 设置account_id
        instrument_id: row.instrument_id,
        direction: row.direction,
        direction_text: row.direction === 'LONG' ? '多头' : '空头',
        available: row.available,
        volume: row.available,
        close_type: 'MARKET',
        price: row.last_price
      }
      this.closeDialogVisible = true
    },

    handleSubmitClose() {
      // ✨ 先过表单校验（空平仓量在此拦下，不再让缺 volume 的请求打到后端换 400）。@yutiansut @quantaxis
      this.$refs.closeForm.validate(valid => {
        if (valid) this._doSubmitClose()
      })
    },

    _doSubmitClose() {
      this.submitting = true

      // ✨ 交易所模式：user_id 和 account_id 都使用账户ID @yutiansut @quantaxis
      // 原因：交易所只关心账户，User→Account映射是经纪商业务
      const orderData = {
        user_id: this.closeForm.account_id,     // 交易所模式：使用账户ID
        account_id: this.closeForm.account_id,
        instrument_id: this.closeForm.instrument_id,
        direction: this.closeForm.direction === 'LONG' ? 'SELL' : 'BUY',
        offset: 'CLOSE',
        price: this.closeForm.close_type === 'LIMIT' ? this.closeForm.price : 0,
        volume: this.closeForm.volume,
        order_type: this.closeForm.close_type
      }

      submitOrder(orderData)
        .then(() => {
          this.$message.success('平仓委托已提交')
          this.closeDialogVisible = false
          this.loadPositions()
        })
        .catch(err => {
          // ✨ 透出后端原因（如「Invalid order price」），别吞成光秃秃的「平仓失败」。@yutiansut @quantaxis
          this.$message.error(this.$errMsg(err, '平仓失败'))
        })
        .finally(() => {
          this.submitting = false
        })
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
      this.$alert(pre, '持仓详情', {
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
// @yutiansut @quantaxis - 专业量化交易系统持仓页面样式
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

.positions-page {
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
      /* 定值 26px 等宽字体在 1200~1400 宽区间放不下 9 位金额:
         实测 1200 → 差 49px、1280 → 差 29px,显示成 `¥108,431,040.` 且无省略号。
         根因是 :lg="6" 从 1200px 起就排四列,而字号不随之收缩。
         照 dashboard 的做法改用 clamp。@yutiansut @quantaxis */
      font-size: clamp(16px, 1.6vw, 26px);
      font-weight: 700;
      color: $dark-text-primary;
      font-family: 'JetBrains Mono', monospace;

      &.profit {
        color: $buy-color;
        text-shadow: 0 0 20px rgba($buy-color, 0.3);
      }

      &.loss {
        color: $sell-color;
        text-shadow: 0 0 20px rgba($sell-color, 0.3);
      }
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

  // 盈亏文字
  .profit-text {
    color: $buy-color;
    font-weight: 600;
    font-family: 'JetBrains Mono', monospace;
    text-shadow: 0 0 10px rgba($buy-color, 0.2);
  }

  .loss-text {
    color: $sell-color;
    font-weight: 600;
    font-family: 'JetBrains Mono', monospace;
    text-shadow: 0 0 10px rgba($sell-color, 0.2);
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

// 平仓对话框
::v-deep .el-dialog {
  background: $dark-bg-card;
  border-radius: 12px;
  border: 1px solid $dark-border;

  .el-dialog__header {
    background: $dark-bg-secondary;
    border-bottom: 1px solid $dark-border;
    padding: 16px 20px;
    border-radius: 12px 12px 0 0;

    .el-dialog__title {
      color: $dark-text-primary;
      font-weight: 600;
    }

    .el-dialog__headerbtn .el-dialog__close {
      color: $dark-text-secondary;

      &:hover {
        color: $dark-text-primary;
      }
    }
  }

  .el-dialog__body {
    padding: 24px;
    background: $dark-bg-card;
  }

  .el-dialog__footer {
    background: $dark-bg-secondary;
    border-top: 1px solid $dark-border;
    padding: 16px 20px;
    border-radius: 0 0 12px 12px;

    .el-button--default {
      background: $dark-bg-tertiary;
      border-color: $dark-border;
      color: $dark-text-secondary;

      &:hover {
        border-color: $primary-color;
        color: $primary-color;
      }
    }

    .el-button--primary {
      background: linear-gradient(135deg, $buy-color 0%, darken($buy-color, 10%) 100%);
      border: none;
    }
  }

  .el-form-item__label {
    color: $dark-text-secondary;
  }

  .el-input__inner {
    background: $dark-bg-tertiary;
    border: 1px solid $dark-border;
    color: $dark-text-primary;

    &:disabled {
      background: $dark-bg-secondary;
      color: $dark-text-secondary;
    }
  }

  .el-input-number {
    .el-input__inner {
      background: $dark-bg-tertiary;
      border: 1px solid $dark-border;
      color: $dark-text-primary;
    }

    .el-input-number__decrease,
    .el-input-number__increase {
      background: $dark-bg-secondary;
      border-color: $dark-border;
      color: $dark-text-secondary;

      &:hover {
        color: $primary-color;
      }
    }
  }

  .el-radio {
    color: $dark-text-secondary;

    &.is-checked .el-radio__label {
      color: $primary-color;
    }

    .el-radio__inner {
      background: $dark-bg-tertiary;
      border-color: $dark-border;
    }

    &.is-checked .el-radio__inner {
      background: $primary-color;
      border-color: $primary-color;
    }
  }
}
</style>
