<template>
  <div class="market-overview">
    <el-row :gutter="20" style="margin-bottom: 20px;">
      <!-- 统计卡片 -->
      <el-col :xs="24" :sm="12" :lg="12" :xl="6">
        <el-card shadow="hover">
          <div class="stat-card">
            <i class="el-icon-user" style="color: #409EFF;"></i>
            <div class="stat-content">
              <div class="stat-label">总账户数</div>
              <div class="stat-value">{{ totalAccounts }}</div>
            </div>
          </div>
        </el-card>
      </el-col>
      <el-col :xs="24" :sm="12" :lg="12" :xl="6">
        <el-card shadow="hover">
          <div class="stat-card">
            <i class="el-icon-document" style="color: #E6A23C;"></i>
            <div class="stat-content">
              <!-- ⚠️ 口径:本数字来自 /api/management/orders,读的是 order_router
                   的**内存注册表** —— 重启时只恢复 SUBMITTED/ALIVE 的单
                   (order_router.rs:2342),已撤/已成交的历史单不在其中。
                   监控页的「总订单数」读 QIFI dailyorders(全部历史),
                   两者可以相差数倍,都不是错的。@yutiansut @quantaxis -->
              <div class="stat-label">
                活跃订单
                <el-tooltip
                  content="当前在册订单（含挂单与本次运行产生的单）。历史累计请看「系统监控」页"
                  placement="top"
                >
                  <i class="el-icon-question" style="color:#909399;font-size:12px;cursor:help;"></i>
                </el-tooltip>
              </div>
              <!-- 用 formatInt 而非直出:totalOrders 在全局聚合到达前是 null,
                   `{{ null }}` 渲染成**空白**而不是 `--`。@yutiansut @quantaxis -->
              <div class="stat-value">{{ formatInt(totalOrders) }}</div>
            </div>
          </div>
        </el-card>
      </el-col>
      <el-col :xs="24" :sm="12" :lg="12" :xl="6">
        <el-card shadow="hover">
          <div class="stat-card">
            <i class="el-icon-money" style="color: #67C23A;"></i>
            <div class="stat-content">
              <div class="stat-label">总资金</div>
              <div class="stat-value">{{ formatNumber(totalBalance) }}</div>
            </div>
          </div>
        </el-card>
      </el-col>
      <el-col :xs="24" :sm="12" :lg="12" :xl="6">
        <el-card shadow="hover">
          <div class="stat-card">
            <i class="el-icon-warning" style="color: #F56C6C;"></i>
            <div class="stat-content">
              <!-- 口径与其余三张卡不同:这是**当前页**统计,见 highRiskCount 上的说明 -->
              <div class="stat-label">高风险账户<span style="font-size:11px;color:#909399">(本页)</span></div>
              <div class="stat-value" style="color: #F56C6C;">{{ highRiskCount }}</div>
            </div>
          </div>
        </el-card>
      </el-col>
    </el-row>

    <!-- 主内容区域 -->
    <el-tabs v-model="activeTab" type="border-card">
      <!-- 账户列表标签页 -->
      <el-tab-pane label="账户列表" name="accounts">
        <div style="margin-bottom: 15px;">
          <el-button type="primary" size="small" icon="el-icon-refresh" @click="loadAccounts" :loading="accountsLoading">
            刷新
          </el-button>
          <el-input
            v-model="accountSearch"
            placeholder="搜索账户ID或名称"
            size="small"
            style="width: 300px; margin-left: 10px;"
            clearable
          >
            <i slot="prefix" class="el-input__icon el-icon-search"></i>
          </el-input>
        </div>

        <!-- ✨ 表格高度改为跟随视口, 不再写死 600px @yutiansut @quantaxis
             实测 1366x768: 上方 233px; 原 600px 令表格底部溢出 141px
             var(--qa-content-h) 由 layout/index.vue 统一定义 = 100vh - 56(顶栏) - 40(padding),
             有公告条时自动再减 40px; max(260px, ...) 是极短视口下的兜底 -->
        <el-table
          :empty-text="$emptyText(accountsError, '暂无账户')"
          :data="filteredAccounts"
          @sort-change="handleAccountSortChange"
          border
          stripe
          :height="'max(260px, calc(var(--qa-content-h, calc(100vh - 96px)) - 233px))'"
          v-loading="accountsLoading"
          :default-sort="{prop: 'risk_ratio', order: 'descending'}"
        >
          <el-table-column prop="user_id" label="账户ID" min-width="280" show-overflow-tooltip />
          <el-table-column prop="user_name" label="账户名称" min-width="180" />
          <el-table-column prop="balance" label="余额" min-width="140" align="right" sortable="custom" show-overflow-tooltip>
            <template slot-scope="scope">
              {{ formatNumber(scope.row.balance) }}
            </template>
          </el-table-column>
          <el-table-column prop="available" label="可用资金" min-width="140" align="right" sortable="custom" show-overflow-tooltip>
            <template slot-scope="scope">
              {{ formatNumber(scope.row.available) }}
            </template>
          </el-table-column>
          <el-table-column prop="margin_used" label="占用保证金" min-width="140" align="right" sortable="custom" show-overflow-tooltip>
            <template slot-scope="scope">
              {{ formatNumber(scope.row.margin_used) }}
            </template>
          </el-table-column>
          <!-- show-overflow-tooltip:该列实测被 ellipsis 截断且无提示,hover 也弹不出全文。
               不加宽是因为这张表在 1280 下已横向溢出,加宽会加剧;
               tooltip 让内容 hover 可达而不增加列宽。@yutiansut @quantaxis -->
          <el-table-column prop="risk_ratio" label="风险率" min-width="70" align="center" sortable="custom" show-overflow-tooltip>
            <template slot-scope="scope">
              <el-tag :type="getRiskType(scope.row.risk_ratio)" size="small">
                {{ $pct(scope.row.risk_ratio, 2) }}
              </el-tag>
            </template>
          </el-table-column>
          <el-table-column label="操作" width="200">
            <template slot-scope="scope">
              <el-button type="text" size="small" @click="viewAccountDetail(scope.row)">
                详情
              </el-button>
              <el-button type="text" size="small" @click="viewAccountOrders(scope.row)">
                订单
              </el-button>
            </template>
          </el-table-column>
        </el-table>

        <el-pagination
          v-if="accountsTotal > accountsPageSize"
          @current-change="handleAccountsPageChange"
          :current-page="accountsPage"
          :page-size="accountsPageSize"
          layout="total, prev, pager, next"
          :total="accountsTotal"
          style="margin-top: 20px; text-align: center;"
        />
      </el-tab-pane>

      <!-- 订单列表标签页 -->
      <el-tab-pane label="活跃订单" name="orders">
        <div style="margin-bottom: 15px;">
          <el-button type="primary" size="small" icon="el-icon-refresh" @click="loadAllOrders" :loading="ordersLoading">
            刷新
          </el-button>
          <el-select
            v-model="orderStatusFilter"
            placeholder="订单状态"
            size="small"
            style="width: 150px; margin-left: 10px;"
            clearable
            @change="loadAllOrders"
          >
            <el-option label="全部" value="" />
            <el-option label="已提交" value="Submitted" />
            <el-option label="部分成交" value="PartiallyFilled" />
            <el-option label="已成交" value="Filled" />
            <el-option label="已撤销" value="Cancelled" />
            <el-option label="已拒绝" value="Rejected" />
          </el-select>
          <el-input
            v-model="orderSearch"
            placeholder="搜索订单ID或合约"
            size="small"
            style="width: 300px; margin-left: 10px;"
            clearable
          >
            <i slot="prefix" class="el-input__icon el-icon-search"></i>
          </el-input>
        </div>

        <!-- ✨ 表格高度改为跟随视口, 不再写死 600px @yutiansut @quantaxis
             同上(另一 tab)
             var(--qa-content-h) 由 layout/index.vue 统一定义 = 100vh - 56(顶栏) - 40(padding),
             有公告条时自动再减 40px; max(260px, ...) 是极短视口下的兜底 -->
        <el-table
          :empty-text="$emptyText(ordersError, '暂无订单')"
          :data="filteredOrders"
          border
          stripe
          :height="'max(260px, calc(var(--qa-content-h, calc(100vh - 96px)) - 233px))'"
          v-loading="ordersLoading"
        >
          <el-table-column prop="order_id" label="订单ID" min-width="200" show-overflow-tooltip />
          <el-table-column prop="user_id" label="账户ID" min-width="200" show-overflow-tooltip>
            <template slot-scope="scope">
              {{ getAccountName(scope.row.user_id) }}
            </template>
          </el-table-column>
          <el-table-column prop="instrument_id" label="合约" min-width="100" show-overflow-tooltip/>
          <el-table-column prop="direction" label="方向" width="70" align="center">
            <template slot-scope="scope">
              <el-tag :type="scope.row.direction === 'BUY' ? 'danger' : 'success'" size="mini">
                {{ scope.row.direction === 'BUY' ? '买' : '卖' }}
              </el-tag>
            </template>
          </el-table-column>
          <el-table-column prop="offset" label="开平" width="70" align="center">
            <template slot-scope="scope">
              {{ scope.row.offset === 'OPEN' ? '开' : '平' }}
            </template>
          </el-table-column>
          <el-table-column prop="price" label="价格" min-width="100" align="right" show-overflow-tooltip>
            <template slot-scope="scope">
              {{ $num(scope.row.price, 2) }}
            </template>
          </el-table-column>
          <el-table-column prop="volume" label="数量" width="80" align="right" show-overflow-tooltip/>
          <el-table-column prop="filled_volume" label="成交" width="80" align="right" show-overflow-tooltip/>
          <el-table-column prop="status" label="状态" min-width="70" align="center">
            <template slot-scope="scope">
              <el-tag :type="getOrderStatusType(scope.row.status)" size="mini">
                {{ getOrderStatusText(scope.row.status) }}
              </el-tag>
            </template>
          </el-table-column>
          <el-table-column prop="submit_time" label="提交时间" min-width="160">
            <template slot-scope="scope">
              {{ formatTimestamp(scope.row.submit_time) }}
            </template>
          </el-table-column>
        </el-table>
        <!-- ✨ 如实告知被截断。@yutiansut @quantaxis
             本页签固定取最近 200 条(loadAll* 里写死 page_size),没有分页。
             实测:后端 total=677 而这里渲染 200 行、**无任何被截断的迹象**,
             用户会当成全部。
             为什么不直接加分页:`allOrders` 同时喂着「实时监控」页签的
             `recentOrders`(过滤活动单后取 20),改成真分页会让它只看到当前页,
             把一个现在正常的功能改坏 —— 而 dev server 的 watch 已死、
             我无法在浏览器里验证。所以先做**最小且诚实**的一步:说清楚。 -->
        <div v-if="ordersTotal > allOrders.length" class="trunc-note">
          仅显示最近 {{ allOrders.length }} 条，共 <b>{{ ordersTotal }}</b> 条 —— 列表已被截断
        </div>
      </el-tab-pane>

      <!-- 成交记录标签页 @yutiansut @quantaxis -->
      <el-tab-pane label="成交记录" name="trades">
        <div style="margin-bottom: 15px;">
          <el-button type="primary" size="small" icon="el-icon-refresh" @click="loadAllTrades" :loading="tradesLoading">
            刷新
          </el-button>
          <el-input
            v-model="tradeSearch"
            placeholder="搜索合约或账户"
            size="small"
            style="width: 300px; margin-left: 10px;"
            clearable
          >
            <i slot="prefix" class="el-input__icon el-icon-search"></i>
          </el-input>
        </div>

        <!-- ✨ 表格高度改为跟随视口, 不再写死 600px @yutiansut @quantaxis
             同上(另一 tab)
             var(--qa-content-h) 由 layout/index.vue 统一定义 = 100vh - 56(顶栏) - 40(padding),
             有公告条时自动再减 40px; max(260px, ...) 是极短视口下的兜底 -->
        <el-table
          :empty-text="$emptyText(tradesError, '暂无成交')"
          :data="filteredTrades"
          border
          stripe
          :height="'max(260px, calc(var(--qa-content-h, calc(100vh - 96px)) - 233px))'"
          v-loading="tradesLoading"
        >
          <el-table-column prop="trade_id" label="成交ID" min-width="155" show-overflow-tooltip />
          <el-table-column prop="instrument_id" label="合约" min-width="100" show-overflow-tooltip/>
          <el-table-column prop="buy_user_id" label="买方账户" min-width="120" show-overflow-tooltip>
            <template slot-scope="scope">
              {{ getAccountNameShort(scope.row.buy_user_id) }}
            </template>
          </el-table-column>
          <el-table-column prop="sell_user_id" label="卖方账户" min-width="125" show-overflow-tooltip>
            <template slot-scope="scope">
              {{ getAccountNameShort(scope.row.sell_user_id) }}
            </template>
          </el-table-column>
          <el-table-column prop="price" label="成交价" min-width="100" align="right" show-overflow-tooltip>
            <template slot-scope="scope">
              {{ $num(scope.row.price, 2) }}
            </template>
          </el-table-column>
          <el-table-column prop="volume" label="成交量" width="80" align="right" show-overflow-tooltip/>
          <el-table-column prop="trading_day" label="交易日" min-width="100" />
          <el-table-column prop="timestamp" label="成交时间" min-width="160">
            <template slot-scope="scope">
              {{ formatTimestamp(scope.row.timestamp) }}
            </template>
          </el-table-column>
        </el-table>
        <!-- 同订单页签:如实告知被截断,理由见那里的注释。@yutiansut @quantaxis -->
        <div v-if="tradesTotal > allTrades.length" class="trunc-note">
          仅显示最近 {{ allTrades.length }} 条，共 <b>{{ tradesTotal }}</b> 条 —— 列表已被截断
        </div>
      </el-tab-pane>

      <!-- 实时监控标签页 -->
      <el-tab-pane label="实时监控" name="realtime">
        <div class="realtime-monitor">
          <el-row :gutter="20">
            <el-col :xs="24" :lg="12">
              <el-card shadow="hover">
                <div slot="header">
                  <span>订单流监控</span>
                  <el-tag size="mini" style="margin-left: 10px;">
                    实时更新
                  </el-tag>
                </div>
                <div class="order-flow-chart" style="height: 400px;">
                  <div v-for="(order, index) in recentOrders" :key="index" class="order-flow-item">
                    <div class="order-time">{{ formatTime(order.submit_time) }}</div>
                    <div class="order-info">
                      <el-tag :type="order.direction === 'BUY' ? 'danger' : 'success'" size="mini">
                        {{ order.direction }}
                      </el-tag>
                      <span style="margin: 0 10px;">{{ order.instrument_id }}</span>
                      <span>{{ order.volume }}手 @ {{ order.price.toFixed(2) }}</span>
                    </div>
                    <div class="order-account">{{ getAccountNameShort(order.user_id) }}</div>
                  </div>
                </div>
              </el-card>
            </el-col>
            <el-col :xs="24" :lg="12">
              <el-card shadow="hover">
                <div slot="header">
                  <span>风险监控</span>
                </div>
                <div style="height: 400px; overflow-y: auto;">
                  <el-table
                    :empty-text="$emptyText(accountsError, '暂无高风险账户')"
                    :data="highRiskAccounts"
                    border
                    size="small"
                    max-height="380"
                  >
                    <el-table-column prop="user_name" label="账户" min-width="150" />
                    <el-table-column prop="risk_ratio" label="风险率" align="center">
                      <template slot-scope="scope">
                        <el-tag type="danger" size="mini">
                          {{ $pct(scope.row.risk_ratio, 2) }}
                        </el-tag>
                      </template>
                    </el-table-column>
                    <el-table-column prop="margin_used" label="保证金占用" align="right" show-overflow-tooltip>
                      <template slot-scope="scope">
                        {{ formatNumber(scope.row.margin_used) }}
                      </template>
                    </el-table-column>
                  </el-table>
                </div>
              </el-card>
            </el-col>
          </el-row>
        </div>
      </el-tab-pane>
    </el-tabs>
  </div>
