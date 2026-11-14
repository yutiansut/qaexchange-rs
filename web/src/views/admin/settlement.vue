<template>
  <div class="settlement-container">
    <!-- 页面标题 -->
    <div class="page-header">
      <h2>结算管理</h2>
    </div>

    <!-- 标签页 -->
    <el-tabs v-model="activeTab" class="tabs-container" @tab-click="onTabClick">
      <!-- 日终结算操作 -->
      <el-tab-pane label="日终结算" name="daily">
        <el-card class="settlement-card">
          <div slot="header">
            <span>日终结算操作</span>
          </div>

          <el-form :model="settlementForm" label-width="120px">
            <el-form-item label="结算日期">
              <el-date-picker
                v-model="settlementForm.settlementDate"
                type="date"
                placeholder="选择结算日期"
                value-format="yyyy-MM-dd"
                :picker-options="datePickerOptions"
              ></el-date-picker>
            </el-form-item>

            <el-form-item label="结算价设置">
              <el-button type="primary" size="small" @click="showSetPriceDialog">
                设置结算价
              </el-button>
              <el-button type="info" size="small" @click="importPrices">
                批量导入
              </el-button>
              <span style="margin-left: 15px; color: #909399;">
                已设置 {{ settlementPrices.length }} 个合约结算价
              </span>
            </el-form-item>

            <el-form-item>
              <el-button type="danger" size="large" @click="executeSettlement" :loading="executing">
                <i class="el-icon-s-check"></i>
                执行日终结算
              </el-button>
              <span style="margin-left: 15px; color: #E6A23C;">
                <i class="el-icon-warning"></i>
                执行后将计算所有账户的盈亏并进行风险检查，请谨慎操作！
              </span>
            </el-form-item>
          </el-form>

          <!-- 结算价列表 -->
          <div v-if="settlementPrices.length > 0" style="margin-top: 20px;">
            <h4>当前结算价</h4>
            <el-table
              :data="settlementPrices"
              border
              stripe
              height="200"
            >
              <el-table-column prop="instrument_id" label="合约代码" min-width="150" show-overflow-tooltip></el-table-column>
              <el-table-column prop="settlement_price" label="结算价" min-width="120" align="right" show-overflow-tooltip></el-table-column>
              <!-- last_price / change_rate 取不到时是 null,必须显示「-」。
                   若走 `(Number(null)||0)` 会渲染成 `+0.00%` —— 那仍然是编造,
                   只是把随机数换成了 0。@yutiansut @quantaxis -->
              <el-table-column prop="last_price" label="最新价" min-width="120" align="right" show-overflow-tooltip>
                <template slot-scope="scope">
                  {{ scope.row.last_price == null ? '-' : Number(scope.row.last_price).toFixed(2) }}
                </template>
              </el-table-column>
              <el-table-column prop="change_rate" label="涨跌幅" min-width="120" align="right" show-overflow-tooltip>
                <template slot-scope="scope">
                  <span v-if="scope.row.change_rate == null">-</span>
                  <span v-else :style="{ color: (Number(scope.row.change_rate) || 0) >= 0 ? '#F56C6C' : '#67C23A' }">
                    {{ (Number(scope.row.change_rate) || 0) >= 0 ? '+' : '' }}{{ $pct(scope.row.change_rate, 2) }}
                  </span>
                </template>
              </el-table-column>
              <el-table-column label="操作" width="100">
                <template slot-scope="scope">
                  <el-button size="mini" type="text" @click="removePrice(scope.$index)">删除</el-button>
                </template>
              </el-table-column>
            </el-table>
          </div>
        </el-card>
      </el-tab-pane>

      <!-- 结算历史 -->
      <el-tab-pane label="结算历史" name="history">
        <div class="table-toolbar">
          <el-date-picker
            v-model="historyDateRange"
            type="daterange"
            range-separator="至"
            start-placeholder="开始日期"
            end-placeholder="结束日期"
            value-format="yyyy-MM-dd"
            @change="loadHistory"
          ></el-date-picker>
        </div>

        <!-- ✨ 表格高度改为跟随视口, 不再写死 500px @yutiansut @quantaxis
             推算(该 tab 未实测): 参照 risk.vue 同构布局取 300px
             var(--qa-content-h) 由 layout/index.vue 统一定义 = 100vh - 56(顶栏) - 40(padding),
             有公告条时自动再减 40px; max(260px, ...) 是极短视口下的兜底 -->
        <el-table
          :empty-text="$emptyText(loadError)"
          ref="historyTable"
          :data="historyList"
          border
          stripe
          v-loading="historyLoading"
          :height="'max(260px, calc(var(--qa-content-h, calc(100vh - 96px)) - 300px))'"
        >
          <el-table-column prop="settlement_date" label="结算日期" min-width="120"></el-table-column>
          <!-- ⚠️ 列名必须对齐后端 `SettlementResult`(exchange/settlement.rs) ——
               handler `admin.rs::get_settlement_history` **直接序列化** `Vec<SettlementResult>`,
               无映射层(末尾是 `Ok(...json(ApiResponse::success(history)))`)。
               原来 10 列里 7 列读后端不返回的字段:
                 instrument_count / profit_accounts / loss_accounts / status → 后端完全没有
                 account_count → 实为 total_accounts
                 liquidation_count → 实为 force_closed_accounts(**是 Vec<String>**)
                 execution_time → 实为 elapsed_ms
               只有 settlement_date / total_profit / total_commission 是对的。
               「合约数」整列删除:后端按**账户**维度统计,没有合约维度的数据。
               因为本系统从不自动结算(见 DECISIONS-PENDING),这张表一直是空的,错误从未显形。
               @yutiansut @quantaxis -->
          <el-table-column prop="total_accounts" label="账户数" min-width="100" align="center"></el-table-column>
          <el-table-column prop="settled_accounts" label="已结算" min-width="100" align="center"></el-table-column>
          <el-table-column prop="failed_accounts" label="失败" min-width="90" align="center">
            <template slot-scope="scope">
              <span :style="{ color: (Number(scope.row.failed_accounts) || 0) > 0 ? '#F56C6C' : '#67C23A' }">
                {{ Number(scope.row.failed_accounts) || 0 }}
              </span>
            </template>
          </el-table-column>
          <el-table-column prop="total_profit" label="总盈亏" min-width="150" align="right" show-overflow-tooltip>
            <template slot-scope="scope">
              <!-- 颜色必须与符号读**同一个**值:插值走 (Number(x)||0),这里若读生值,
                   缺字段时 `undefined >= 0` 为 false → 绿,而符号给 '+' —— 同一格子
                   符号说正、颜色说负。@yutiansut @quantaxis -->
              <span :style="{ color: (Number(scope.row.total_profit) || 0) >= 0 ? '#F56C6C' : '#67C23A' }">
                {{ (Number(scope.row.total_profit) || 0) >= 0 ? '+' : '' }}{{ $money(scope.row.total_profit) }}
              </span>
            </template>
          </el-table-column>
          <el-table-column prop="total_commission" label="总手续费" min-width="150" align="right" show-overflow-tooltip>
            <template slot-scope="scope">
              {{ $money(scope.row.total_commission) }}
            </template>
          </el-table-column>
          <el-table-column prop="force_closed_accounts" label="强平账户数" min-width="110" align="center">
            <template slot-scope="scope">
              {{ Array.isArray(scope.row.force_closed_accounts) ? scope.row.force_closed_accounts.length : '--' }}
            </template>
          </el-table-column>
          <el-table-column label="状态" min-width="90" align="center">
            <template slot-scope="scope">
              <!-- 后端 SettlementResult 没有 status 字段,由 failed_accounts 推导 -->
              <el-tag :type="(Number(scope.row.failed_accounts) || 0) === 0 ? 'success' : 'danger'" size="mini">
                {{ (Number(scope.row.failed_accounts) || 0) === 0 ? '成功' : '部分失败' }}
              </el-tag>
            </template>
          </el-table-column>
          <el-table-column prop="elapsed_ms" label="耗时" min-width="100" align="right" show-overflow-tooltip>
            <template slot-scope="scope">
              {{ Number.isFinite(Number(scope.row.elapsed_ms)) ? Number(scope.row.elapsed_ms) + ' ms' : '--' }}
            </template>
          </el-table-column>
          <el-table-column label="操作" width="100">
            <template slot-scope="scope">
              <el-button size="mini" type="text" @click="viewDetail(scope.row)">详情</el-button>
            </template>
          </el-table-column>
        </el-table>
      </el-tab-pane>

      <!-- 结算统计 -->
      <el-tab-pane label="结算统计" name="statistics">
        <el-row :gutter="20" class="stats-row">
          <el-col :xs="24" :sm="12" :lg="6">
            <div class="stat-card">
              <div class="stat-icon">
                <i class="el-icon-document-checked"></i>
              </div>
              <div class="stat-content">
                <div class="stat-value">{{ statistics.monthSettlementCount }}</div>
                <div class="stat-label">本月结算次数</div>
              </div>
            </div>
          </el-col>

          <el-col :xs="24" :sm="12" :lg="6">
            <div class="stat-card">
              <div class="stat-icon" style="color: #F56C6C">
                <i class="el-icon-user"></i>
              </div>
              <div class="stat-content">
                <div class="stat-value">{{ statistics.settledAccountsCount }}</div>
                <div class="stat-label">已结算账户数</div>
              </div>
            </div>
          </el-col>

          <el-col :xs="24" :sm="12" :lg="6">
            <div class="stat-card">
              <div class="stat-icon" style="color: #67C23A">
                <i class="el-icon-user"></i>
              </div>
              <div class="stat-content">
                <div class="stat-value">{{ statistics.failedAccountsCount }}</div>
                <div class="stat-label">结算失败账户数</div>
              </div>
            </div>
          </el-col>

          <el-col :xs="24" :sm="12" :lg="6">
            <div class="stat-card">
              <div class="stat-icon" style="color: #409EFF">
                <i class="el-icon-coin"></i>
              </div>
              <div class="stat-content">
                <div class="stat-value">{{ fmtMoney(statistics.totalCommission) }}</div>
                <div class="stat-label">总手续费收入</div>
              </div>
            </div>
          </el-col>
        </el-row>
        <el-card class="chart-wrapper" shadow="never">
          <div id="settlement-chart" style="height:360px;"></div>
        </el-card>
      </el-tab-pane>
    </el-tabs>

    <input
      type="file"
      ref="priceFileInput"
      accept=".csv,.txt"
      style="display: none;"
      @change="handlePriceFileUpload"
    />

    <!-- 设置结算价对话框 -->
    <el-dialog
      title="设置结算价"
      :visible.sync="priceDialogVisible"
      width="500px"
    >
      <el-form :model="priceForm" label-width="100px">
        <el-form-item label="合约代码">
          <el-select v-model="priceForm.instrument_id" placeholder="请选择合约" filterable>
            <el-option
              v-for="item in availableInstruments"
              :key="item.instrument_id"
              :label="item.instrument_id"
              :value="item.instrument_id"
            ></el-option>
          </el-select>
        </el-form-item>

        <el-form-item label="结算价">
          <el-input-number
            v-model="priceForm.settlement_price"
            :precision="2"
            :step="0.1"
            controls-position="right"
          ></el-input-number>
        </el-form-item>
      </el-form>

      <div slot="footer">
        <el-button @click="priceDialogVisible = false">取消</el-button>
        <el-button type="primary" @click="addSettlementPrice">确定</el-button>
      </div>
    </el-dialog>

    <!-- 结算详情对话框 -->
    <el-dialog
      title="结算详情"
      :visible.sync="detailDialogVisible"
      width="780px"
    >
      <div v-loading="detailLoading">
        <el-descriptions :column="2" border v-if="detailData">
          <el-descriptions-item label="结算日期">{{ detailData.settlement_date }}</el-descriptions-item>
          <el-descriptions-item label="账户数">{{ detailData.account_count }}</el-descriptions-item>
          <el-descriptions-item label="总盈亏">{{ formatCurrency(detailData.total_profit) }}</el-descriptions-item>
          <el-descriptions-item label="总手续费">{{ formatCurrency(detailData.total_commission) }}</el-descriptions-item>
        </el-descriptions>

        <el-tabs v-model="detailTab" style="margin-top: 16px;">
          <el-tab-pane label="账户明细" name="accounts">
            <el-table
              :empty-text="$emptyText(detailError)"
              :data="detailData && detailData.accounts || []"
              height="240"
              border
              size="mini"
            >
              <el-table-column prop="user_id" label="账户" min-width="160" show-overflow-tooltip/>
              <el-table-column prop="balance" label="结算后权益" min-width="140" align="right" show-overflow-tooltip>
                <template slot-scope="{ row }">{{ formatCurrency(row.balance) }}</template>
              </el-table-column>
              <el-table-column prop="close_profit" label="平仓盈亏" min-width="120" align="right" show-overflow-tooltip>
                <template slot-scope="{ row }">{{ formatCurrency(row.close_profit) }}</template>
              </el-table-column>
              <el-table-column prop="position_profit" label="持仓盈亏" min-width="120" align="right" show-overflow-tooltip>
                <template slot-scope="{ row }">{{ formatCurrency(row.position_profit) }}</template>
              </el-table-column>
              <el-table-column prop="commission" label="手续费" min-width="100" align="right" show-overflow-tooltip>
                <template slot-scope="{ row }">{{ formatCurrency(row.commission) }}</template>
              </el-table-column>
            </el-table>
          </el-tab-pane>
          <el-tab-pane label="结算价" name="prices">
            <el-table
              :empty-text="$emptyText(detailError)"
              :data="detailData && detailData.prices || []"
              height="240"
              border
              size="mini"
            >
              <el-table-column prop="instrument_id" label="合约" min-width="140" show-overflow-tooltip/>
              <el-table-column prop="settlement_price" label="结算价" min-width="120" align="right" show-overflow-tooltip/>
              <el-table-column prop="last_price" label="最新价" min-width="120" align="right" show-overflow-tooltip>
                <template slot-scope="{ row }">
                  <!-- 与 :68 的编辑表同口径:null/缺失显示「-」而非空单元格。
                       口径分叉只修一半等于没修。@yutiansut @quantaxis -->
                  {{ row.last_price == null ? '-' : Number(row.last_price).toFixed(2) }}
                </template>
              </el-table-column>
              <el-table-column prop="change_rate" label="涨跌幅" min-width="120" align="right" show-overflow-tooltip>
                <template slot-scope="{ row }">
                  <!-- 与 :71 的编辑表同口径:没有值显示「-」,不要渲染成 0.00%
                       —— 那仍是编造,只是把随机数换成了 0。@yutiansut @quantaxis -->
                  <span v-if="row.change_rate == null">-</span>
                  <span v-else>{{ $pct(row.change_rate, 2) }}</span>
                </template>
              </el-table-column>
            </el-table>
          </el-tab-pane>
        </el-tabs>
      </div>
    </el-dialog>
  </div>
