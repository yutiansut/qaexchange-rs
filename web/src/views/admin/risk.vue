<template>
  <div class="risk-container">
    <!-- 页面标题 -->
    <div class="page-header">
      <h2>风控监控</h2>
      <div class="header-actions">
        <el-button icon="el-icon-refresh" @click="loadData">刷新</el-button>
        <el-switch
          v-model="autoRefresh"
          active-text="自动刷新"
          @change="toggleAutoRefresh"
        ></el-switch>
      </div>
    </div>

    <!-- 风险统计卡片 -->
    <el-row :gutter="20" class="stats-row">
      <el-col :xs="24" :sm="12" :lg="6">
        <div class="stat-card danger">
          <div class="stat-icon">
            <i class="el-icon-warning"></i>
          </div>
          <div class="stat-content">
            <div class="stat-value">{{ statistics ? statistics.highRiskCount : "—" }}</div>
            <div class="stat-label">高风险账户 (>80%)</div>
          </div>
        </div>
      </el-col>

      <el-col :xs="24" :sm="12" :lg="6">
        <div class="stat-card critical">
          <div class="stat-icon">
            <i class="el-icon-warning-outline"></i>
          </div>
          <div class="stat-content">
            <div class="stat-value">{{ statistics ? statistics.criticalRiskCount : "—" }}</div>
            <div class="stat-label">临近爆仓 (>90%)</div>
          </div>
        </div>
      </el-col>

      <el-col :xs="24" :sm="12" :lg="6">
        <div class="stat-card warning">
          <div class="stat-icon">
            <i class="el-icon-s-finance"></i>
          </div>
          <div class="stat-content">
            <div class="stat-value">{{ statistics ? statistics.todayLiquidations : "—" }}</div>
            <div class="stat-label">今日强平次数</div>
          </div>
        </div>
      </el-col>

      <el-col :xs="24" :sm="12" :lg="6">
        <div class="stat-card info">
          <div class="stat-icon">
            <i class="el-icon-data-line"></i>
          </div>
          <div class="stat-content">
            <div class="stat-value">{{ statistics ? $pct(statistics.averageRiskRatio, 1) : "—" }}</div>
            <div class="stat-label">平均风险率</div>
          </div>
        </div>
      </el-col>
    </el-row>

    <!-- 标签页 -->
    <el-tabs v-model="activeTab" class="tabs-container">
      <!-- 实时风险监控 -->
      <el-tab-pane label="实时风险监控" name="realtime">
        <div class="table-toolbar">
          <el-input
            v-model="searchKeyword"
            placeholder="搜索用户ID"
            prefix-icon="el-icon-search"
            style="width: 200px;"
            clearable
          ></el-input>

          <div class="toolbar-right">
            <span style="margin-right: 10px; color: #909399;">
              共 {{ filteredAccounts.length }} 个账户
            </span>
          </div>
        </div>

        <!-- ✨ 表格高度改为跟随视口, 不再写死 500px @yutiansut @quantaxis
             实测 1366x768: 上方 324px; 原 500px 令表格底部溢出 132px
             var(--qa-content-h) 由 layout/index.vue 统一定义 = 100vh - 56(顶栏) - 40(padding),
             有公告条时自动再减 40px; max(260px, ...) 是极短视口下的兜底 -->
        <el-table
          :empty-text="$emptyText(accountsError, '暂无账户')"
          ref="accountTable"
          :data="filteredAccounts"
          border
          stripe
          v-loading="loading"
          :default-sort="{ prop: 'risk_ratio', order: 'descending' }"
          :height="'max(260px, calc(var(--qa-content-h, calc(100vh - 96px)) - 365px))'"
        >
          <el-table-column prop="user_id" label="账户ID" min-width="180" sortable show-overflow-tooltip></el-table-column>
          <el-table-column prop="balance" label="总权益" min-width="150" align="right" sortable show-overflow-tooltip>
            <template slot-scope="scope">
              {{ $money(scope.row.balance) }}
            </template>
          </el-table-column>
          <el-table-column prop="margin_used" label="占用保证金" min-width="130" align="right" sortable show-overflow-tooltip>
            <template slot-scope="scope">
              {{ $money(scope.row.margin_used) }}
            </template>
          </el-table-column>
          <el-table-column prop="available" label="可用资金" min-width="150" align="right" sortable show-overflow-tooltip>
            <template slot-scope="scope">
              {{ $money(scope.row.available) }}
            </template>
          </el-table-column>
          <!-- show-overflow-tooltip:该列实测被 ellipsis 截断且无提示,hover 也弹不出全文。
               不加宽是因为这张表在 1280 下已横向溢出,加宽会加剧;
               tooltip 让内容 hover 可达而不增加列宽。@yutiansut @quantaxis -->
          <el-table-column prop="float_profit" label="浮动盈亏" min-width="90" align="right" sortable show-overflow-tooltip>
            <template slot-scope="scope">
              <span :style="{ color: (Number(scope.row.float_profit) || 0) >= 0 ? '#F56C6C' : '#67C23A' }">
                {{ Number(scope.row.float_profit) > 0 ? '+' : '' }}{{ $money(scope.row.float_profit) }}
              </span>
            </template>
          </el-table-column>
          <el-table-column prop="risk_ratio" label="风险率" min-width="90" align="right" sortable show-overflow-tooltip>
            <template slot-scope="scope">
              <el-tag :type="getRiskTagType(scope.row.risk_ratio)" size="small">
                {{ $pct(scope.row.risk_ratio, 1) }}
              </el-tag>
            </template>
          </el-table-column>
          <el-table-column prop="position_count" label="持仓品种数" min-width="95" align="center" sortable></el-table-column>
          <el-table-column label="操作" width="150" fixed="right">
            <template slot-scope="scope">
              <el-button
                size="mini"
                type="text"
                @click="viewAccountDetail(scope.row)"
              >
                详情
              </el-button>
              <el-button
                size="mini"
                type="text"
                style="color: #F56C6C"
                v-if="scope.row.risk_ratio >= 0.9"
                @click="forceLiquidate(scope.row)"
              >
                强平
              </el-button>
            </template>
          </el-table-column>
        </el-table>
      </el-tab-pane>

      <!-- 强平记录 -->
      <el-tab-pane label="强平记录" name="liquidations">
        <div class="table-toolbar">
          <el-date-picker
            v-model="dateRange"
            type="daterange"
            range-separator="至"
            start-placeholder="开始日期"
            end-placeholder="结束日期"
            value-format="yyyy-MM-dd"
            @change="loadLiquidations"
          ></el-date-picker>
        </div>

        <!-- ✨ 表格高度改为跟随视口, 不再写死 500px @yutiansut @quantaxis
             同上(另一 tab 内的表格, 页面 chrome 相同)
             var(--qa-content-h) 由 layout/index.vue 统一定义 = 100vh - 56(顶栏) - 40(padding),
             有公告条时自动再减 40px; max(260px, ...) 是极短视口下的兜底 -->
        <el-table
          :empty-text="$emptyText(liquidationError, '暂无强平记录')"
          ref="liquidationTable"
          :data="liquidations"
          border
          stripe
          v-loading="liquidationLoading"
          :height="'max(260px, calc(var(--qa-content-h, calc(100vh - 96px)) - 365px))'"
        >
          <!-- ⚠️ 列名必须对齐后端 `LiquidationRecord`(risk/risk_monitor.rs) ——
               handler `management.rs::get_liquidation_records` **直接序列化该结构体**,
               无映射层。原来这里 10 列中有 8 列读的字段后端根本不返回:
                 user_name / liquidation_price / liquidation_volume / trigger_type / operator  → 后端完全没有
                 pre_balance → 实为 balance_before;loss_amount → 实为 total_loss
                 instrument_id → 实为 instruments_closed(**是 Vec<String>**)
               只有 liquidation_time 与 user_id 是对的。因为生产至今零条强平记录,
               这个错误一直没显形 —— 一旦真发生强平,整张表除时间和用户ID外全是空白。
               @yutiansut @quantaxis -->
          <el-table-column prop="liquidation_time" label="强平时间" min-width="180"></el-table-column>
          <el-table-column prop="user_id" label="用户ID" min-width="150" show-overflow-tooltip></el-table-column>
          <el-table-column prop="risk_ratio_before" label="强平前风险度" min-width="120" align="right" show-overflow-tooltip>
            <template slot-scope="scope">
              {{ Number.isFinite(Number(scope.row.risk_ratio_before))
                 ? (Number(scope.row.risk_ratio_before) * 100).toFixed(2) + '%' : '--' }}
            </template>
          </el-table-column>
          <el-table-column prop="balance_before" label="强平前权益" min-width="130" align="right" show-overflow-tooltip>
            <template slot-scope="scope">
              {{ fmtMoney(scope.row.balance_before) }}
            </template>
          </el-table-column>
          <el-table-column prop="balance_after" label="强平后权益" min-width="130" align="right" show-overflow-tooltip>
            <template slot-scope="scope">
              {{ fmtMoney(scope.row.balance_after) }}
            </template>
          </el-table-column>
          <el-table-column prop="total_loss" label="平仓损失" min-width="130" align="right" show-overflow-tooltip>
            <template slot-scope="scope">
              <span style="color: #F56C6C">
                {{ fmtMoney(scope.row.total_loss) }}
              </span>
            </template>
          </el-table-column>
          <el-table-column prop="instruments_closed" label="平仓合约" min-width="180" show-overflow-tooltip>
            <template slot-scope="scope">
              {{ Array.isArray(scope.row.instruments_closed) ? scope.row.instruments_closed.join(', ') : '--' }}
            </template>
          </el-table-column>
          <el-table-column prop="remark" label="备注" min-width="160" show-overflow-tooltip>
            <template slot-scope="scope">{{ scope.row.remark || '--' }}</template>
          </el-table-column>
        </el-table>
      </el-tab-pane>
    </el-tabs>

    <!-- 账户详情对话框 -->
    <el-dialog
      title="账户详情"
      :visible.sync="detailDialogVisible"
      width="900px"
    >
      <div v-if="currentAccount" v-loading="detailLoading">
        <el-descriptions :column="2" border class="detail-summary">
          <el-descriptions-item label="账户ID">{{ currentAccount.user_id }}</el-descriptions-item>
          <!-- ⚠️ 不能用 `详情.x || 列表.x` @yutiansut @quantaxis
               `0 || 旧值` 得到的是**旧值** —— 而爆仓账户的 balance/available/margin
               恰恰就是 0。实测:详情接口返回 `balance:0, available:0, margin:0`,
               页面却渲染出列表里的陈旧值「总权益 1,000,000 / 可用 555,555」。
               **一个被打穿的账户在风控详情页显示一百万权益。**
               用 `detailNum()` 显式判 null/undefined,只有「详情真的没这个字段」
               才回落到列表值。 -->
          <el-descriptions-item label="总权益">{{ formatNumber(detailNum('balance', 'balance')) }}</el-descriptions-item>
          <el-descriptions-item label="可用资金">{{ formatNumber(detailNum('available', 'available')) }}</el-descriptions-item>
          <el-descriptions-item label="占用保证金">{{ formatNumber(detailNum('margin', 'margin_used')) }}</el-descriptions-item>
          <el-descriptions-item label="浮动盈亏">
            <span :style="{ color: detailNum('float_profit', 'float_profit') >= 0 ? '#F56C6C' : '#67C23A' }">
              {{ formatNumber(detailNum('float_profit', 'float_profit')) }}
            </span>
          </el-descriptions-item>
          <el-descriptions-item label="风险率">
            <el-tag :type="getRiskTagType(currentAccount.risk_ratio)">
              {{ $pct(currentAccount.risk_ratio, 1) }}
            </el-tag>
          </el-descriptions-item>
        </el-descriptions>

        <el-tabs v-model="detailTab">
          <el-tab-pane label="持仓明细" name="positions">
            <el-table
              :data="accountDetail.positions"
              border
              stripe
              size="mini"
              :empty-text="$emptyText(detailError, '暂无持仓')"
            >
              <el-table-column prop="instrument_id" label="合约" min-width="140" show-overflow-tooltip/>
              <el-table-column prop="volume_long" label="多头" min-width="100" align="right" show-overflow-tooltip/>
              <el-table-column prop="volume_short" label="空头" min-width="100" align="right" show-overflow-tooltip/>
              <!-- QIFI 里均价是 open_price_long;open_cost_long 是**总成本**(差 手数×乘数 倍) -->
              <el-table-column prop="open_price_long" label="多头均价" min-width="120" align="right" show-overflow-tooltip>
                <template slot-scope="{ row }">{{ $num(row.open_price_long, 2) }}</template>
              </el-table-column>
              <el-table-column prop="open_price_short" label="空头均价" min-width="120" align="right" show-overflow-tooltip>
                <template slot-scope="{ row }">{{ $num(row.open_price_short, 2) }}</template>
              </el-table-column>
              <el-table-column prop="float_profit" label="浮动盈亏" min-width="140" align="right" show-overflow-tooltip>
                <template slot-scope="{ row }">
                  <span :style="{ color: (Number(row.float_profit) || 0) >= 0 ? '#F56C6C' : '#67C23A' }">
                    {{ formatNumber(Number(row.float_profit) || 0) }}
                  </span>
                </template>
              </el-table-column>
            </el-table>
          </el-tab-pane>
          <el-tab-pane label="订单明细" name="orders">
            <!-- height 是必须的:没有它 el-table 会把全部行渲染进 DOM,
                 实测 25,611 行 → 表格高 922,034 px / 38 万个节点。@yutiansut @quantaxis -->
            <el-table
              :data="accountDetail.orders"
              height="360"
              border
              stripe
              size="mini"
              :empty-text="$emptyText(detailError, '暂无订单')"
            >
              <el-table-column prop="order_id" label="订单号" min-width="160" show-overflow-tooltip/>
              <el-table-column prop="instrument_id" label="合约" min-width="120" show-overflow-tooltip/>
              <el-table-column prop="direction" label="方向" width="80" align="center">
                <template slot-scope="{ row }">{{ enumText(row.direction) }}</template>
              </el-table-column>
              <el-table-column prop="offset" label="开平" width="80" align="center">
                <template slot-scope="{ row }">{{ enumText(row.offset) }}</template>
              </el-table-column>
              <!-- QIFI 订单字段是 limit_price / volume_orign,没有 price / volume -->
              <el-table-column prop="limit_price" label="价格" min-width="100" align="right" show-overflow-tooltip>
                <template slot-scope="{ row }">{{ $num(row.limit_price, 2) }}</template>
              </el-table-column>
              <el-table-column prop="volume_orign" label="数量" width="80" align="right" show-overflow-tooltip/>
              <el-table-column prop="status" label="状态" min-width="120">
                <template slot-scope="{ row }">{{ enumText(row.status) }}</template>
              </el-table-column>
            </el-table>
            <el-pagination
              v-if="ordersTotal > orderPageSize"
              style="margin-top: 10px; text-align: right;"
              background
              layout="total, sizes, prev, pager, next"
              :current-page="orderPage"
              :page-size="orderPageSize"
              :page-sizes="[50, 100, 200, 500]"
              :total="ordersTotal"
              @current-change="onOrderPageChange"
              @size-change="onOrderPageSizeChange"
            />
          </el-tab-pane>
        </el-tabs>
      </div>
    </el-dialog>
  </div>