</template>

<script>
// @yutiansut @quantaxis - 使用高效的全市场 API
import { listAllAccounts, listAllOrders, listAllTrades } from '@/api'

export default {
  name: 'MarketOverview',
  data() {
    return {
      // 三个分区(账户/订单/成交)各有独立的 loading 标志,错误态同样分开,
      // 避免一个分区失败把另外两个的表格标成「加载失败」。@yutiansut @quantaxis
      accountsError: null,
      ordersError: null,
      tradesError: null,
      activeTab: 'accounts',
      // 账户相关
      accounts: [],
      accountsLoading: false,
      // 账户表排序状态(服务端排序) @yutiansut @quantaxis
      accountSort: { prop: '', order: '' },
      accountsPage: 1,
      accountsPageSize: 20,
      accountsTotal: 0,
      // 订单/成交页签**固定取最近 200 条**(见 loadAllOrders/loadAllTrades)。
      // 记住服务端总数,好在表下如实说明「只显示了一部分」——
      // 实测该页曾渲染 200 行、而后端 total=677,**没有任何被截断的迹象**,
      // 用户会当成全部。@yutiansut @quantaxis
      ordersTotal: 0,
      tradesTotal: 0,
      pageCap: 200,
      accountSearch: '',
      // 订单相关
      allOrders: [],
      ordersLoading: false,
      orderStatusFilter: '',
      orderSearch: '',
      // 成交相关 @yutiansut @quantaxis
      allTrades: [],
      tradesLoading: false,
      tradeSearch: '',
      // 实时监控
      recentOrders: [],
      // 自动刷新
      refreshTimer: null
    }
  },
  computed: {
    filteredAccounts() {
      if (!this.accountSearch) {
        return this.accounts
      }
      const search = this.accountSearch.toLowerCase()
      return this.accounts.filter(acc =>
        acc.user_id.toLowerCase().includes(search) ||
        acc.user_name.toLowerCase().includes(search)
      )
    },
    filteredOrders() {
      let orders = this.allOrders

      if (this.orderStatusFilter) {
        orders = orders.filter(o => o.status === this.orderStatusFilter)
      }

      if (this.orderSearch) {
        const search = this.orderSearch.toLowerCase()
        orders = orders.filter(o =>
          o.order_id.toLowerCase().includes(search) ||
          o.instrument_id.toLowerCase().includes(search) ||
          o.user_id.toLowerCase().includes(search)
        )
      }

      return orders
    },
    // @yutiansut @quantaxis - 过滤成交记录
    filteredTrades() {
      if (!this.tradeSearch) {
        return this.allTrades
      }
      const search = this.tradeSearch.toLowerCase()
      return this.allTrades.filter(t =>
        t.instrument_id.toLowerCase().includes(search) ||
        t.buy_user_id.toLowerCase().includes(search) ||
        t.sell_user_id.toLowerCase().includes(search) ||
        t.trade_id.toLowerCase().includes(search)
      )
    },
    // ⚠️ 这三个头部大数字必须用**全市场**聚合,不能用 `this.accounts`。
    // @yutiansut @quantaxis
    //
    // 原来写的是 `this.accounts.length` / `.reduce(...)` / `.filter(...)`,
    // 而 `this.accounts` 只有**当前这一页**(20 条)。实测(2026-09-05):
    //   第 1 页 → 总账户数 20,总资金 4,230,598,173.15
    //   第 2 页 → 总账户数 20,总资金 7,335,911,336.77   ← 翻一页凭空多 31 亿
    // 而正下方分页条明明写着「共 78 条」—— **同屏自相矛盾**。
    // 「高风险账户」同理:第 3 页的高风险账户在首屏被计为 0。
    //
    // 全局聚合来自 store 的 `monitoring`(`/api/monitoring/system`)。
    // ⚠️ store 的自动轮询只在 dashboard 挂载期间跑,**本页不在其中** ——
    // 所以 mounted 里自己 dispatch 了一次 `fetchMonitoring`,
    // 并由本页已有的 10 秒定时器继续刷新。复用既有 action,不新增接口。
    totalAccounts() {
      const m = this.$store.state.monitoring && this.$store.state.monitoring.accounts
      // 退回分页总数(同样是全局的),再退回当前页长度
      return (m && m.total_count) || this.accountsTotal || this.accounts.length
    },
    // 活跃订单数。@yutiansut @quantaxis
    //
    // ⚠️ 必须用全局:`this.allOrders` 是 `loadAllOrders` 取的**最近 200 条**,
    // 回验实测本页显示 192,而 store 的 `monitoring.orders.pending_count` = 680,
    // **差 3.5 倍**。它夹在两个已经变成全局的数字中间,不改就会同屏打架。
    totalOrders() {
      const m = this.$store.state.monitoring && this.$store.state.monitoring.orders
      if (m && m.total_count > 0) return m.pending_count
      // ⚠️ 与 totalBalance 同样的道理:全局聚合未到达时返回 **null**,
      // **不要**退回 `allOrders` 的本地统计。@yutiansut @quantaxis
      //
      // 回验实测(2026-09-05):`loadAllOrders` 只取最近 200 条,
      // 本地统计出 **192**,而全局 `pending_count` 是 **680**。
      // 真实首屏里 192 只闪 55.5ms,但把 /monitoring/system 人为压后 3 秒时,
      // **192 会在屏幕上停留 2,705ms** —— 用户完全来得及读到并当真。
      // 而且那一刻旁边的「总资金」已经正确显示 `--` 了,两张卡自相矛盾。
      return null
    },
    totalBalance() {
      const m = this.$store.state.monitoring && this.$store.state.monitoring.accounts
      // ⚠️ 判据用 `total_count > 0` 而**不是** `total_balance != null` ——
      // store 的初始值是 `0` 不是 null(store/index.js:105),`0 != null` 成立,
      // 于是首屏在 fetchMonitoring 回来之前会显示「总资金 0」。
      // 用 total_count 判「这份聚合是否已加载」才对。@yutiansut @quantaxis
      if (m && m.total_count > 0) return m.total_balance
      // ⚠️ 拿不到全局值时返回 **null**,而不是当前页合计。@yutiansut @quantaxis
      //
      // 回验实测:首屏 318ms 时 `/management/accounts` 先返回,兜底算出
      // 4,232,972,133.15(只有真值的 **25.5%**),一直显示到 579ms
      // `/monitoring/system` 回来才纠正 —— **错误金额可见 262ms**。
      // 0 和 `--` 用户知道是「还没加载」,而一个像模像样的 42 亿会被当成真值。
      // 这与本轮删掉的假趋势徽章、绿色对勾是同一类:**编造出来的确定感**。
      return null
    },
    // ⚠️ 这个数**仍是当前页**的,与旁边三个已改成全局的数字口径不同。
    // @yutiansut @quantaxis
    //
    // 为什么没一起改:`/api/monitoring/system` 只给 accounts/orders/trades/storage
    // 四段聚合,**没有风险分档**。全局风险分档在 `/api/data/statistics/risk`
    // (返回 high/medium/low_risk_count),但那整个 `/api/data/*` 家族
    // 前端一次都没接过(见 tasks/DECISIONS-PENDING.md「1,858 行未接线」一节)——
    // 接它是独立决定,不该夹在本次修复里。
    //
    // 回验实测:今天全 78 户最大 risk_ratio 只有 0.192,页级与全局**都是 0**,
    // 界面上观察不到分歧。但换个能被现实数据触发的阈值就现形:
    //   >0.1 时 第1页(20户)=0,而全局(78户)=3 —— 3 个最高风险账户不在第 1 页。
    // 一旦有账户越过 0.8 且落在第 2 页之后,就会与旁边的「总账户数 78」
    // 构成同屏矛盾 —— 与刚修掉的「总资金」完全同类。
    //
    // 所以标签上如实标注口径,不假装它是全局值。
    highRiskCount() {
      return this.accounts.filter(acc => acc.risk_ratio > 0.8).length
    },
    highRiskAccounts() {
      return this.accounts
        .filter(acc => acc.risk_ratio > 0.7)
        .sort((a, b) => b.risk_ratio - a.risk_ratio)
        .slice(0, 10)
    }
  },
  mounted() {
    this.loadAccounts()
    this.loadAllOrders()
    this.loadAllTrades()  // @yutiansut @quantaxis - 加载全市场成交
    // 头部三个大数字要的是**全市场**聚合(见 totalAccounts 上的说明)。
    // store 的 monitoring 轮询只在 dashboard 挂载期间跑,本页不经过它,
    // 所以这里自己取一次;下面已有的 10 秒定时器会继续刷新。
    // @yutiansut @quantaxis
    this.$store.dispatch('fetchMonitoring').catch(() => {})  // 见 store/index.js 的说明:该 action 会 rethrow
    // 每10秒自动刷新
    this.refreshTimer = setInterval(() => {
      this.loadAccounts()
      this.loadAllOrders()
      this.loadAllTrades()  // @yutiansut @quantaxis
      // 头部大数字的全局聚合也要跟着刷,否则页面其余部分在动、
      // 而「总账户数/总资金/高风险」停在首屏那一刻。@yutiansut @quantaxis
      this.$store.dispatch('fetchMonitoring').catch(() => {})  // 见 store/index.js 的说明:该 action 会 rethrow
    }, 10000)
  },
  beforeDestroy() {
    if (this.refreshTimer) {
      clearInterval(this.refreshTimer)
    }
  },
  methods: {
    // 账户表列头排序 —— 回服务端重查。@yutiansut @quantaxis
    //
    // ⚠️ 本表服务端分页(:data 只有当前页),默认 `sortable` 只排当前页 20 行,
    // 顺序是错的却不声张 —— 同 admin/accounts.vue:handleSortChange 的实测:
    // 82 个账户 / 每页 20 时,排序后第一页有 15 行本不该在那里。
    // 后端排序在 management.rs 的分页**之前**做,浮点用 total_cmp(NaN 安全)。
    handleAccountSortChange ({ prop, order }) {
      this.accountSort = { prop: order ? prop : '', order: order || '' }
      this.accountsPage = 1
      this.loadAccounts()
    },

    async loadAccounts() {
      this.accountsLoading = true; this.accountsError = null
      try {
        const params = {
          page: this.accountsPage,
          page_size: this.accountsPageSize
        }
        // 排序回服务端 —— 见 handleAccountSortChange 的说明
        if (this.accountSort.prop) {
          params.sort_by = this.accountSort.prop
          params.sort_order = this.accountSort.order === 'ascending' ? 'asc' : 'desc'
        }
        const response = await listAllAccounts(params)
        this.accounts = response.accounts || []
        this.accountsTotal = response.total || 0
      } catch (error) {
        this.accountsError = this.$errMsg(error, '加载失败')
        this.$message.error('加载账户列表失败: ' + (this.$errMsg(error, '未知错误')))
      } finally {
        this.accountsLoading = false
      }
    },

    // @yutiansut @quantaxis - 使用高效的全市场订单 API (单次请求)
    async loadAllOrders() {
      this.ordersLoading = true; this.ordersError = null
      try {
        // 单次 API 调用获取所有订单（后端支持分页和过滤）
        const params = {
          page: 1,
          page_size: 200  // 获取最近200条订单
        }
        if (this.orderStatusFilter) {
          params.status = this.orderStatusFilter
        }

        const response = await listAllOrders(params)
        this.allOrders = response.orders || []
        // ✨ 记下**服务端总数**,用于如实告知被截断。@yutiansut @quantaxis
        this.ordersTotal = typeof response.total === 'number' ? response.total : this.allOrders.length

        // 更新最近订单（用于实时监控）
        this.recentOrders = this.allOrders
          .filter(o => o.status === 'Submitted' || o.status === 'PartiallyFilled' || o.status === 'PendingRoute')
          .sort((a, b) => b.submit_time - a.submit_time)
          .slice(0, 20)
      } catch (error) {
        this.ordersError = this.$errMsg(error, '加载失败')
        console.error('加载订单失败:', error)
        this.$message.error('加载订单失败: ' + (this.$errMsg(error, '未知错误')))
      } finally {
        this.ordersLoading = false
      }
    },

    // @yutiansut @quantaxis - 加载全市场成交记录
    async loadAllTrades() {
      this.tradesLoading = true; this.tradesError = null
      try {
        const params = {
          page: 1,
          page_size: 200  // 获取最近200条成交
        }

        const response = await listAllTrades(params)
        this.allTrades = response.trades || []
        this.tradesTotal = typeof response.total === 'number' ? response.total : (this.allTrades || []).length
      } catch (error) {
        this.tradesError = this.$errMsg(error, '加载失败')
        console.error('加载成交记录失败:', error)
        this.$message.error('加载成交记录失败: ' + (this.$errMsg(error, '未知错误')))
      } finally {
        this.tradesLoading = false
      }
    },

    handleAccountsPageChange(page) {
      this.accountsPage = page
      this.loadAccounts()
    },

    // 同 `getAccountNameShort`:对空值调 `.slice()` 会抛 TypeError ⇒ 整张表空白。
    // 本函数当前只接订单表的 `user_id`(非可空),这里是**预防性加固**。
    getAccountName(accountId) {
      if (accountId === null || accountId === undefined || accountId === '') return '--'
      const account = this.accounts.find(acc => acc.user_id === accountId)
      const s = String(accountId)
      return account ? `${account.user_name} (${s.slice(0, 8)}...)` : s
    },

    // ⚠️ 必须处理 null —— 否则整张表会**空白**。@yutiansut @quantaxis
    //
    // 后端 `TradeListItem` 的 buy/sell 账户字段已改成 `Option<String>`:
    // 从账户 QIFI 的 `dailytrades` 补齐的成交是**单边**的(QIFI Trade 规范里
    // 没有对手方字段),对手方那一侧就是 `null`。
    // 原实现直接 `accountId.slice(0, 12)`,对 null 抛 TypeError ——
    // 而渲染期 TypeError 会让**整张表空白、且没有任何提示**
    // (见 main.js 全局 errorHandler 上方的说明)。
    //
    // 三态渲染,与 `views/trades/index.vue` 的 `is_taker` 同一条先例:
    // 不存在 → `--`,**不编造一个没有名字的账户**。
    getAccountNameShort(accountId) {
      if (accountId === null || accountId === undefined || accountId === '') return '--'
      const account = this.accounts.find(acc => acc.user_id === accountId)
      if (account) return account.user_name
      const s = String(accountId)
      return s.length > 12 ? s.slice(0, 12) + '...' : s
    },

    getRiskType(ratio) {
      if (ratio < 0.5) return 'success'
      if (ratio < 0.7) return 'warning'
      return 'danger'
    },

    // @yutiansut @quantaxis - 后端 OrderStatus 枚举映射
    getOrderStatusType(status) {
      const map = {
        'PendingRisk': 'warning',
        'PendingRoute': 'warning',
        'Submitted': 'primary',
        'PartiallyFilled': 'info',
        'Filled': 'success',
        'Cancelled': 'info',
        'Rejected': 'danger'
      }
      return map[status] || 'info'
    },

    getOrderStatusText(status) {
      const map = {
        'PendingRisk': '风控审核中',
        'PendingRoute': '等待路由',
        'Submitted': '已提交',
        'PartiallyFilled': '部分成交',
        'Filled': '已成交',
        'Cancelled': '已撤销',
        'Rejected': '已拒绝'
      }
      return map[status] || status
    },

    // 整数型统计值的格式化 —— 与 formatNumber 同一套判空口径,
    // 只是不补两位小数(账户数/订单数是整数)。@yutiansut @quantaxis
    formatInt(value) {
      if (value === null || value === undefined || value === '') return '--'
      return Number(value).toLocaleString('zh-CN')
    },

    formatNumber(value) {
      // ⚠️ null/undefined 单独走 `--`,**不能落进 `Number(x) || 0`** ——
      // 那会把「还没加载」渲染成 `0.00`,而 0.00 看起来是个真值。
      // `totalBalance` 在全局聚合到达前正是返回 null(见其上的说明)。
      // 注意 `Number(null) === 0`,所以必须在转换**之前**判。@yutiansut @quantaxis
      if (value === null || value === undefined || value === '') return '--'
      // 原来只挡 null/undefined 且返回 '0'(邻列是 '0.00',不齐);
      // 且 `"20.5".toLocaleString(opts)` —— String 版**忽略 options**,
      // 原样吐 `20.5`(无千分位、无两位小数)。`Number(x) || 0` 三者一并解决,
      // 顺带把 -0 归一到 +0。@yutiansut @quantaxis
      // ✨ 交给全局 `$money`(main.js)—— 它在上面那条判空之外还加了
      // **量级上限 1e15**。本页此前挡不住 `balance = 1e300`:
      // `Number.isFinite(1e300)` 为真,渲染出 404 个字符把统计卡撑爆。
      // 上面那条 `null/undefined/''` 判空保留(注释解释了为什么必须在
      // `Number()` 之前),`$money` 里也有同样一条,双保险不冲突。
      // @yutiansut @quantaxis
      return this.$money(value)
    },

    formatTimestamp(timestamp) {
      if (!timestamp) return '-'
      const date = new Date(timestamp / 1000000) // 纳秒转毫秒
      return date.toLocaleString('zh-CN', {
        year: 'numeric',
        month: '2-digit',
        day: '2-digit',
        hour: '2-digit',
        minute: '2-digit',
        second: '2-digit'
      })
    },

    formatTime(timestamp) {
      if (!timestamp) return '-'
      const date = new Date(timestamp / 1000000)
      return date.toLocaleTimeString('zh-CN')
    },

    viewAccountDetail(account) {
      this.$router.push(`/account/${account.user_id}`)
    },

    viewAccountOrders(account) {
      this.activeTab = 'orders'
      this.orderSearch = account.user_id
    }
  }
}
</script>

