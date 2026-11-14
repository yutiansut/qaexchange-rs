<template>
  <!-- ⚠️ 本页数据来自 /api/management/account/{id}/detail,
     它返回的是 **QIFI 视图**(qifi.orders / qifi.positions),
     字段名与交易所视图不同 —— 绑错了整列空白:
       cost_long   → open_price_long     cost_short → open_price_short
       ⚠️ 均价一定是 open_price_*。上一轮改成 open_cost_* 只对了一半 ——
          open_cost_long 是**开仓总成本**(实测 129785004),
          open_price_long 才是均价(3277.4),两者差「手数 × 乘数」= 39600 倍。
          列不再空白了,但数字变成误导性的天文数字,比空白更危险。
       price       → limit_price         volume     → volume_orign
     另注:QIFI 的 order_id 是 qars 内部 UUID,与订单管理页显示的
     交易所订单号(O178846...)是两套编号,同一笔单对不上。
     @yutiansut @quantaxis -->

  <div class="account-detail" v-loading="loading">
    <div class="page-header">
      <el-button icon="el-icon-arrow-left" @click="$router.back()">返回</el-button>
      <h2>账户详情 - {{ accountId }}</h2>
    </div>

    <!-- ✨ 加载失败时必须**有话说**。@yutiansut @quantaxis
         原实现里 `loadError` 在 `:216` 算了出来,但它唯一的渲染出口
         (下面两张表的 `:empty-text="$emptyText(loadError)"`)都在 `v-if="detail"`
         里面 —— 而请求失败时 `detail` 保持 null,那两块**根本不渲染**,
         错误文案算出来就被丢掉了。
         实测(2026-09-06,真实浏览器):打开 `/account/NOSUCH123`,
         页头渲染成「账户详情 - NOSUCH123」,下面 0 个表格、0 提示、
         0 alert、0 骨架屏、0 pageError —— 一个以该 ID 命名的详情页,
         却一句解释也没有。后端其实返回了 `Account not found: NOSUCH123`,
         只是前端接不住。
         注意条件是 `!loading` —— 否则首屏加载中会先闪一下错误块。 -->
    <el-alert
      v-if="!detail && !loading"
      :title="$emptyText(loadError, '未找到该账户')"
      type="warning" show-icon :closable="false" class="info-card">
      <template slot="default">
        账户 ID：{{ accountId }}
      </template>
    </el-alert>

    <el-card shadow="hover" class="info-card" v-if="detail">
      <el-descriptions :column="2" border>
        <el-descriptions-item label="账户ID">{{ accountId }}</el-descriptions-item>
        <el-descriptions-item label="资金">{{ formatCurrency(detail.account_info.balance) }}</el-descriptions-item>
        <el-descriptions-item label="可用资金">{{ formatCurrency(detail.account_info.available) }}</el-descriptions-item>
        <el-descriptions-item label="占用保证金">{{ formatCurrency(detail.account_info.margin) }}</el-descriptions-item>
        <el-descriptions-item label="浮动盈亏">
          <span :class="detail.account_info.position_profit >= 0 ? 'profit' : 'loss'">
            {{ formatCurrency(detail.account_info.position_profit) }}
          </span>
        </el-descriptions-item>
        <el-descriptions-item label="风险率">
          {{ $pct(detail.account_info.risk_ratio, 2) }}
        </el-descriptions-item>
      </el-descriptions>
    </el-card>

    <el-row :gutter="20" v-if="detail">
      <el-col :xs="24" :lg="12">
        <el-card shadow="hover">
          <div slot="header">持仓</div>
          <el-table
            :empty-text="$emptyText(loadError)" :data="detail.positions" height="300" size="mini" border>
            <el-table-column prop="instrument_id" label="合约" min-width="120" show-overflow-tooltip/>
            <el-table-column prop="volume_long" label="多头" width="80" align="right" show-overflow-tooltip/>
            <el-table-column prop="volume_short" label="空头" width="80" align="right" show-overflow-tooltip/>
            <!-- ⚠️ 均价是 open_price_long,不是 open_cost_long(后者是总成本)。
                 实测 open_cost_long=129785004 / open_price_long=3277.4,差 39600 倍
                 (=132手 × 300乘数)。@yutiansut @quantaxis -->
            <!-- ✨ 均价必须格式化 —— 后端给的是裸 f64,实测渲染成
                 `2730.3573122768976`(16 位有效数字),而全站其它价格一律 2 位小数。
                 既是显示错位,也会把列撑爆。@yutiansut @quantaxis -->
            <el-table-column prop="open_price_long" label="多头均价" min-width="120" align="right" show-overflow-tooltip>
              <template slot-scope="{ row }">{{ formatPrice(row.open_price_long) }}</template>
            </el-table-column>
            <el-table-column prop="open_price_short" label="空头均价" min-width="120" align="right" show-overflow-tooltip>
              <template slot-scope="{ row }">{{ formatPrice(row.open_price_short) }}</template>
            </el-table-column>
            <el-table-column prop="float_profit" label="浮动盈亏" min-width="120" align="right" show-overflow-tooltip>
              <template slot-scope="{ row }">
                <span :class="row.float_profit >= 0 ? 'profit' : 'loss'">
                  {{ formatCurrency(row.float_profit) }}
                </span>
              </template>
            </el-table-column>
          </el-table>
        </el-card>
      </el-col>

      <el-col :xs="24" :lg="12">
        <el-card shadow="hover">
          <div slot="header">订单</div>
          <el-table
            :empty-text="$emptyText(loadError)" :data="detail.orders" height="300" size="mini" border>
            <!-- 两个订单号并存,不是冗余 @yutiansut @quantaxis
                 order_id          = qars 内部 UUID(本页所属的 QIFI 视图)
                 exchange_order_id = 交易所订单号(订单管理页/DIFF WS 显示的那个)
                 只显示前者会导致本页的单在订单管理页里查不到。 -->
            <el-table-column prop="order_id" label="内部单号" min-width="150" show-overflow-tooltip/>
            <el-table-column prop="exchange_order_id" label="交易所单号" min-width="150" show-overflow-tooltip/>
            <el-table-column prop="instrument_id" label="合约" min-width="120" show-overflow-tooltip/>
            <!-- ✨ 中文列头下不能直接摆英文原始枚举。@yutiansut @quantaxis
                 本接口走 QIFI/DIFF 线上词汇(`protocol/diff/types.rs:430,436`:
                 BUY/SELL、OPEN/CLOSE/CLOSE_TODAY/CLOSE_YESTERDAY),
                 而 `/orders` 页拿到的是内部枚举(Submitted/Filled/…) ——
                 **两套词汇都要认**,认不出就原样显示,不要吞成空白。 -->
            <el-table-column prop="direction" label="方向" width="80" align="center">
              <template slot-scope="{ row }">{{ enumText(row.direction) }}</template>
            </el-table-column>
            <el-table-column prop="offset" label="开平" width="90" align="center">
              <template slot-scope="{ row }">{{ enumText(row.offset) }}</template>
            </el-table-column>
            <el-table-column prop="limit_price" label="价格" min-width="100" align="right" show-overflow-tooltip/>
            <el-table-column prop="volume_orign" label="数量" width="80" align="right" show-overflow-tooltip/>
            <!-- QIFI 只有 volume_left(剩余),交易所视图只有 filled_volume(已成)——
                 两者互补。这里由 volume_orign - volume_left 算出已成交量,
                 避免用户在两个页面看到不同口径。 -->
            <el-table-column label="已成交" width="90" align="right" show-overflow-tooltip>
              <template slot-scope="scope">
                {{ (Number(scope.row.volume_orign) || 0) - (Number(scope.row.volume_left) || 0) }}
              </template>
            </el-table-column>
            <el-table-column prop="last_msg" label="回报" min-width="110" show-overflow-tooltip />
            <el-table-column prop="status" label="状态" min-width="100">
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
        </el-card>
      </el-col>
    </el-row>
  </div>