</template>

<script>
import {
  getRiskAccounts,
  getMarginSummary,
  getLiquidationRecords,
  getAccountDetail,
  forceLiquidateAccount
} from '@/api'

export default {
  name: 'RiskMonitoring',
  data() {
    return {
      // 三个区各自的加载失败原因 —— 页面有 loading / liquidationLoading /
      // detailLoading 三个独立加载标志,错误态也必须分开:否则打开账户详情
      // 会擦掉主列表的错误态,详情失败又会反过来污染主列表。@yutiansut @quantaxis
      accountsError: null,
      liquidationError: null,
      detailError: null,
      loading: false,
      liquidationLoading: false,
      autoRefresh: false,
      refreshTimer: null,
      activeTab: 'realtime',
      // ✨ 详情弹窗的 tab —— 模板 :239 用了 v-model="detailTab" 却从没声明,
      //    undefined 时 el-tabs 回退到索引 "0",而两个 pane 用的是具名
      //    name="positions"/"orders",永远匹配不上 → 两个 tab 都不高亮、内容全空白。
      //    @yutiansut @quantaxis
      detailTab: 'positions',
      searchKeyword: '',
      dateRange: [],
      accounts: [],
      liquidations: [],
      statistics: {
        highRiskCount: 0,
        criticalRiskCount: 0,
        todayLiquidations: 0,
        averageRiskRatio: 0
      },
      detailDialogVisible: false,
      detailLoading: false,
      // 订单明细分页 —— 后端 /management/account/{id}/detail 已支持 page/page_size
      orderPage: 1,
      orderPageSize: 50,
      ordersTotal: 0,
      // 翻页锚点 —— 不带回会看到重复订单(实测重叠数 == 期间新增单数)
      snapshotTs: null,
      currentAccount: null,
      accountDetail: {
        info: null,
        positions: [],
        orders: []
      }
    }
  },
  computed: {
    filteredAccounts() {
      if (!this.searchKeyword) return this.accounts

      return this.accounts.filter(account =>
        account.user_id.toLowerCase().includes(this.searchKeyword.toLowerCase())
      )
    }
  },
  mounted() {
    this.loadData()
  },
  beforeDestroy() {
    this.stopAutoRefresh()
  },
  methods: {
    // ✨ QIFI 枚举本地化(方向/开平/状态),复用 account-detail.vue:202 的同一张表。
    // ⚠️ FINISHED 在 QIFI 是「已完成」统称,不区分成交/撤单/拒绝,别擅自译成「已成交」。@yutiansut @quantaxis
    enumText(v) {
      const map = {
        BUY: '买入', SELL: '卖出',
        OPEN: '开仓', CLOSE: '平仓',
        CLOSE_TODAY: '平今', CLOSE_YESTERDAY: '平昨',
        CLOSETODAY: '平今', CLOSEYESTERDAY: '平昨',
        ALIVE: '未完成', SUBMITTED: '已报', FINISHED: '已完成',
        Pending: '待成交', PartiallyFilled: '部分成交', Filled: '已成交',
        Cancelled: '已撤销', Rejected: '已拒绝'
      }
      if (v == null || v === '') return ''
      return map[v] || String(v)
    },
    // 详情优先、列表兜底,但**只在详情确实缺该字段时**才回落 @yutiansut @quantaxis
    // 用 `== null` 同时覆盖 null 与 undefined;`0` / `''` / `false` 都算「详情有值」。
    detailNum (detailKey, listKey) {
      const d = this.accountDetail && this.accountDetail.info
      if (d && d[detailKey] != null) return Number(d[detailKey]) || 0
      const l = this.currentAccount
      if (l && l[listKey] != null) return Number(l[listKey]) || 0
      return 0
    },
    // 加载数据
    async loadData() {
      await Promise.all([
        this.loadAccounts(),
        this.loadStatistics(),
        this.loadLiquidations()
      ])
    },

    // 加载账户风险数据
    async loadAccounts() {
      this.loading = true; this.accountsError = null
      try {
        const params = {}
        if (this.searchKeyword) {
          params.user_id = this.searchKeyword
        }

        const response = await getRiskAccounts(params)
        const rows = (response || []).map(item => this.normalizeRiskAccount(item))
        this.accounts = rows
      } catch (error) {
        this.accountsError = this.$errMsg(error, '加载失败')
        this.$message.error('加载账户数据失败')
        console.error(error)
      } finally {
        this.loading = false
      }
    },

    // 加载统计数据
    async loadStatistics() {
      try {
        // request 拦截器已解包 { success, data, error }，直接拿到汇总对象；
        // 请求失败会 reject 到下面的 catch，那里有同样的兜底计算
        const data = await getMarginSummary()
        this.statistics = {
          highRiskCount: data.high_risk_count || 0,
          criticalRiskCount: data.critical_risk_count || 0,
          todayLiquidations: data.liquidation_count || 0,
          averageRiskRatio: data.average_risk_ratio || 0
        }
      } catch (error) {
        // ⚠️ **不要**在这里用 this.accounts 自己重算 @yutiansut @quantaxis
        //
        // 原实现 catch 后遍历 this.accounts 重算高风险/临近爆仓数,但
        // loadAccounts() 与 loadStatistics() 是 Promise.all **并发**的,
        // catch 执行时 this.accounts 往往还是 [] → 统计卡静默显示 **0**,
        // 而下方表格仍显示「共 55 个账户」、右上角仍是绿色「系统正常」。
        // 实测:只让 margin-summary 返回 500,高风险账户 18 → **0**
        // (后端权威值当时是 high_risk_count:19)。
        //
        // 即便不并发,客户端重算权威量本身就是错的(lessons.md L45)。
        // 置 null 让卡片显示「统计不可用」,比假报 0 诚实。
        console.error('加载统计数据失败', error)
        this.statistics = null
        this.$message.error('风控统计加载失败,数据不可用')
      }
    },

    // 加载强平记录
    async loadLiquidations() {
      this.liquidationLoading = true; this.liquidationError = null
      try {
        const params = {}
        if (this.dateRange && this.dateRange.length === 2) {
          params.start_date = this.dateRange[0]
          params.end_date = this.dateRange[1]
        }

        // request 拦截器已解包，直接拿到强平记录数组
        const records = await getLiquidationRecords(params)
        this.liquidations = records || []
      } catch (error) {
        this.liquidationError = this.$errMsg(error, '加载失败')
        this.$message.error('加载强平记录失败')
        console.error(error)
      } finally {
        this.liquidationLoading = false
      }
    },

    // ⚠️ **映射层不能把空值抹成 0** —— 否则显示层的守卫永远看不到它。
    // @yutiansut @quantaxis
    //
    // 原实现是 `Number(item.balance) || 0`。`Number(null) === 0`、`Number('') === 0`,
    // 于是「后端没给这个字段」在这里就变成了「这个账户是 0」,
    // 再往下 `$money()` 拿到的已经是一个货真价实的 0,**它的 `--` 分支永远不触发**。
    // 子代理实测:注入 `null` / `''` 到本页,总权益仍渲染 `0.00` 而不是 `--`
    // —— 显示层替换在这一页**等于没做**。
    //
    // 保持 `null`(而不是 0),让 `$money`/`$pct` 去判定;
    // `position_count` 是计数,0 是合理默认,维持原样。
    normalizeRiskAccount(item) {
      const num = v => {
        if (v === null || v === undefined || v === '') return null
        const n = Number(v)
        return Number.isFinite(n) ? n : null
      }
      return {
        user_id: item.user_id,
        balance: num(item.balance),
        available: num(item.available),
        margin_used: num(item.margin_used),
        float_profit: num(item.unrealized_pnl),
        risk_ratio: num(item.risk_ratio),
        position_count: Number(item.position_count) || 0,
        risk_level: item.risk_level || 'low'
      }
    },

    // 获取风险标签颜色
    getRiskTagType(ratio) {
      if (ratio >= 0.9) return 'danger'
      if (ratio >= 0.8) return 'warning'
      if (ratio >= 0.6) return 'info'
      return 'success'
    },

    // 查看账户详情
    async viewAccountDetail(account) {
      this.currentAccount = account
      this.detailDialogVisible = true
      this.orderPage = 1
      this.snapshotTs = null
      await this.loadAccountDetail()
    },

    onOrderPageChange(page) {
      this.orderPage = page
      this.loadAccountDetail()
    },

    onOrderPageSizeChange(size) {
      this.orderPageSize = size
      this.orderPage = 1
      this.snapshotTs = null
      this.loadAccountDetail()
    },

    async loadAccountDetail() {
      const account = this.currentAccount
      if (!account) return
      this.detailLoading = true; this.detailError = null
      try {
        const params = { page: this.orderPage, page_size: this.orderPageSize }
        if (this.snapshotTs !== null) params.snapshot_ts = this.snapshotTs
        const detail = await getAccountDetail(account.user_id, params)
        if (this.snapshotTs === null && detail.snapshot_ts !== undefined) {
          this.snapshotTs = detail.snapshot_ts
        }
        this.ordersTotal = detail.orders_total || (detail.orders || []).length
        this.accountDetail = {
          info: detail.account_info || account,
          positions: detail.positions || [],
          orders: detail.orders || []
        }
      } catch (error) {
        this.detailError = this.$errMsg(error, '加载失败')
        this.$message.error('加载账户详情失败')
        console.error(error)
        this.accountDetail = {
          info: account,
          positions: [],
          orders: []
        }
      } finally {
        this.detailLoading = false
      }
    },

    // 强平操作
    async forceLiquidate(account) {
      try {
        await this.$confirm(
          `确定要强制平仓账户 ${account.user_id} 吗？当前风险率：${((Number(account.risk_ratio) || 0) * 100).toFixed(1)}%`,
          '强制平仓确认',
          {
            type: 'warning',
            confirmButtonText: '确定强平',
            cancelButtonText: '取消'
          }
        )

        await forceLiquidateAccount({
          account_id: account.user_id,
          reason: 'Manual force liquidation from admin UI'
        })
        this.$message.success('强平任务已提交')
        this.loadData()
      } catch (error) {
        if (error !== 'cancel') {
          const msg = (error.response && error.response.data && error.response.data.error && error.response.data.error.message) || error.message
          this.$message.error('强平失败：' + msg)
        }
      }
    },

      // ✨ 委托全局 `$money`(main.js):判空 → isFinite → **量级上限 1e15**。
      // 全仓曾有 13 个各写各的金额格式化器,**没有一个带量级闸**,
      // 于是 `balance = 1e300` 的污染账户在 6 个页面渲染成 404 个字符
      // (`Number.isFinite(1e300)` 为真,只判 isFinite 挡不住)。@yutiansut @quantaxis
    formatNumber(val) {
      return this.$money(val)
    },

    // 切换自动刷新
    toggleAutoRefresh() {
      if (this.autoRefresh) {
        this.startAutoRefresh()
      } else {
        this.stopAutoRefresh()
      }
    },

    // 开始自动刷新
    startAutoRefresh() {
      this.stopAutoRefresh()
      this.refreshTimer = setInterval(() => {
        this.loadData()
      }, 10000) // 每10秒刷新一次
    },

    // 停止自动刷新
    stopAutoRefresh() {
      if (this.refreshTimer) {
        clearInterval(this.refreshTimer)
        this.refreshTimer = null
      }
    }
  }
}
</script>