<style lang="scss" scoped>
.trunc-note {
  margin-top: 12px;
  text-align: center;
  font-size: 12px;
  color: #909399;
}

// @yutiansut @quantaxis - 市场总览页面样式（现代化设计）
// 本文件原先只在下面**本地**定义了几个语义色,没有 $dark-* 系列。
// 用到暗色变量必须先 import,否则 sass 报 Undefined variable,
// 整个路由构建失败 → 前端会把你重定向回 /dashboard(排查时极易误判成路由问题)。
@import '@/styles/variables.scss';

$primary-color: #1890ff;
$success-color: #52c41a;
$warning-color: #faad14;
$danger-color: #f5222d;

.market-overview {
  padding: 0;

  // 统计卡片网格
  ::v-deep .el-col {
    .el-card {
      border-radius: 12px;
      border: none;
      box-shadow: 0 2px 12px rgba(0, 0, 0, 0.04);
      transition: all 0.3s ease;
      overflow: visible;

      &:hover {
        transform: translateY(-4px);
        box-shadow: 0 8px 24px rgba(0, 0, 0, 0.08);
      }

      .el-card__body {
        padding: 20px;
      }
    }
  }

  .stat-card {
    display: flex;
    align-items: center;
    gap: 16px;

    // ⚠️ 必须是 `> i`(直接子),不能是后代 `i`。
    // 后代写法会把 .stat-label 里 el-tooltip 的问号图标 <i class="el-icon-question">
    // 一并撑成 56×56 的 flex 方块,标签被顶成 3 行(实测「活跃订单」)。
    // @yutiansut @quantaxis
    > i {
      // 窄屏 56px 图标会把金额挤到装不下(实测 1366 屏卡片内容区仅 148px,
      // 而 ¥28,883,972,795.32 需要 196px)。跟视口收一点。
      width: clamp(42px, 3.2vw, 56px);
      height: clamp(42px, 3.2vw, 56px);
      border-radius: 12px;
      display: flex;
      align-items: center;
      justify-content: center;
      font-size: clamp(21px, 1.6vw, 28px);
      color: white;
      flex-shrink: 0;

      &.el-icon-user {
        background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
      }

      &.el-icon-document {
        background: linear-gradient(135deg, #f093fb 0%, #f5576c 100%);
      }

      &.el-icon-money {
        background: linear-gradient(135deg, #43e97b 0%, #38f9d7 100%);
      }

      &.el-icon-warning {
        background: linear-gradient(135deg, #fa709a 0%, #fee140 100%);
      }
    }

    .stat-content {
      flex: 1;
      // flex 子项默认 min-width:auto,不会收缩到内容宽度以下 ——
      // 56px 图标 + 28px 等宽金额撑出 358px,而卡片只有 219px。
      // 必须显式 min-width:0 才能让下面的 ellipsis 生效。@yutiansut @quantaxis
      min-width: 0;

      .stat-label {
        font-size: 13px;
        color: #909399;
        margin-bottom: 6px;
        font-weight: 500;
      }

      .stat-value {
        font-size: clamp(16px, 1.4vw, 28px);   // 见 dashboard 同名注释
        font-weight: 700;
        color: #8b949e !important;
        line-height: 1.2;
        font-family: 'JetBrains Mono', monospace;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
      }
    }
  }

  // 标签页样式
  ::v-deep .el-tabs--border-card {
    border-radius: 12px;
    border: none;
    background: $dark-bg-secondary;   // 不声明则露 Element 默认 #FFF
    box-shadow: 0 2px 12px rgba(0, 0, 0, 0.04);
    overflow: hidden;

    > .el-tabs__header {
      background: #21262d;
      border-bottom: 1px solid #30363d;

      .el-tabs__item {
        /* ⚠️ 缺 background 声明 → 露出 Element 默认白底(实测 98×48 呈
           rgb(255,255,255))。全局规则在 styles/index.scss 里已加,
           但本文件的 ::v-deep 嵌套优先级更高,必须在这里也声明。
           @yutiansut @quantaxis */
        background: transparent;
        color: #8b949e;
        height: 48px;
        line-height: 48px;
        font-weight: 500;
        transition: all 0.2s ease;

        &.is-active {
          font-weight: 600;
          color: $primary-color;
          // Element 给 border-card 的 is-active 单独指定了
          // border-left/right-color:#DCDFE6 → 页签两侧两根白线
          background-color: $dark-bg-secondary;
          border-left-color: $dark-border;
          border-right-color: $dark-border;
        }

        &:hover {
          color: $primary-color;
        }
      }
    }

    > .el-tabs__content {
      padding: 20px;
    }
  }

  // 工具栏样式
  .toolbar-row {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-bottom: 16px;
    padding: 16px;
    background: #21262d;
    border-radius: 8px;
  }

  // 表格样式增强
  ::v-deep .el-table {
    border-radius: 8px;
    overflow: hidden;

    th {
      background-color: #21262d !important;
      font-weight: 600;
      color: #8b949e !important;
    }

    .el-table__row:hover > td {
      background-color: rgba($primary-color, 0.04) !important;
    }

    // 方向标签
    .el-tag--danger {
      background-color: rgba($danger-color, 0.1);
      border-color: transparent;
      color: $danger-color;
    }

    .el-tag--success {
      background-color: rgba($success-color, 0.1);
      border-color: transparent;
      color: $success-color;
    }
  }

  // 订单流监控
  .order-flow-chart {
    overflow-y: auto;
    padding-right: 8px;

    &::-webkit-scrollbar {
      width: 6px;
    }

    &::-webkit-scrollbar-thumb {
      background: #e4e7ed;
      border-radius: 3px;
    }

    .order-flow-item {
      display: flex;
      align-items: center;
      padding: 12px 16px;
      border-radius: 8px;
      margin-bottom: 8px;
      background: #21262d;
      gap: 16px;
      transition: all 0.2s ease;

      &:hover {
        background-color: #f0f2f5;
        transform: translateX(4px);
      }

      .order-time {
        width: 70px;
        color: #909399;
        font-size: 12px;
        font-family: 'JetBrains Mono', monospace;
      }

      .order-info {
        flex: 1;
        display: flex;
        align-items: center;
        font-size: 13px;
        font-family: 'JetBrains Mono', monospace;
      }

      .order-account {
        width: 120px;
        text-align: right;
        color: #606266;
        font-size: 12px;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
      }
    }
  }

  // 实时监控区域
  .realtime-monitor {
    ::v-deep .el-card {
      border-radius: 12px;
      border: none;
      box-shadow: 0 2px 12px rgba(0, 0, 0, 0.04);

      .el-card__header {
        border-bottom: 1px solid #30363d;
        padding: 16px 20px;
        font-weight: 600;
        color: #8b949e !important;
      }
    }
  }

  // 风险标签样式
  ::v-deep .el-tag--warning {
    background-color: rgba($warning-color, 0.1);
    border-color: transparent;
    color: darken($warning-color, 10%);
  }

  ::v-deep .el-tag--primary {
    background-color: rgba($primary-color, 0.1);
    border-color: transparent;
    color: $primary-color;
  }

  ::v-deep .el-tag--info {
    background-color: rgba(#909399, 0.1);
    border-color: transparent;
    color: #909399;
  }

  // 按钮样式
  ::v-deep .el-button--primary {
    background: linear-gradient(135deg, $primary-color 0%, #096dd9 100%);
    border: none;

    &:hover {
      background: linear-gradient(135deg, #40a9ff 0%, $primary-color 100%);
    }
  }

  // 输入框样式
  ::v-deep .el-input__inner {
    border-radius: 8px;
    border-color: #e4e7ed;

    &:focus {
      border-color: $primary-color;
      box-shadow: 0 0 0 2px rgba($primary-color, 0.1);
    }
  }

  // 分页器样式
  ::v-deep .el-pagination {
    margin-top: 20px;
    text-align: center;

    .el-pager li.active {
      background-color: $primary-color;
    }
  }
}

// 响应式调整
@media (max-width: 768px) {
  .market-overview {
    .stat-card {
      .stat-value {
        font-size: 22px;
      }

      i {
        width: 48px;
        height: 48px;
        font-size: 24px;
      }
    }
  }
}
</style>