</template>

<script>
import { getAccountDetail } from '@/api'
import { extractErrorMessage } from '@/api/request'

export default {
  name: 'AccountDetail',
  data() {
    return {
      // 加载失败原因 —— 与「真的没数据」区分开,交给 $emptyText 渲染
      // @yutiansut @quantaxis
      loadError: null,
      loading: false,
      detail: null,
      // 订单分页 —— 缺了它,大账户会拉回 69.7 MB / 16 万单,
      // 实测浏览器主线程连续阻塞 ≥75 秒。@yutiansut @quantaxis
      orderPage: 1,
      orderPageSize: 50,
      ordersTotal: 0,
      // 翻页锚点:首页由服务端给出,后续页原样带回,否则活跃账户会看到重复订单
      snapshotTs: null
    }
  },
  computed: {
    accountId() {
      return this.$route.params.accountId
    }
  },
  watch: {
    accountId() {
      this.fetchDetail()
    }
  },
  mounted() {
    this.fetchDetail()
  },
  methods: {
    // 价格统一 2 位小数;非数值原样返回(不要吞成 0,那会把「没有」说成「零」)
    formatPrice(v) {
      // ⚠️ **判空必须在 `Number()` 之前** —— `Number(null) === 0`、`Number('') === 0`,
      // 两者都是有限数,于是 `Number.isFinite` 为真、下面那条兜底分支**永远走不到**,
      // `null` 会被显示成 `0.00`。我上一版就是这么写的,子代理实测抓出来:
      //   formatPrice(null) -> "0.00"   formatPrice('') -> "0.00"
      // 「把『没有』说成『零』比不显示更糟」这句我写在注释里,守卫却被隐式转换绕过。
      // @yutiansut @quantaxis
      if (v === null || v === undefined || v === '') return '--'
      const n = Number(v)
      if (!Number.isFinite(n)) return '--'
      // `toFixed` 对 >= 1e21 返回指数字符串(实测 1e300 -> "1e+300"),
      // 量级超限一律显示 `--`,不要把坏数据渲染成看似正常的价格。
      if (Math.abs(n) >= 1e15) return '--'
      return n.toFixed(2)
    },

    // QIFI/DIFF 线上词汇 + 内部枚举,两套都认;认不出**原样显示**。
    // 注意 `FINISHED` 在 QIFI 里是「已完成」的统称,不区分成交/撤单/拒绝 ——
    // 这是协议本身的口径,不要在前端擅自译成「已成交」。@yutiansut @quantaxis
    enumText(v) {
      const map = {
        // 方向
        BUY: '买入', SELL: '卖出',
        // 开平
        OPEN: '开仓', CLOSE: '平仓',
        CLOSE_TODAY: '平今', CLOSE_YESTERDAY: '平昨',
        CLOSETODAY: '平今', CLOSEYESTERDAY: '平昨',
        // 状态 —— QIFI 线上词汇
        ALIVE: '未完成', SUBMITTED: '已报', FINISHED: '已完成',
        // 状态 —— 内部枚举(与 views/orders/index.vue:311 同一张表)
        Pending: '待成交', PartiallyFilled: '部分成交', Filled: '已成交',
        Cancelled: '已撤销', Rejected: '已拒绝'
      }
      if (v == null || v === '') return ''
      return map[v] || String(v)
    },

    async fetchDetail() {
      if (!this.accountId) return
      this.loading = true; this.loadError = null
      try {
        const params = { page: this.orderPage, page_size: this.orderPageSize }
        if (this.snapshotTs !== null) params.snapshot_ts = this.snapshotTs
        const data = await getAccountDetail(this.accountId, params)
        if (this.snapshotTs === null && data.snapshot_ts !== undefined) {
          this.snapshotTs = data.snapshot_ts
        }
        this.ordersTotal = data.orders_total || (data.orders || []).length
        this.detail = {
          account_info: data.account_info || {},
          positions: data.positions || [],
          orders: data.orders || []
        }
      } catch (error) {
        // ✨ 用 `extractErrorMessage` 而不是 axios 的 `error.message`。@yutiansut @quantaxis
        //
        // 后者对任何 HTTP 错误都只给通用串 `Request failed with status code 400`,
        // 而服务端真正说的是 `Account error: Account not found: NOSUCH123` ——
        // 信息量差得远,而且用户看到的是一句与自己无关的英文。
        // 本仓 `login.vue:243` / `register.vue:361` 早就在用这个提取器,
        // 这里只是补上同一个用法(与 privatefund 那条
        // 「期权分析失败: Request failed with status code 400」同类)。
        this.loadError = extractErrorMessage(error, '加载失败')
        this.$message.error('加载账户详情失败')
        console.error(error)
        this.detail = null
      } finally {
        this.loading = false
      }
    },
    onOrderPageChange(page) {
      this.orderPage = page
      this.fetchDetail()
    },
    onOrderPageSizeChange(size) {
      this.orderPageSize = size
      this.orderPage = 1
      this.snapshotTs = null
      this.fetchDetail()
    },
    // ✨ 委托给全局 `$money`(main.js)—— 判空 → isFinite → **量级上限 1e15**。
    // @yutiansut @quantaxis
    //
    // 本页此前是自己写的格式化器,挡不住 `balance = 1e300` 的污染账户:
    // 子代理实测它在这里渲染成 404 个字符(或 `¥1e+300`)。
    // `Number.isFinite(1e300)` 为真,所以只判 isFinite 也不够,必须判量级。
    // 全站四个各写各的格式化器现已统一到同一个判据。
    formatCurrency(value) {
      return this.$money(value)
    }
  }
}
</script>

<style scoped>
.account-detail {
  padding: 20px;
}

.page-header {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 20px;
}

.info-card {
  margin-bottom: 20px;
}

.profit {
  color: #F56C6C;
}

.loss {
  color: #67C23A;
}
</style>