<style lang="scss" scoped>
// @yutiansut @quantaxis - 深色主题样式
$dark-bg-primary: #0d1117;
$dark-bg-secondary: #161b22;
$dark-bg-card: #1c2128;
$dark-bg-tertiary: #21262d;
$dark-border: #30363d;
$dark-text-primary: #f0f6fc;
$dark-text-secondary: #8b949e;
$dark-text-muted: #6e7681;
$primary-color: #1890ff;
$danger-color: #f5222d;
$warning-color: #faad14;
$success-color: #52c41a;

.risk-container {
  padding: 20px;
  background: $dark-bg-primary;
  // ⚠️ 用 --qa-content-h,不要手写 calc(100vh - N)。@yutiansut @quantaxis
  //    原值 -60px 少算了 content-wrapper 的上下 padding(各 20px):
  //    真实 chrome = top-header 56 + padding 40 = 96px(有公告条时再 +40)。
  //    实测本页在无内容可滚时仍有 41px 的死滚动条
  //    (.content-wrapper 的 scrollHeight - clientHeight = 41)。
  //    `layout/index.vue:546` 早就写明「各页面请一律用 var(--qa-content-h)」。
  min-height: var(--qa-content-h, calc(100vh - 96px));
}

.page-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 20px;
}

.page-header h2 {
  margin: 0;
  color: $dark-text-primary !important;
}

