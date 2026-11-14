<template>
  <div class="orders-page qa-flex-page">
    <el-card>
      <div slot="header" class="card-header">
        <!-- ⚠️ 口径:本页数据来自 `/api/order/user/{id}`,读的是 order_router 的**内存注册表**。
             重启时只恢复 SUBMITTED/ALIVE 的活单(order_router.rs:2559),已成交/已撤的
             历史单不在其中。实测某做市账户 QIFI 里有 131,223 单,本接口返回 total=109。
             与 admin/market-overview.vue 的「活跃订单」同源同口径,那边已加同样的说明。
             @yutiansut @quantaxis -->
        <span>
          订单管理
          <el-tooltip
            content="仅显示当前在册订单（挂单与本次运行期间产生的单）。已成交/已撤的历史订单不在此列表。"
            placement="top"
          >
            <i class="el-icon-question" style="color:#909399;font-size:12px;cursor:help;margin-left:4px;"></i>
          </el-tooltip>
        </span>
        <el-button type="primary" size="small" @click="showSubmitDialog">
          <i class="el-icon-plus"></i> 下单
        </el-button>
      </div>

      <el-form :inline="true" size="small">
        <el-form-item label="交易账户">
          <el-select v-model="queryAccountId" placeholder="选择账户" clearable filterable>
            <el-option
              v-for="account in accounts"
              :key="account.account_id"
              :label="`${account.account_name} (${account.account_id.slice(0, 8)}...)`"
              :value="account.account_id"
            />
          </el-select>
        </el-form-item>
        <el-form-item label="合约">
          <el-select v-model="queryInstrument" placeholder="选择合约" clearable>
            <el-option label="IF2501" value="IF2501" />
            <el-option label="IF2502" value="IF2502" />
            <el-option label="IC2501" value="IC2501" />
            <el-option label="IH2501" value="IH2501" />
          </el-select>
        </el-form-item>
        <el-form-item>
          <el-button type="primary" @click="handleQuery">查询</el-button>
          <el-button @click="handleReset">重置</el-button>
        </el-form-item>
      </el-form>

      <!-- ✨ 表格高度改为跟随视口, 不再写死 500px @yutiansut @quantaxis
           实测 1366x768: 表格上沿距内容区顶部 171px
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
        :empty-text="$emptyText(loadError, '暂无订单')"
        :data="orderList"
        border
        stripe
        :height="'100%'"
        v-loading="loading"
        style="width: 100%"
      >
        <el-table-column prop="order_id" label="订单ID" min-width="200" show-overflow-tooltip/>
        <el-table-column prop="user_id" label="账户ID" min-width="150" show-overflow-tooltip>
          <template slot-scope="scope">
            {{ getAccountName(scope.row.user_id) }}
          </template>
        </el-table-column>
        <el-table-column prop="instrument_id" label="合约" min-width="100" show-overflow-tooltip/>
        <el-table-column prop="direction" label="方向" width="80" align="center">
          <template slot-scope="scope">
            <el-tag :type="scope.row.direction === 'BUY' ? 'danger' : 'success'" size="mini">
              {{ scope.row.direction === 'BUY' ? '买入' : '卖出' }}
            </el-tag>
          </template>
        </el-table-column>
        <el-table-column prop="offset" label="开平" width="80" align="center">
          <template slot-scope="scope">
            {{ scope.row.offset === 'OPEN' ? '开仓' : '平仓' }}
          </template>
        </el-table-column>
        <el-table-column prop="price" label="价格" min-width="100" align="right" show-overflow-tooltip>
          <template slot-scope="scope">
            {{ $num(scope.row.price, 2) }}
          </template>
        </el-table-column>
        <el-table-column prop="volume" label="数量" width="80" align="right" show-overflow-tooltip>
          <template slot-scope="{ row }">{{ $num(row.volume, 0) }}</template>
        </el-table-column>
        <el-table-column prop="filled_volume" label="成交量" width="80" align="right" show-overflow-tooltip>
          <template slot-scope="{ row }">{{ $num(row.filled_volume, 0) }}</template>
        </el-table-column>
        <el-table-column prop="status" label="状态" min-width="100" align="center">
          <template slot-scope="scope">
            <el-tag :type="getStatusType(scope.row.status)" size="mini">
              {{ getStatusText(scope.row.status) }}
            </el-tag>
          </template>
        </el-table-column>
        <!-- 后端 OrderInfo(models.rs:97) 只有 submit_time / update_time,
             **没有 created_at** —— 原来绑 created_at,整列 7 行全空白。
             submit_time 是纳秒(order_router.rs:734 timestamp_nanos_opt),
             与 trades 页 trade.timestamp 同一约定。@yutiansut @quantaxis -->
        <el-table-column prop="submit_time" label="创建时间" min-width="160" :formatter="formatSubmitTime" />
        <el-table-column label="操作" width="150">
          <template slot-scope="scope">
            <el-button
              v-if="scope.row.status === 'Pending'"
              type="text"
              size="small"
              @click="handleCancel(scope.row)"
            >
              撤单
            </el-button>
            <el-button type="text" size="small" @click="handleViewDetail(scope.row)">
              详情
            </el-button>
          </template>
        </el-table-column>
      </el-table>

      <!-- ✨ 分页条 —— 仅在按账户查询时显示 @yutiansut @quantaxis
           不分页时单账户 5,951 单要 18.4 秒出表、905MB heap;
           两个活跃账户合计 11,902 行 → 39.9 秒 / 1.7GB。 -->
      <el-pagination
        v-if="queryAccountId && totalOrders > pageSize"
        style="margin-top: 12px; text-align: right"
        :current-page.sync="currentPage"
        :page-sizes="[50, 100, 200]"
        :page-size="pageSize"
        :total="totalOrders"
        layout="total, sizes, prev, pager, next"
        @current-change="fetchAccountOrders"
        @size-change="onPageSizeChange"
      />
    </el-card>

    <!-- 下单对话框 -->
    <el-dialog title="提交订单" :visible.sync="submitDialogVisible" width="500px">
      <el-form :model="submitForm" :rules="submitRules" ref="submitForm" label-width="100px">
        <el-form-item label="交易账户" prop="user_id">
          <!-- ✨ 必须同步 account_id @yutiansut @quantaxis
               原先下拉只改 user_id,而 account_id 停在 showSubmitDialog 设的
               **第一个**账户上。多账户用户选第二个账户下单 →
               「user_id=账户B / account_id=账户A」→ 后端返回
               403 code 4003「Account X does not belong to user Y」,100% 失败。 -->
          <el-select
            v-model="submitForm.user_id"
            filterable
            style="width: 100%"
            @change="onSubmitAccountChange"
          >
            <el-option
              v-for="account in accounts"
              :key="account.account_id"
              :label="`${account.account_name} (可用: ¥${$money(account.available)})`"
              :value="account.account_id"
            >
              <div style="display: flex; justify-content: space-between;">
                <span>{{ account.account_name }}</span>
                <span style="color: #8492a6; font-size: 12px;">
                  可用: ¥{{ $money(account.available) }}
                </span>
              </div>
            </el-option>
          </el-select>
        </el-form-item>
        <el-form-item label="合约" prop="instrument_id">
          <el-select v-model="submitForm.instrument_id" style="width: 100%">
            <el-option label="IF2501 - 沪深300股指期货" value="IF2501" />
            <el-option label="IF2502 - 沪深300股指期货" value="IF2502" />
            <el-option label="IC2501 - 中证500股指期货" value="IC2501" />
            <el-option label="IH2501 - 上证50股指期货" value="IH2501" />
          </el-select>
        </el-form-item>
        <el-form-item label="方向" prop="direction">
          <el-radio-group v-model="submitForm.direction">
            <el-radio label="BUY">买入</el-radio>
            <el-radio label="SELL">卖出</el-radio>
          </el-radio-group>
        </el-form-item>
        <el-form-item label="开平" prop="offset">
          <el-radio-group v-model="submitForm.offset">
            <el-radio label="OPEN">开仓</el-radio>
            <el-radio label="CLOSE">平仓</el-radio>
          </el-radio-group>
        </el-form-item>
        <!-- 市价单隐藏价格框 —— 与 trade/components/OrderForm.vue:55 一致。
             提交处也做了归一(见 handleSubmitOrder),两道都留着:
             隐藏是给用户看的,归一是防止任何路径漏掉。@yutiansut @quantaxis -->
        <el-form-item v-if="submitForm.order_type === 'LIMIT'" label="价格" prop="price">
          <el-input-number
            v-model="submitForm.price"
            :min="0"
            :step="0.2"
            :precision="1"
            style="width: 100%"
          />
        </el-form-item>
        <el-form-item label="数量" prop="volume">
          <el-input-number
            v-model="submitForm.volume"
            :min="1"
            :step="1"
            style="width: 100%"
          />
        </el-form-item>
        <el-form-item label="订单类型" prop="order_type">
          <el-select v-model="submitForm.order_type" style="width: 100%">
            <el-option label="限价单" value="LIMIT" />
            <el-option label="市价单" value="MARKET" />
          </el-select>
        </el-form-item>
      </el-form>
      <div slot="footer">
        <el-button @click="submitDialogVisible = false">取消</el-button>
        <el-button type="primary" @click="handleSubmitOrder" :loading="submitting">
          提交
        </el-button>
      </div>
    </el-dialog>
  </div>