</template>

<script>
import dayjs from 'dayjs'
import * as echarts from 'echarts'
import {
  getSettlementHistory,
  setSettlementPrice,
  batchSetSettlementPrices,
  executeSettlement,
  getSettlementDetail,
  getTick
} from '@/api'

export default {
  name: 'Settlement',
  data() {
    return {
      // 加载失败原因 —— 与「真的没数据」区分开,交给 $emptyText 渲染
      // @yutiansut @quantaxis
      loadError: null,
      // 弹窗详情表的加载失败原因 —— 与主历史表的 loadError 分开:
      // 否则打开详情会擦掉历史表的错误态,详情失败又会反过来污染历史表。
      // @yutiansut @quantaxis
      detailError: null,
      activeTab: 'daily',
      executing: false,
      historyLoading: false,
      settlementForm: {
        settlementDate: dayjs().format('YYYY-MM-DD')
      },
      settlementPrices: [],
      historyList: [],
      historyDateRange: [],
      statistics: {
        monthSettlementCount: 0,
        profitAccountsCount: 0,
        lossAccountsCount: 0,
        totalCommission: 0
      },
      priceDialogVisible: false,
      priceForm: {
        instrument_id: '',
        settlement_price: 0
      },
      availableInstruments: [
        { instrument_id: 'IF2501' },
        { instrument_id: 'IF2502' },
        { instrument_id: 'IH2501' },
        { instrument_id: 'IC2501' }
      ],
      datePickerOptions: {
        disabledDate(time) {
          // 禁止选择未来日期
          return time.getTime() > Date.now()
        }
      },
      chartInstance: null,
      detailDialogVisible: false,
      detailLoading: false,
      detailData: null,
      detailTab: 'accounts'
    }
  },
  mounted() {
    this.loadHistory()
    // ⚠️ 此时 activeTab='daily',图表容器在隐藏的 statistics pane 内,宽度为 0,
    //    echarts 会把 0 宽缓存下来 → 切过去后画布永远空白。
    //    真正的尺寸修正在 onTabClick 里做。 @yutiansut @quantaxis
    this.initChart()
  },
  beforeDestroy() {
    if (this._resizeHandler) {
      window.removeEventListener('resize', this._resizeHandler)
    }
    if (this.chartInstance) {
      this.chartInstance.dispose()
    }
  },
  methods: {
    // 金额格式化 —— 统一两位小数。@yutiansut @quantaxis
    //
    // ⚠️ 原来这几个统计卡用的是裸 `toLocaleString()`(无 options),
    // 而**同文件下方表格**用的是带 `minimumFractionDigits: 2` 的版本。
    // 结果:1366×768 实测卡片显示 `总资金 4,232,972,133.148`(**三位小数**)、
    // `可用资金 4,156,965,285.352`,而正下方表格里同类金额是两位。
    // 375×667 下这还会让 `.stat-value` 溢出容器(scrollWidth 164px / 槽宽 48px)。
      // ✨ 委托全局 `$money`(main.js):判空 → isFinite → **量级上限 1e15**。
      // 全仓曾有 13 个各写各的金额格式化器,**没有一个带量级闸**,
      // 于是 `balance = 1e300` 的污染账户在 6 个页面渲染成 404 个字符
      // (`Number.isFinite(1e300)` 为真,只判 isFinite 挡不住)。@yutiansut @quantaxis
    fmtMoney (v) {
      return this.$money(v)
    },

    // 取真实行情填「最新价 / 涨跌幅」;失败就保持 null,由表格显示「-」
    // —— **宁可不显示,也不编。** @yutiansut @quantaxis
    // 第二参已成死参(改用 row.settlement_price 后),保留签名以免漏改调用点;
    // 实际不再读它。@yutiansut @quantaxis
    async fetchLastPrice (instrumentId, _settlementPriceUnused) {
      if (!instrumentId) return
      try {
        const res = await getTick(instrumentId)
        const d = (res && res.data) || res
        const last = Number(d && d.last_price)
        if (!Number.isFinite(last) || last === 0) return
        const row = this.settlementPrices.find(p => p.instrument_id === instrumentId)
        if (!row) return
        // ⚠️ 用**行上的现值**而不是调用时捕获的参数 @yutiansut @quantaxis
        // 回验实测:同一合约先后改价时,迟到的应答会拿旧结算价算涨跌幅,
        // 配着新结算价一起显示(`settlement_price:2000` 配 `change_rate:1.6342`,
        // 而 1.6342 = (2634.2-1000)/1000 用的是旧价 1000)。读 row 就永远自洽。
        const sp = Number(row.settlement_price)
        this.$set(row, 'last_price', last)
        // 涨跌幅相对结算价 —— 这正是操作员要判断的「我填的价离市场多远」
        this.$set(row, 'change_rate', Number.isFinite(sp) && sp !== 0 ? (last - sp) / sp : null)
      } catch (e) {
        // 合约无行情(如刚上市)会 404 —— 属正常,保持「-」。
        // ⚠️ 注意**不是静默**:`api/request.js` 的响应拦截器会对失败弹提示。
        // 这里不再自行弹,避免同一件事弹两条。@yutiansut @quantaxis
        console.warn('[settlement] 取行情失败,最新价显示为 -:', instrumentId, e && e.message)
      }
    },
    // 切到「结算统计」时才有真实宽度,此时重新测量画布。
    // 若 mount 时 init 失败(容器不存在)则补一次 init。 @yutiansut @quantaxis
    onTabClick(tab) {
      if ((tab && tab.name) !== 'statistics') return
      this.$nextTick(() => {
        if (this.chartInstance) {
          this.chartInstance.resize()
        } else {
          this.initChart()
        }
      })
    },
    // 加载结算历史
    async loadHistory() {
      this.historyLoading = true; this.loadError = null
      try {
        const params = {}
        if (this.historyDateRange && this.historyDateRange.length === 2) {
          params.start_date = this.historyDateRange[0]
          params.end_date = this.historyDateRange[1]
        }

        const records = await getSettlementHistory(params)
        this.historyList = records || []
        this.updateStatisticsFromHistory()
        this.updateChart()
      } catch (error) {
        this.loadError = this.$errMsg(error, '加载失败')
        this.$message.error('加载结算历史失败')
        console.error(error)
      } finally {
        this.historyLoading = false
      }
    },

    // 加载统计数据（从历史记录中计算）
    async loadStatistics() {
      this.updateStatisticsFromHistory()
    },

    // 显示设置结算价对话框
    showSetPriceDialog() {
      this.priceForm = {
        instrument_id: '',
        settlement_price: 0
      }
      this.priceDialogVisible = true
    },

    // 添加结算价
    addSettlementPrice() {
      if (!this.priceForm.instrument_id || !(this.priceForm.settlement_price > 0)) {
        // ✨ 用 !(x > 0) 而非 x <= 0：清空 el-input-number 会写 undefined，
        // 而 `undefined <= 0` 为 false 会漏过守卫，导致 undefined 价被加进待结算列表、
        // 执行时经 batchSetSettlementPrices 发出换 400。!(undefined > 0) 为 true，能拦住。@yutiansut @quantaxis
        this.$message.warning('请填写完整信息')
        return
      }

      // 检查是否已存在
      const index = this.settlementPrices.findIndex(
        p => p.instrument_id === this.priceForm.instrument_id
      )

      if (index >= 0) {
        // 更新
        // ⚠️ 不再用 Math.random() 伪造行情 @yutiansut @quantaxis
        //
        // 原来这里写的是:
        //   last_price:  settlement_price * (1 + Math.random()*0.02 - 0.01)
        //   change_rate: Math.random()*0.04 - 0.02
        // 这是**结算页** —— 结算价决定所有账户的盯市盈亏,操作员要对着「最新价」
        // 判断自己填的结算价是否离谱。给他一个编造的数,比不给更坏:
        // 实测同一输入(结算价 100)两次渲染出「99.08 / +1.12%」和「100.46 / +0.09%」,
        // 第一行最新价**低于**结算价却标红显示上涨 —— 编的数连自洽都做不到。
        // 而批量导入那条路径(:498)用的是 `last_price: value, change_rate: 0`,
        // 两条路径口径还不一致。
        //
        // 现在:取真实行情(getTick,现成接口);取不到就留 null,表格显示「-」。
        this.settlementPrices.splice(index, 1, { ...this.priceForm, last_price: null, change_rate: null })
      } else {
        this.settlementPrices.push({ ...this.priceForm, last_price: null, change_rate: null })
      }
      this.fetchLastPrice(this.priceForm.instrument_id, this.priceForm.settlement_price)

      this.priceDialogVisible = false
      this.$message.success('添加成功')
    },

    // 删除结算价
    removePrice(index) {
      this.settlementPrices.splice(index, 1)
    },

    // 批量导入
    importPrices() {
      if (this.$refs.priceFileInput) {
        this.$refs.priceFileInput.click()
      }
    },

    handlePriceFileUpload(event) {
      const file = event.target.files && event.target.files[0]
      if (!file) return
      const reader = new FileReader()
      reader.onload = e => {
        const lines = (e.target.result || '').split(/\r?\n/)
        lines.forEach(line => {
          const [instrument, price] = line.split(',').map(item => item && item.trim())
          const value = parseFloat(price)
          if (instrument && !isNaN(value)) {
            const existingIndex = this.settlementPrices.findIndex(p => p.instrument_id === instrument)
            // 与上面同源:不编造。原来这里塞 `last_price: value, change_rate: 0`
            // (把结算价当最新价),与另一条路径的随机数口径矛盾。@yutiansut @quantaxis
            const payload = {
              instrument_id: instrument,
              settlement_price: value,
              last_price: null,
              change_rate: null
            }
            if (existingIndex >= 0) {
              this.settlementPrices.splice(existingIndex, 1, payload)
            } else {
              this.settlementPrices.push(payload)
            }
          }
        })
        this.$message.success('已导入结算价')
      }
      reader.onerror = () => this.$message.error('读取文件失败')
      reader.readAsText(file)
      event.target.value = ''
    },

    // 执行结算
    async executeSettlement() {
      if (this.settlementPrices.length === 0) {
        this.$message.warning('请先设置结算价')
        return
      }

      try {
        await this.$confirm(
          `确定要执行 ${this.settlementForm.settlementDate} 的日终结算吗？`,
          '结算确认',
          {
            type: 'warning',
            confirmButtonText: '确定执行',
            cancelButtonText: '取消'
          }
        )

        this.executing = true

        // 步骤1：批量设置结算价
        const pricesData = {
          prices: this.settlementPrices.map(p => ({
            instrument_id: p.instrument_id,
            settlement_price: p.settlement_price
          }))
        }

        // request 拦截器已解包 { success, data, error }：任一步失败都会 reject 到 catch
        await batchSetSettlementPrices(pricesData)

        // 步骤2：执行日终结算
        await executeSettlement()
        this.$message.success('结算执行成功')
        this.settlementPrices = []
        this.loadHistory()
        this.loadStatistics()
      } catch (error) {
        if (error !== 'cancel') {
          this.$message.error(this.$errMsg(error, '结算执行失败'))
          console.error(error)
        }
      } finally {
        this.executing = false
      }
    },

    // 查看详情
    async viewDetail(row) {
      this.detailDialogVisible = true
      this.detailTab = 'accounts'
      this.detailLoading = true; this.detailError = null
      try {
        const detail = await getSettlementDetail(row.settlement_date)
        this.detailData = detail || {}
      } catch (error) {
        this.detailError = this.$errMsg(error, '加载失败')
        this.$message.error('加载结算详情失败')
        console.error(error)
        this.detailData = null
      } finally {
        this.detailLoading = false
      }
    },

    // 获取状态标签颜色
    getStatusTagType(status) {
      const statusMap = {
        'success': 'success',
        'failed': 'danger',
        'partial': 'warning'
      }
      return statusMap[status] || ''
    },

    // 获取状态名称
    getStatusName(status) {
      const statusMap = {
        'success': '成功',
        'failed': '失败',
        'partial': '部分成功'
      }
      return statusMap[status] || status
    },

    initChart() {
      this.$nextTick(() => {
        const dom = document.getElementById('settlement-chart')
        if (dom) {
          this.chartInstance = echarts.init(dom)
          this.updateChart()
          this._resizeHandler = () => {
            this.chartInstance && this.chartInstance.resize()
          }
          window.addEventListener('resize', this._resizeHandler)
        }
      })
    },

    updateChart() {
      if (!this.chartInstance) return
      const dates = this.historyList.map(item => item.settlement_date)
      const profits = this.historyList.map(item => item.total_profit || 0)
      // ✨ 空态必须**说明原因**,不能只画一个空坐标框。@yutiansut @quantaxis
      //
      // 本系统从不自动结算(只能手动触发),所以历史长期为 0 条 ——
      // 原先渲染出的是「两条轴线 + 无刻度 + 无数据 + 无一个字」,
      // 与「加载失败」「还没加载完」在外观上完全无法区分。
      // 它没有编造数据(比 dashboard 那张 33/33/33 轻),但同样没告诉用户为什么是空的。
      // 复用 dashboard 已经验证过的做法:空时给标题文案,并把坐标轴一起隐掉。
      // 标题样式走全局 `$chartTitle`(main.js),两处共用一份。
      const isEmpty = dates.length === 0
      const option = {
        title: isEmpty
          ? this.$chartTitle('暂无结算记录', '系统尚未执行过日终结算')
          : this.$chartTitle(null),
        tooltip: { trigger: 'axis' },
        grid: { left: '3%', right: '4%', bottom: '3%', containLabel: true },
        xAxis: { type: 'category', data: dates, show: !isEmpty },
        yAxis: { type: 'value', show: !isEmpty },
        series: [
          {
            name: '总盈亏',
            type: 'line',
            smooth: true,
            data: profits,
            areaStyle: { opacity: 0.1 }
          }
        ]
      }
      this.chartInstance.setOption(option)
    },

    updateStatisticsFromHistory() {
      if (!this.historyList.length) {
        this.statistics = {
          monthSettlementCount: 0,
          profitAccountsCount: 0,
          lossAccountsCount: 0,
          totalCommission: 0
        }
        return
      }
      this.statistics = {
        monthSettlementCount: this.historyList.length,
        settledAccountsCount: this.historyList.reduce((sum, item) => sum + (Number(item.settled_accounts) || 0), 0),
        failedAccountsCount: this.historyList.reduce((sum, item) => sum + (Number(item.failed_accounts) || 0), 0),
        totalCommission: this.historyList.reduce((sum, item) => sum + (Number(item.total_commission) || 0), 0)
      }
    },

    // ⚠️ 同一文件里的**第二个**金额格式化器(上面还有 `fmtMoney`)——
    // 两个并存,只有一个带守卫。已一并委托 `$money`。@yutiansut @quantaxis
    formatCurrency(value) {
      return this.$money(value)
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

.settlement-container {
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
  margin-bottom: 20px;
}

.page-header h2 {
  margin: 0;
  color: $dark-text-primary !important;
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
  }

  ::v-deep .el-button--text {
    color: $primary-color !important;

    &:hover {
      color: lighten($primary-color, 15%) !important;
    }
  }
}

.settlement-card {
  margin-bottom: 20px;

  ::v-deep .el-card {
    background: $dark-bg-card !important;
    border: 1px solid $dark-border !important;

    .el-card__header {
      background: $dark-bg-secondary !important;
      border-bottom: 1px solid $dark-border !important;
      color: $dark-text-primary !important;
    }

    .el-card__body {
      background: $dark-bg-card !important;
    }
  }
}

.chart-wrapper {
  ::v-deep .el-card {
    background: $dark-bg-card !important;
    border: 1px solid $dark-border !important;

    .el-card__body {
      background: $dark-bg-card !important;
    }
  }
}

.table-toolbar {
  margin-bottom: 15px;

  ::v-deep .el-date-editor {
    .el-input__inner {
      background: $dark-bg-tertiary !important;
      border-color: $dark-border !important;
      color: $dark-text-primary !important;
    }
  }
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

// 表单样式
::v-deep .el-form {
  .el-form-item__label {
    color: $dark-text-secondary !important;
  }

  .el-input__inner {
    background: $dark-bg-tertiary !important;
    border-color: $dark-border !important;
    color: $dark-text-primary !important;

    &::placeholder {
      color: $dark-text-muted !important;
    }
  }

  .el-input-number {
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

  .el-dialog__footer {
    background: $dark-bg-card !important;
    border-top: 1px solid $dark-border !important;
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

// 下拉选择框
::v-deep .el-select-dropdown {
  background: $dark-bg-secondary !important;
  border-color: $dark-border !important;

  .el-select-dropdown__item {
    color: $dark-text-primary !important;

    &:hover, &.hover {
      background: $dark-bg-tertiary !important;
    }

    &.selected {
      color: $primary-color !important;
      font-weight: 600;
    }
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

  &.el-button--info {
    background: $dark-bg-tertiary !important;
    border-color: $dark-border !important;
    color: $dark-text-primary !important;
  }
}
</style>