.header-actions {
  display: flex;
  align-items: center;
}

.header-actions > * {
  margin-left: 15px;
}

.header-actions > *:first-child {
  margin-left: 0;
}

// 开关样式
::v-deep .el-switch__label {
  color: $dark-text-secondary !important;
}

.stats-row {
  margin-bottom: 20px;
}

.stat-card {
  display: flex;
  align-items: center;
  padding: 20px;
  background: $dark-bg-card !important;
  border: 1px solid $dark-border;
  border-radius: 8px;
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.3);
}

.stat-icon {
  font-size: 40px;
  margin-right: 15px;
}

.stat-card.danger .stat-icon {
  color: $danger-color;
}

.stat-card.critical .stat-icon {
  color: $warning-color;
}

.stat-card.warning .stat-icon {
  color: $warning-color;
}

.stat-card.info .stat-icon {
  color: $primary-color;
}

.stat-value {
  font-size: 28px;
  font-weight: bold;
  color: $dark-text-primary !important;
  font-family: 'JetBrains Mono', monospace;
}

.stat-label {
  font-size: 14px;
  color: $dark-text-secondary !important;
  margin-top: 5px;
}

.tabs-container {
  background: $dark-bg-card !important;
  border: 1px solid $dark-border;
  border-radius: 8px;
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.3);
  padding: 20px;

  // 标签页样式
  ::v-deep .el-tabs__item {
    color: $dark-text-secondary !important;

    &.is-active {
      color: $primary-color !important;
    }

    &:hover {
      color: $primary-color !important;
    }
  }

  ::v-deep .el-tabs__nav-wrap::after {
    background-color: $dark-border !important;
  }

  // 表格样式
  ::v-deep .el-table {
    background: transparent !important;
    color: $dark-text-primary !important;

    &::before {
      background-color: $dark-border !important;
    }

    th.el-table__cell {
      background: $dark-bg-secondary !important;
      color: $dark-text-secondary !important;
      border-bottom: 1px solid $dark-border !important;
      font-weight: 600;
    }

    tr {
      background: $dark-bg-card !important;
    }

    td.el-table__cell {
      background: $dark-bg-card !important;
      color: $dark-text-primary !important;
      border-bottom: 1px solid $dark-border !important;
    }

    .el-table__row:hover > td.el-table__cell {
      background: $dark-bg-tertiary !important;
    }

    .el-table__row--striped .el-table__cell {
      background: rgba($dark-bg-tertiary, 0.5) !important;
    }

    .el-table__fixed,
    .el-table__fixed-right {
      background: $dark-bg-card !important;

      &::before {
        background-color: $dark-border !important;
      }
    }
  }

  ::v-deep .el-button--text {
    color: $primary-color !important;

    &:hover {
      color: lighten($primary-color, 15%) !important;
    }
  }
}