</template>

<script>
import { submitOrder, cancelOrder, queryUserOrders, getUserAccounts } from '@/api'
import { extractErrorMessage } from '@/api/request'
import { mapGetters, mapActions } from 'vuex'

export default {
  name: 'Orders',
  computed: {
    ...mapGetters(['currentUser'])
  },
  data() {
    return {
      // 加载失败原因(null = 未失败)—— 供表格 empty-text 区分两种空
      loadError: null,
      currentPage: 1,
      pageSize: 100,
      totalOrders: 0,
      loading: false,
      submitting: false,
      orderList: [],
      accounts: [],
      queryAccountId: '',
      queryInstrument: '',
      submitDialogVisible: false,
      submitForm: {
        user_id: '',        // ✨ 交易所模式：与account_id相同 @yutiansut @quantaxis
        account_id: '',     // 交易账户ID
        instrument_id: 'IF2501',
        direction: 'BUY',
        offset: 'OPEN',
        price: 3800,
        volume: 1,
        order_type: 'LIMIT'
      },
      submitRules: {
        user_id: [{ required: true, message: '请选择交易账户', trigger: 'change' }],
        instrument_id: [{ required: true, message: '请选择合约', trigger: 'change' }],
        direction: [{ required: true, message: '请选择方向', trigger: 'change' }],
        offset: [{ required: true, message: '请选择开平', trigger: 'change' }],
        price: [{ required: true, message: '请输入价格', trigger: 'blur' }],
        volume: [{ required: true, message: '请输入数量', trigger: 'blur' }],
        order_type: [{ required: true, message: '请选择订单类型', trigger: 'change' }]
      }
    }
  },
  async mounted() {
    // 必须先拿到账户列表：订单只能按 account_id 查 @yutiansut @quantaxis
    await this.loadAccounts()
    this.loadOrders()
  },
  methods: {
    // ✨ 映射 Vuex websocket 模块的 actions @yutiansut @quantaxis
    ...mapActions('websocket', ['fetchUserAccounts']),

    async loadAccounts() {
      if (!this.currentUser) {
        return
      }

      try {
        const res = await getUserAccounts(this.currentUser)
        this.accounts = res.accounts || []
      } catch (error) {
        console.error('加载账户列表失败:', error)
      }
    },

    getAccountName(accountId) {
      const account = this.accounts.find(acc => acc.account_id === accountId)
      return account ? `${account.account_name} (${accountId.slice(0, 8)}...)` : accountId
    },
    getStatusType(status) {
      const map = {
        'Pending': 'warning',
        'PartiallyFilled': 'info',
        'Filled': 'success',
        'Cancelled': 'info',
        'Rejected': 'danger'
      }
      return map[status] || 'info'
    },

    getStatusText(status) {
      const map = {
        'Pending': '待成交',
        'PartiallyFilled': '部分成交',
        'Filled': '已成交',
        'Cancelled': '已撤销',
        'Rejected': '已拒绝'
      }
      return map[status] || status
    },

    async loadOrders() {
      if (!this.currentUser) {
        this.$message.warning('请先登录')
        return
      }

      // ✨ 后端 /api/order/user/{id} 的索引键实际是 account_id
      // （order_router.rs: user_orders.entry(req.account_id)），
      // 传用户 UUID 恒返回空列表。这里遍历该用户的账户逐个查询后聚合，
      // 与 views/trade/index.vue 的用法保持一致。@yutiansut @quantaxis
      if (!this.accounts.length) {
        await this.loadAccounts()
      }

      this.loading = true
      try {
        this.loadError = null   // 每次重试先清除上次的失败标记
        // ⚠️ 逐账户 catch 是为了「一个账户失败不拖垮整页」,但原写法
        //    `.catch(() => null)` 把异常**彻底吞掉**:外层 try 永不抛,
        //    loadError 恒为 null,表格显示「暂无订单」。实测 9 个账户全部 500
        //    (9 条 toast 为证)时页面仍显示「暂无订单」,下面 catch 里的
        //    extractErrorMessage 成了不可达死代码。
        //    改法:保留逐账户容错,记住第一个错误,**全军覆没时抛出去**。
        //    @yutiansut @quantaxis
        let firstError = null
        // ✨ 把合约筛选**真的传给接口**。@yutiansut @quantaxis
        //
        // `queryInstrument` 声明了(:251)、绑了下拉框(:36)、重置时也清(:419),
        // **唯独不传给 queryUserOrders** —— 回验代理网络层实测:
        // 选了 IF2502 点查询,请求仍是 `?page=1&page_size=100`,无 instrument_id。
        // 后端 `handlers.rs:690` 的 `match_instrument` 本来就支持。
        const filterParams = {}
        if (this.queryInstrument) filterParams.instrument_id = this.queryInstrument
        const pages = await Promise.all(
          this.accounts.map(acc =>
            queryUserOrders(acc.account_id, filterParams).catch(err => {
              if (!firstError) firstError = err
              return null
            })
          )
        )
        const failed = pages.filter(d => d === null).length
        if (failed > 0 && failed === this.accounts.length) {
          throw firstError || new Error('全部账户的订单都加载失败')
        }
        if (failed > 0) {
          console.warn('[orders] ' + failed + '/' + this.accounts.length + ' 个账户的订单加载失败,列表为部分数据')
        }
        this.orderList = pages.reduce((all, data) => {
          if (!data) return all
          return all.concat(Array.isArray(data) ? data : (data.orders || []))
        }, [])
      } catch (error) {
        // ✨ 记录失败原因 —— 否则表格只显示默认「暂无数据」,
        //    与「真的没有数据」完全同形,用户无从判断。@yutiansut @quantaxis
        this.loadError = this.$errMsg(error, '加载失败')
        this.$message.error('加载订单失败: ' + extractErrorMessage(error))
        this.orderList = []
      } finally {
        this.loading = false
      }
    },

    // 纳秒时间戳 → 本地时间串
    formatSubmitTime(row, column, value) {
      if (!value) return '—'
      const ms = Number(value) / 1e6
      if (!isFinite(ms) || ms <= 0) return '—'
      return new Date(ms).toLocaleString('zh-CN')
    },

    handleQuery() {
      // ✨ 单账户查询走分页 —— 不分页时 5,951 单要 18.4 秒 / 905MB heap
      //    @yutiansut @quantaxis
      this.currentPage = 1
      this.fetchAccountOrders()
    },

    onPageSizeChange(size) {
      this.pageSize = size
      this.currentPage = 1
      this.fetchAccountOrders()
    },

    fetchAccountOrders() {
      if (!this.queryAccountId) { this.loadOrders(); return }
      this.loading = true
      queryUserOrders(this.queryAccountId, {
        page: this.currentPage,
        page_size: this.pageSize
      })
        .then(data => {
          this.orderList = data.orders || []
          this.totalOrders = data.total || 0
        })
        .catch(err => {
          // ⚠️ 原来这里只弹 toast,既不置 loadError 也不清 orderList。@yutiansut @quantaxis
          // 两个后果:① toast 3 秒后消失,表格 empty-text 仍是「暂无订单」——
          // 「查询失败」与「这个账户确实没单」在屏幕上完全同形;
          // ② orderList 保留**上一个账户**的行 —— 换账户查询失败时,
          // 用户看到的是别人的订单挂在自己账号下。比空表更糟。
          // 本页 :63 的 $emptyText(loadError, ...) 机制早已铺好,这里只是绕过了它。
          this.loadError = extractErrorMessage(err)
          this.orderList = []
          this.totalOrders = 0
          this.$message.error('查询失败: ' + this.loadError)
        })
        .finally(() => {
          this.loading = false
        })
    },

    handleReset() {
      this.queryAccountId = ''
      this.queryInstrument = ''
      this.loadOrders()
    },

    // 账户下拉变更时同步 account_id —— 交易所模式下两者都是 account_id
    onSubmitAccountChange(val) {
      this.submitForm.account_id = val
    },

    showSubmitDialog() {
      // ✨ 交易所模式：自动选择第一个账户，user_id和account_id都设置为账户ID @yutiansut @quantaxis
      if (this.accounts.length > 0) {
        const accountId = this.accounts[0].account_id
        this.submitForm.user_id = accountId
        this.submitForm.account_id = accountId
      }
      this.submitDialogVisible = true
      this.$nextTick(() => {
        this.$refs.submitForm && this.$refs.submitForm.resetFields()
        // 重新设置账户ID（因为resetFields会清空）
        if (this.accounts.length > 0) {
          const accountId = this.accounts[0].account_id
          this.submitForm.user_id = accountId
          this.submitForm.account_id = accountId
        }
      })
    },

    handleSubmitOrder() {
      this.$refs.submitForm.validate(valid => {
        if (valid) {
          this.submitting = true
          // ⚠️ 市价单必须把 price 归零,不能原样提交表单。@yutiansut @quantaxis
          //
          // 原来是 `submitOrder(this.submitForm)` —— 整个表单照发。
          // 而价格框无条件显示、`data()` 里默认 `price: 3800`,
          // 于是切到"市价单"后仍带着这个**陈旧限价**提交。
          // 回验代理截获的实际请求体:
          //   {"…","price":3800,"volume":1,"order_type":"MARKET"}
          //
          // 后端两处会因此走偏:
          //   · `order_router.rs:490` 在 `price > 0` 时**跳过市价解析**
          //     ⇒ 实际按 3800 这个限价撮合;
          //   · `pre_trade_check.rs:262` 对 MARKET **提前返回**
          //     ⇒ 跳过订单金额上限,风控闸门失效。
          //
          // 正确写法在同项目 `trade/components/OrderForm.vue:311`:
          //   `price: order_type === 'LIMIT' ? form.price : 0`
          // 这里对齐它。
          const payload = {
            ...this.submitForm,
            price: this.submitForm.order_type === 'LIMIT' ? this.submitForm.price : 0
          }
          submitOrder(payload)
            .then(() => {
              this.$message.success('订单提交成功')
              this.submitDialogVisible = false
              this.loadOrders()
            })
            .catch(() => {
              this.$message.error('订单提交失败')
            })
            .finally(() => {
              this.submitting = false
            })
        }
      })
    },

    handleCancel(row) {
      this.$confirm('确认撤销该订单?', '提示', {
        type: 'warning'
      }).then(() => {
        // ✨ 交易所模式：user_id 和 account_id 都使用账户ID @yutiansut @quantaxis
        // 后端 verify_account_ownership 会检测 user_id == account_id，自动跳过所有权验证
        cancelOrder({
          user_id: row.user_id,     // 账户ID (交易所模式)
          account_id: row.user_id,  // 必须传递，否则后端会走 get_default_account 失败
          order_id: row.order_id
        })
          .then(() => {
            this.$message.success('撤单成功')
            this.loadOrders()

            // ✨ 撤单后刷新账户数据，更新可用资金显示 @yutiansut @quantaxis
            const userInfo = this.$store.state.userInfo
            const userId = userInfo && userInfo.user_id
            if (userId) {
              this.fetchUserAccounts(userId).catch(err => {
                console.warn('[Orders] Failed to refresh account after cancel:', err)
              })
            }
          })
          .catch(() => {
            this.$message.error('撤单失败')
          })
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
      this.$alert(pre, '订单详情', {
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
// @yutiansut @quantaxis - 专业量化交易系统订单页面样式
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
$warning-color: #faad14;

.orders-page {
  min-height: 100%;
  background: $dark-bg-primary;
  padding: 0;

  // 卡片样式
  ::v-deep .el-card {
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

    .el-select-dropdown {
      background: $dark-bg-card;
      border: 1px solid $dark-border;
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

    // 固定列
    .el-table__fixed-right {
      background: transparent;

      th.el-table__cell,
      td.el-table__cell {
        background: $dark-bg-card;
      }
    }

    // 空数据
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

    &--warning {
      background: rgba($warning-color, 0.15);
      color: $warning-color;
    }

    &--info {
      background: rgba($dark-text-secondary, 0.15);
      color: $dark-text-secondary;
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

// 下单对话框
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
      background: linear-gradient(135deg, $primary-color 0%, #096dd9 100%);
      border: none;
    }
  }

  .el-form-item__label {
    color: $dark-text-secondary;
  }

  .el-input__inner,
  .el-select .el-input__inner {
    background: $dark-bg-tertiary;
    border: 1px solid $dark-border;
    color: $dark-text-primary;

    &:focus {
      border-color: $primary-color;
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

  // 选项下拉样式
  .el-select-dropdown__item {
    color: $dark-text-primary;

    &:hover {
      background: $dark-bg-tertiary;
    }

    &.selected {
      color: $primary-color;
      font-weight: 600;
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