.table-toolbar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 15px;

  ::v-deep .el-input__inner {
    background: $dark-bg-tertiary !important;
    border-color: $dark-border !important;
    color: $dark-text-primary !important;

    &::placeholder {
      color: $dark-text-muted !important;
    }
  }
}

.toolbar-right {
  display: flex;
  align-items: center;
  color: $dark-text-secondary !important;
}

.toolbar-right > * {
  margin-left: 10px;
}

.toolbar-right > *:first-child {
  margin-left: 0;
}

// 标签样式
::v-deep .el-tag {
  border: none !important;

  &.el-tag--success {
    background: rgba($success-color, 0.15) !important;
    color: $success-color !important;
  }

  &.el-tag--warning {
    background: rgba($warning-color, 0.15) !important;
    color: $warning-color !important;
  }

  &.el-tag--danger {
    background: rgba($danger-color, 0.15) !important;
    color: $danger-color !important;
  }

  &.el-tag--info {
    background: rgba($primary-color, 0.15) !important;
    color: $primary-color !important;
  }
}

// 对话框样式
::v-deep .el-dialog {
  background: $dark-bg-card !important;
  border: 1px solid $dark-border !important;
  border-radius: 8px !important;

  .el-dialog__header {
    background: $dark-bg-secondary !important;
    border-bottom: 1px solid $dark-border !important;

    .el-dialog__title {
      color: $dark-text-primary !important;
    }
  }

  .el-dialog__body {
    background: $dark-bg-card !important;
  }
}

// 描述列表样式
::v-deep .el-descriptions {
  .el-descriptions-item__label {
    background: $dark-bg-secondary !important;
    color: $dark-text-secondary !important;
  }

  .el-descriptions-item__content {
    background: $dark-bg-card !important;
    color: $dark-text-primary !important;
  }

  .el-descriptions__body {
    background: $dark-bg-card !important;
  }

  &.is-bordered .el-descriptions-item__cell {
    border-color: $dark-border !important;
  }
}

// 日期选择器
::v-deep .el-date-editor {
  .el-input__inner {
    background: $dark-bg-tertiary !important;
    border-color: $dark-border !important;
    color: $dark-text-primary !important;
  }
}

// 按钮样式
::v-deep .el-button {
  &.el-button--default {
    background: $dark-bg-tertiary !important;
    border-color: $dark-border !important;
    color: $dark-text-primary !important;

    &:hover {
      border-color: $primary-color !important;
      color: $primary-color !important;
    }
  }
}
</style>
