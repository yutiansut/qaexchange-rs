<template>
  <div class="dashboard">
    <!-- 统计卡片区域 -->
    <div class="stat-cards">
      <div class="stat-card">
        <div class="stat-header">
          <div class="stat-icon accounts">
            <i class="el-icon-user-solid"></i>
          </div>
          <!-- ❌ 已删除写死的趋势徽章「12%」@yutiansut @quantaxis
               它是字面量,不来自任何接口 —— 而旁边的 stat-value 是**真数据**,
               一个真数字配一个假涨幅,比纯假更误导(用户会以为整块都是实测)。
               要恢复需后端提供环比数据(当前 /api/monitoring/* 都没有)。 -->
        </div>
        <div class="stat-body">
          <div class="stat-value">{{ $num(monitoring.accounts.total_count, 0) }}</div>
          <div class="stat-title">总账户数</div>
        </div>
        <div class="stat-footer">
          <span class="label">活跃账户</span>
          <span class="value">{{ $num(monitoring.accounts.active_count, 0) }}</span>
        </div>
      </div>

      <div class="stat-card">
        <div class="stat-header">
          <div class="stat-icon balance">
            <i class="el-icon-wallet"></i>
          </div>
          <!-- ❌ 已删除写死的趋势徽章「8.5%」@yutiansut @quantaxis
               它是字面量,不来自任何接口 —— 而旁边的 stat-value 是**真数据**,
               一个真数字配一个假涨幅,比纯假更误导(用户会以为整块都是实测)。
               要恢复需后端提供环比数据(当前 /api/monitoring/* 都没有)。 -->
        </div>
        <div class="stat-body">
          <div class="stat-value">¥{{ formatNumber(monitoring.accounts.total_balance) }}</div>
          <div class="stat-title">总权益</div>
        </div>
        <div class="stat-footer">
          <span class="label">可用资金</span>
          <span class="value">¥{{ formatNumber(monitoring.accounts.total_available) }}</span>
        </div>
      </div>

      <div class="stat-card">
        <div class="stat-header">
          <div class="stat-icon margin">
            <i class="el-icon-pie-chart"></i>
          </div>
          <div class="stat-trend" :class="marginUtilization > 50 ? 'warning' : 'normal'">
            <span>{{ marginUtilizationText }}</span>
          </div>
        </div>
        <div class="stat-body">
          <div class="stat-value">¥{{ formatNumber(monitoring.accounts.total_margin_used) }}</div>
          <div class="stat-title">保证金占用</div>
        </div>
        <div class="stat-footer">
          <div class="progress-bar">
            <div class="progress" :style="{ width: marginUtilizationWidth }"></div>
          </div>
        </div>
      </div>

      <div class="stat-card">
        <div class="stat-header">
          <div class="stat-icon orders">
            <i class="el-icon-document"></i>
          </div>
          <div class="stat-trend normal">
            <span>{{ $num(monitoring.orders.pending_count, 0) }} 待处理</span>
          </div>
        </div>
        <div class="stat-body">
          <div class="stat-value">{{ $num(monitoring.orders.total_count, 0) }}</div>
          <div class="stat-title">总订单数</div>
        </div>
        <div class="stat-footer">
          <span class="label">已成交</span>
          <span class="value">{{ $num(monitoring.orders.filled_count, 0) }}</span>
        </div>
      </div>

      <div class="stat-card">
        <div class="stat-header">
          <div class="stat-icon trades">
            <i class="el-icon-s-data"></i>
          </div>
          <!-- ❌ 已删除写死的趋势徽章「15%」@yutiansut @quantaxis
               它是字面量,不来自任何接口 —— 而旁边的 stat-value 是**真数据**,
               一个真数字配一个假涨幅,比纯假更误导(用户会以为整块都是实测)。
               要恢复需后端提供环比数据(当前 /api/monitoring/* 都没有)。 -->
        </div>
        <div class="stat-body">
          <div class="stat-value">{{ $num(monitoring.trades.total_count, 0) }}</div>
          <div class="stat-title">总成交数</div>
        </div>
        <div class="stat-footer">
          <span class="label">成交量</span>
          <span class="value">{{ $num(monitoring.trades.total_volume, 0) }}</span>
        </div>
      </div>

      <div class="stat-card">
        <div class="stat-header">
          <div class="stat-icon storage">
            <i class="el-icon-coin"></i>
          </div>
          <div class="stat-trend normal">
            <span>{{ $num(monitoring.storage.oltp.total_batches, 0) }} 批次</span>
          </div>
        </div>
        <div class="stat-body">
          <div class="stat-value">{{ formatLargeNumber(monitoring.storage.oltp.total_records) }}</div>
          <div class="stat-title">存储记录</div>
        </div>
        <div class="stat-footer">
          <span class="label">WAL 文件</span>
          <span class="value">{{ $num(monitoring.storage.oltp.total_batches, 0) }}</span>
        </div>
      </div>
    </div>

    <!-- 图表区域 -->
    <div class="charts-section">
      <div class="chart-row">
        <div class="chart-card">
          <div class="chart-header">
            <h3>账户资金分布</h3>
            <el-radio-group v-model="balanceChartType" size="mini">
              <el-radio-button label="pie">饼图</el-radio-button>
              <el-radio-button label="bar">柱图</el-radio-button>
            </el-radio-group>
          </div>
          <div class="chart-body">
            <div id="balanceChart" class="chart"></div>
          </div>
        </div>

        <div class="chart-card">
          <div class="chart-header">
            <h3>订单状态分布</h3>
            <div class="chart-legend">
              <span class="legend-item pending">待成交</span>
              <span class="legend-item filled">已成交</span>
              <span class="legend-item cancelled">已撤销</span>
            </div>
          </div>
          <div class="chart-body">
            <div id="orderChart" class="chart"></div>
          </div>
        </div>
      </div>

      <div class="chart-row full">
        <div class="chart-card wide">
          <div class="chart-header">
            <h3>OLAP 转换任务监控</h3>
            <div class="task-stats">
              <span class="task-stat">
                <i class="dot pending"></i>
                待转换: {{ $num(monitoring.storage.olap.pending_tasks, 0) }}
              </span>
              <span class="task-stat">
                <i class="dot converting"></i>
                转换中: {{ $num(monitoring.storage.olap.converting_tasks, 0) }}
              </span>
              <span class="task-stat">
                <i class="dot success"></i>
                成功: {{ $num(monitoring.storage.olap.success_tasks, 0) }}
              </span>
              <span class="task-stat">
                <i class="dot failed"></i>
                失败: {{ $num(monitoring.storage.olap.failed_tasks, 0) }}
              </span>
            </div>
          </div>
          <div class="chart-body">
            <div id="olapChart" class="chart"></div>
          </div>
        </div>
      </div>
    </div>

    <!-- 系统状态卡片 -->
    <div class="system-status">
      <div class="status-card">
        <!-- ⚠️ 图标也必须跟随真实状态 @yutiansut @quantaxis
             原来是写死的 `healthy` + `el-icon-success`(绿色对勾),
             于是接口 500 时**值显示 `--`、旁边仍是绿色对勾** ——
             与本轮删掉的假趋势徽章是同一类「假的确定感」,只是换了个通道。
             回验 agent 有截图佐证(a3c_fail_cards.png)。 -->
        <div class="status-icon" :class="systemStatusClass">
          <i :class="systemStatusIcon"></i>
        </div>
        <div class="status-info">
          <div class="status-title">系统状态</div>
          <!-- 用接口返回的 status,不写死。@yutiansut @quantaxis -->
          <div class="status-value">{{ systemStatusText }}</div>
        </div>
      </div>
      <!-- 另外三张卡:值取不到时图标灰化(复用上面已有的 .unknown),但**保留语义图标**。
           图标表达的是「这是哪个指标」,换成问号会丢信息;真正误导的是取不到值时
           图标仍是彩色 —— 那与本轮删掉的绿色对勾是同一类「假的确定感」。
           每张卡判自己的值,不共用系统状态:后端可能只缺其中某个字段。
           @yutiansut @quantaxis -->
      <div class="status-card">
        <div class="status-icon" :class="{ unknown: uptimeText === '--' }">
          <i class="el-icon-time"></i>
        </div>
        <div class="status-info">
          <div class="status-title">运行时间</div>
          <div class="status-value">{{ uptimeText }}</div>
        </div>
      </div>
      <div class="status-card">
        <div class="status-icon" :class="{ unknown: matchingLatencyText === '--' }">
          <i class="el-icon-cpu"></i>
        </div>
        <div class="status-info">
          <div class="status-title">撮合延迟</div>
          <!-- 用接口返回的 matching_latency_p99。@yutiansut @quantaxis
               ⚠️ 后端该字段原本也是硬编码 "P99 < 100μs"(monitoring.rs:537),
               无任何测量支撑 —— 已同步改为「未采集」。前后端各编一次。 -->
          <div class="status-value">{{ matchingLatencyText }}</div>
        </div>
      </div>
      <div class="status-card">
        <div class="status-icon" :class="{ unknown: wsConnectionsText === '--' }">
          <i class="el-icon-connection"></i>
        </div>
        <div class="status-info">
          <div class="status-title">WebSocket</div>
          <div class="status-value">{{ wsConnectionsText }}</div>
        </div>
      </div>
    </div>
  </div>
</template>

<script>
// @yutiansut @quantaxis - 系统监控仪表盘
import { mapGetters, mapActions } from 'vuex'

export default {
  name: 'Dashboard',
  data() {
    return {
      // ⚠️ 不再自己存 sysStatus —— 改从 store 读(layout 的 15 秒健康检查已经在取)
      //    @yutiansut @quantaxis
      // 见 computed 里的 mapGetters。此前本页独立请求同一接口,
      // 与 layout 各打一次,整页加载 2 次、首页稳态 5 次/分钟。
      balanceChartType: 'pie',
      // uptime / wsConnections 已改为从 sysStatus 派生(见 computed),不再单独存
      charts: {},
      resizeRaf: null,
      statusLoading: false
    }
  },
  computed: {
    ...mapGetters(['monitoring', 'marginUtilization', 'marginUtilizationText', 'marginUtilizationWidth', 'sysStatus']),
    // 取不到就显示「--」,**不编** @yutiansut @quantaxis
    systemStatusText () {
      const st = this.sysStatus && this.sysStatus.status
      if (!st) return '--'
      return st === 'running' ? '运行正常' : st
    },
    matchingLatencyText () {
      const v = this.sysStatus && this.sysStatus.matching_latency_p99
      return v || '--'
    },
    uptimeText () {
      return (this.sysStatus && this.sysStatus.uptime_display) || '--'
    },
    // 图标随真实状态变 —— 取不到时用「未知」而非绿色对勾。@yutiansut @quantaxis
    systemStatusClass () {
      const st = this.sysStatus && this.sysStatus.status
      if (!st) return 'unknown'
      return st === 'running' ? 'healthy' : 'warning'
    },
    systemStatusIcon () {
      const st = this.sysStatus && this.sysStatus.status
      if (!st) return 'el-icon-question'
      return st === 'running' ? 'el-icon-success' : 'el-icon-warning'
    },
    wsConnectionsText () {
      const n = this.sysStatus && this.sysStatus.ws_connections
      return (n == null || !Number.isFinite(Number(n))) ? '--' : `${n} 连接`
    }
  },
  watch: {
    balanceChartType() {
      this.$nextTick(() => {
        this.initBalanceChart()
      })
    }
  },
  mounted() {
    this.startAutoRefresh()
    this.$nextTick(() => {
      this.initCharts()
    })
    // ⚠️ 本页**不再**自己取系统状态 —— layout 的 15 秒健康检查已在取并存进 store,
    //    本页 computed 直接读 `sysStatus`。此前两处各自请求,整页加载 2 次。
    //    @yutiansut @quantaxis
    // ✨ 本页 3 个 ECharts 实例此前完全没有 resize 处理 @yutiansut @quantaxis
    // 实测(headless Chrome 141): 1920 宽加载后把视口改到 1024, 三张 canvas
    // 仍然是 786/786/1626px, 一动不动; 1440 下折叠侧边栏, .main-container
    // 由 1220px 变 1376px, canvas 仍是 546/546/1146 —— 右侧凭空多出 156px 空白。
    // .chart-row 在 768 断点下改成单列(见本文件 @media), 但 canvas 不 resize
    // 的话那条媒体查询等于白写。
    window.addEventListener('resize', this.onWindowResize)
  },
  beforeDestroy() {
    this.stopAutoRefresh()
    window.removeEventListener('resize', this.onWindowResize)
    if (this.resizeRaf) {
      window.cancelAnimationFrame(this.resizeRaf)
      this.resizeRaf = null
    }
    Object.values(this.charts).forEach(chart => chart && chart.dispose())
  },
  methods: {
    ...mapActions(['startAutoRefresh', 'stopAutoRefresh']),

    // ✨ 委托给全局 `$money`(main.js)—— 判空 → isFinite → **量级上限 1e15**。
    // @yutiansut @quantaxis
    //
    // 本页此前是自己写的格式化器,挡不住 `balance = 1e300` 的污染账户:
    // 子代理实测它在这里渲染成 404 个字符(或 `¥1e+300`)。
    // `Number.isFinite(1e300)` 为真,所以只判 isFinite 也不够,必须判量级。
    // 全站四个各写各的格式化器现已统一到同一个判据。
    formatNumber(num) {
      return this.$money(num)
    },

    // 饼图数据:**剔除**不可用的扇区(非有限 / 量级超限 / 非正数),不归 0。
    // 归 0 会让扇区消失而剩余扇区变成 100%,那是另一种错(见上方说明)。
    // 阈值与 `$money` 一致(1e15)。@yutiansut @quantaxis
    // 柱图数据:不可用项置 `null`(ECharts 渲染**缺口**);
    // **不能剔除** —— x 轴三个分类是固定的,剔除会打乱 `colors[i]` 的下标对应。
    // 与 `fundPieState` 同阈值,只是「不可用」的表示不同(它剔除,这里置 null)。
    // @yutiansut @quantaxis
    fundBarData(colors) {
      // 未加载时三根柱全给 `null`(缺口)—— 零高柱会被读成「确实是 0」。
      // 与 `fundPieState` 同一个第三态判据。@yutiansut @quantaxis
      if (!this.$store.state.monitoringLoaded) {
        return [0, 1, 2].map(i => ({ value: null, itemStyle: { color: colors[i] } }))
      }
      const a = (this.monitoring && this.monitoring.accounts) || {}
      return [a.total_balance, a.total_available, a.total_margin_used].map((v, i) => {
        const n = Number(v)
        const ok = Number.isFinite(n) && Math.abs(n) < 1e15
        return { value: ok ? n : null, itemStyle: { color: colors[i] } }
      })
    },

    // 饼图状态:可画的项 + 被剔除的项数。**init 与 watch 共用**,否则 title 会留存。
    // @yutiansut @quantaxis
    //
    // ⚠️ 两个此前踩过的坑,都写在这里:
    //   ① `title` 原先只在初始 option 里设,而 `$watch` 只 `setOption({series})` ——
    //      ECharts 会**合并**选项,于是 title 原地留存。`initBalanceChart()` 跑在
    //      `monitoring.accounts` 还是空对象时,文案固定成「3 项」,之后再也不更新:
    //      **数据完全正常也会弹「资金数据不可用」压在画对了的饼图正中间**。
    //      这是竞态 —— 数据到得早就显示对的数字,到得晚就永远是 3。
    //      ⇒ title 必须与 series **同一次** setOption,并在无剔除时显式清空。
    //   ② 判据原先是 `n > 0`,把**合法的 0** 当成不可显示:
    //      `100/60/0` 是一张完全画得出来的饼图(62.5%/37.5%),却被整张挡掉。
    //      **0 不是「超出范围」,它就是 0**。只有非有限、负数、超限才不可画。
    fundPieState() {
      // ✨ 第三态:尚未加载。@yutiansut @quantaxis
      //
      // `monitoring.accounts` 初值是三个 `0`,而我为放行**合法的零**把判据从
      // `n > 0` 放宽到 `n >= 0` 之后,这三个 0 就成了三个「合法值」——
      // 子代理实测:首屏画出一张 **33/33/33 的等分资金分布**,完全虚构。
      // 「还没加载」既不是可显示、也不是超出范围,必须单列。
      if (!this.$store.state.monitoringLoaded) return { items: [], dropped: 0, loading: true }
      const a = (this.monitoring && this.monitoring.accounts) || {}
      const src = [
        { value: a.total_balance, name: '总权益' },
        { value: a.total_available, name: '可用资金' },
        { value: a.total_margin_used, name: '保证金' }
      ]
      const items = []
      let dropped = 0
      src.forEach(it => {
        const n = Number(it.value)
        if (Number.isFinite(n) && n >= 0 && n < 1e15) items.push({ value: n, name: it.name })
        else dropped++
      })
      // ✨ 第四态:已加载、无剔除,但**总和为 0**。@yutiansut @quantaxis
      //
      // ECharts 对 `[0,0,0]` 按**等分**渲染 —— 又是那张 33/33/33 的虚构分布,
      // 只是触发条件从「未加载」换成了「真实全零」。我上一版只堵住了前者。
      // 对照 `100/60/0`:总和非零,ECharts 正常按比例缩放,零值扇区自然消失
      // —— 所以**只有总和为 0 时才发作**。
      // 而这不是假想场景:一个刚启动、还没有任何入金的交易所就是 0/0/0。
      const sum = items.reduce((t, it) => t + it.value, 0)
      // ✨ 扇区百分比用**最大余数法**配平到恰好 100。@yutiansut @quantaxis
      //
      // 直接 `percent.toFixed(0)` 会各自独立进位:100/60/0 → 63%+38% = **101%**,
      // 200/50/50 → 67%+17%+17% = **101%**。同一屏上三个数加起来不等于 100,
      // 正是我们一直在追的「同屏数字互相矛盾」那一类 —— 这条是我改整数化时引入的。
      // 保留一位小数并不能根治(66.7+16.7+16.7 = 100.1),配平才行。
      //
      // 配平结果写进**数据项自身**而不是 formatter 闭包:`series.data` 每次更新
      // 都会重发,`p.data.pct` 必然新鲜;而 watch 里**不重发 `label`**,
      // 闭包会停在 init 那一批 items 上(L153 同类 bug 的第六处)。
      if (sum > 0) {
        const raw = items.map(it => (it.value / sum) * 100)
        const pct = raw.map(Math.floor)
        const short = 100 - pct.reduce((t, n) => t + n, 0)
        // 余数大者优先补 1;零值项小数部分为 0,排在最后,不会被补
        const order = raw
          .map((v, i) => ({ i, frac: v - Math.floor(v) }))
          .sort((a, b) => b.frac - a.frac)
        for (let k = 0; k < short && k < order.length; k++) pct[order[k].i] += 1
        items.forEach((it, i) => { it.pct = pct[i] })
      } else {
        items.forEach(it => { it.pct = 0 })
      }
      return { items, dropped, empty: items.length > 0 && sum === 0 }
    },

    // 饼图的 title 选项:无剔除时**显式清空**(不能省略,省略等于沿用旧值)
    // 图例配置(含金额 formatter)—— **init 与 watch 必须共用**。@yutiansut @quantaxis
    //
    // formatter 是个闭包,捕获了当时的 `pie.items`。若 watch 只发
    // `legend: { show }`,ECharts 会保留旧的 formatter ⇒ **图例里的金额停在旧值**。
    // 这已经是同一个 bug 类的第五处(前四处:饼图 title、柱图 title、
    // 柱图坐标轴显隐、饼图 legend.show),判据见 lessons L153:
    // **凡在 init 里依赖状态算出来的属性,每一处增量更新都必须重发。**
    fundLegend(pie) {
      return {
        show: !(pie.loading || pie.dropped || pie.empty),
        bottom: 2,
        itemGap: 14,
        itemWidth: 10,
        itemHeight: 10,
        textStyle: { fontSize: 11, color: '#606266' },
        formatter: name => {
          const it = pie.items.find(x => x.name === name)
          return it ? `${name} ${this.$money(it.value)}` : name
        }
      }
    },

    // 两种图型共用的状态文案入口:传入已算好的 state 则复用,否则自己算一次。
    // @yutiansut @quantaxis
    chartStateTitle(pie) {
      const st = pie || this.fundPieState()
      return this.fundPieTitle(st.dropped, st.loading, st.empty)
    },

    // 三种状态的文案;样式统一走全局 `$chartTitle`(main.js),
    // 与 `/admin-settlement` 的空态共用一份 —— 原先这里把
    // `left/top/textStyle/subtextStyle` 抄了三遍。@yutiansut @quantaxis
    // 传 null 时 `$chartTitle` 返回 `{ text: '', subtext: '' }` 而非 `{}`,
    // 因为 setOption 是合并语义,省略键会沿用上一次的文案(L153)。
    fundPieTitle(dropped, loading, empty) {
      if (empty) return this.$chartTitle('暂无资金', '总权益、可用资金与保证金均为 0')
      if (loading) return this.$chartTitle('加载中', '正在获取资金数据')
      if (!dropped) return this.$chartTitle(null)
      return this.$chartTitle('资金数据不可用', `${dropped} 项超出可显示范围`)
    },


    formatLargeNumber(num) {
      if (num >= 1000000) {
        return (num / 1000000).toFixed(1) + 'M'
      } else if (num >= 1000) {
        return (num / 1000).toFixed(1) + 'K'
      }
      return num || 0
    },

    initCharts() {
      this.initBalanceChart()
      this.initOrderChart()
      this.initOlapChart()
    },

    // ✨ 用 rAF 合并 resize 回调 @yutiansut @quantaxis
    onWindowResize() {
      if (this.resizeRaf) return
      this.resizeRaf = window.requestAnimationFrame(() => {
        this.resizeRaf = null
        this.resizeCharts()
      })
    },

    resizeCharts() {
      Object.values(this.charts).forEach(chart => chart && chart.resize())
    },

    initBalanceChart() {
      if (this.charts.balance) {
        this.charts.balance.dispose()
      }

      const chart = this.$echarts.init(document.getElementById('balanceChart'))
      this.charts.balance = chart

      const colors = ['#1890ff', '#52c41a', '#faad14']

      let option
      if (this.balanceChartType === 'pie') {
        // ✨ 有项被剔除时**不画图**,直接说明原因。@yutiansut @quantaxis
        //
        // 子代理实测:上一版只剔除不兜底,三项里只剩「保证金」一项时,
        // ECharts 对单个数据点**就是画满环** —— 渲染出一个无标签、无图例、
        // 无百分比的纯色整环,读者无从判断它代表什么、占多少。
        // 更糟的是数组只剩一项时它拿**调色板 0 号色(蓝)**,而蓝色在同页柱图里
        // 代表「总权益」,这个扇区实际是「保证金」—— **颜色指向了错误的类别**。
        // ⇒ 单扇区饼图不是「信息少」,是「误导」。不完整就不画。
        const pie = this.fundPieState()
        option = {
          title: this.chartStateTitle(pie),
          tooltip: {
            trigger: 'item',
            formatter: '{b}: ¥{c} ({d}%)'
          },
          legend: this.fundLegend(pie),
          color: colors,
          series: [{
            name: '账户资金',
            type: 'pie',
            radius: ['42%', '65%'],
            center: ['50%', '44%'],
            avoidLabelOverlap: true,
            itemStyle: {
              borderRadius: 8,
              borderColor: '#fff',
              borderWidth: 2
            },
            // ✨ 常驻标签 —— 与同页「订单状态分布」饼图同一形状(`initOrderChart` 的
            // `formatter: '{b}\n{d}%'`),不自造。@yutiansut @quantaxis
            //
            // 原先是 `label: { show: false }`,只在 hover 时才显示名称。
            // 子代理连续三轮指出:正因为没有常驻标签,**退化态只能靠中心文字兜底** ——
            // 单扇区时它渲染成一个无标签、无图例、无百分比的纯色环,
            // 读者无从判断它代表什么、占多少;而且单项时 ECharts 取调色板 0 号色,
            // 颜色还会指向错误的类别。有了标签,少一项/只剩一项本身就能自我说明。
            // ✨ 标签放**扇区内**、名称交给图例 —— 外置标签在窄画布上会被裁字。
            // @yutiansut @quantaxis
            //
            // 子代理实测(按图表卡片精确裁切):
            //   画布 626 / 338 / 317px → 标签完整
            //   画布 **282px** → 右侧「总权益 50%」的「益」被右边界切掉,
            //                    左侧「可用资金 30%」的「可」被左边界切掉
            // `avoidLabelOverlap: true` 只管**标签之间**压叠,**不管越出画布**。
            //
            // ⚠️ 反直觉的一点:它**在手机上没事,反而在 900–1000px 的桌面宽度坏** ——
            // 375px 是单列布局、图表卡有 317px;而 900px 是两列布局,卡片被压到 282px。
            // 所以这不是「极端窄屏」问题,是常见笔记本/平板横屏区间。
            //
            // 内置标签永远不会溢出;名称由图例承载,信息量不减。
            // Z1:零值扇区**不显示标签**。@yutiansut @quantaxis
            //   子代理实测:零值没有弧长,那个 `0%` 会贴在圆环上、落在相邻两段的交界处,
            //   **不属于任何可见弧段** —— 读者会把它读成旁边那一片的数值。
            //   它自己的评语很准:「比两个极端都差」。类别信息不会丢 ——
            //   图例里「保证金」仍在,且下面的 formatter 会把金额写进图例项。
            //
            // Z2:标签用**深色**而不是白色。子代理算的白字对比度
            //   3.2 / 2.3 / **1.9**:1(橙色最差),三个都低于 WCAG AA 的 4.5:1;
            //   换成 `#262626` 后是 4.7 / 6.7 / 8.0:1,三个都达标。
            label: {
              position: 'inside',
              // 取 `p.data.pct`(配平后的整数,见 fundPieState)而非 `p.percent`
              // —— 后者各扇区独立进位,三数之和会变成 101%。
              formatter: p => (p.value > 0 ? `${p.data && p.data.pct != null ? p.data.pct : Math.round(p.percent)}%` : ''),
              fontSize: 11,
              color: '#262626',
              fontWeight: 500
            },
            labelLine: { show: false },
            emphasis: {
              label: {
                show: true,
                fontSize: 14,
                fontWeight: 'bold'
              }
            },
            // ✨ 占比图里不可用的扇区必须**剔除**,不能归 0。@yutiansut @quantaxis
            //
            // 我上一版用 `chartSafe` 把 `1e300` 归 0,子代理实测的后果是:
            // 总权益与可用资金两个扇区**直接消失**,只剩保证金一个扇区占 **100%**
            // —— 饼图变成一个纯色整环,在无声地断言「所有资金都是保证金」。
            // 从「其余扇区全 0%」换成了「单扇区 100%」,**换了个样子错**。
            //
            // 根本原因:**占比图里 0 的含义是「确实没有」,不是「未知」**。
            // 求和场景可以归 0,占比场景不行。改为剔除,并在全部不可用时
            // 显式说明(见下方 `title`),而不是画一张看似正常的图。
            data: (pie.loading || pie.dropped || pie.empty) ? [] : pie.items
          }]
        }
      } else {
        const barSt = this.fundPieState()
        const barHasState = !!(barSt.loading || barSt.dropped || barSt.empty)
        option = {
          // ✨ 柱图也要能自我说明 —— 此前加载期只有一个空框 + x 轴标签,
          // 没有任何文字,慢网下停在柱图标签页的用户看到的是一张无解释的空图。
          // 与饼图共用同一套状态判据。@yutiansut @quantaxis
          title: this.chartStateTitle(),
          tooltip: {
            trigger: 'axis',
            formatter: '{b}: ¥{c}'
          },
          color: colors,
          grid: {
            left: '3%',
            right: '4%',
            bottom: '3%',
            top: '10%',
            containLabel: true
          },
          // ✨ 有状态文案时关掉坐标轴与网格。@yutiansut @quantaxis
          // 子代理实测:文案定位在图表正中、落在绘图区里,
          // **一条虚线网格线正好从副标题上横穿而过**,可读性明显变差。
          // 居中文字在甜甜圈空心里合适,放在带网格的直角坐标系上就不合适。
          // 这三种状态本就无数据可画,坐标轴留着只是干扰。
          xAxis: {
            show: !barHasState,
            type: 'category',
            data: ['总权益', '可用资金', '保证金'],
            axisLine: { lineStyle: { color: '#e4e7ed' } },
            axisLabel: { color: '#606266' }
          },
          yAxis: {
            show: !barHasState,
            type: 'value',
            axisLine: { show: false },
            splitLine: { lineStyle: { color: '#e4e7ed', type: 'dashed' } },
            axisLabel: { color: '#909399' }
          },
          series: [{
            type: 'bar',
            // ✨ 与 `$watch` 回调共用同一个构造 —— 此前**只有 watch 那处有闸**,
            // 而切换图型会重跑 `initBalanceChart()` 走到这里,于是修复看不见:
            // 子代理截图实测 y 轴直接印出 `1e+300`,真实的保证金柱(8.97 亿)
            // 在那个量纲下是 1e-291,**根本画不出来**。@yutiansut @quantaxis
            data: this.fundBarData(colors),
            barWidth: '40%',
            itemStyle: {
              borderRadius: [4, 4, 0, 0]
            }
          }]
        }
      }
      chart.setOption(option)

      // ✨ 饼图与柱图对「不可用」的正确表示**不同**,不能共用一份 data。
      // @yutiansut @quantaxis
      //   · 饼图:必须**剔除**扇区。归 0 会让它消失而剩余扇区变成 100%,
      //     图表就在无声地断言一个错误的分布(子代理实测:纯色整环、保证金 100%)。
      //   · 柱图:不能剔除(会打乱 x 轴三个固定分类与 `colors[i]` 的下标对应),
      //     用 `null` —— ECharts 对 null 渲染**缺口**,而 0 会渲染成一根零高柱,
      //     那同样是「说它是 0」而不是「说它未知」。
      this.$watch('monitoring.accounts', () => {
        if (this.balanceChartType === 'pie') {
          // ✨ title 必须与 series **同一次**下发,否则会留存上一次的文案(见 fundPieState 说明)
          // **legend 也要同批下发** —— 它的 `show` 现在随状态变化。
          // 这是同一个 bug 类的第四处(前三处:饼图 title、柱图 title、柱图坐标轴)。
          // 判据很简单:**凡是在 init 里依赖状态计算出来的属性,watch 都必须重发**。
          // @yutiansut @quantaxis
          const pie = this.fundPieState()
          const hidePie = !!(pie.loading || pie.dropped || pie.empty)
          chart.setOption({
            title: this.chartStateTitle(pie),
            legend: this.fundLegend(pie),
            series: [{ data: hidePie ? [] : pie.items }]
          })
        } else {
          // title **与坐标轴显隐**都必须与 series 同批下发 —— 任何只在 init 设一次的
          // 属性都会停在初始值。这已经是同一个 bug 类的第三次:
          //   N1 饼图 title 只在 init 设 → 数据正常也弹「不可用」
          //   柱图 title 只在 init 设   → 切过去后文案不更新
          //   本次:坐标轴显隐只在 init 设 → 状态消失后坐标轴回不来
          // @yutiansut @quantaxis
          const barSt2 = this.fundPieState()
          const hasState2 = !!(barSt2.loading || barSt2.dropped || barSt2.empty)
          chart.setOption({
            title: this.chartStateTitle(barSt2),
            xAxis: { show: !hasState2 },
            yAxis: { show: !hasState2 },
            series: [{ data: this.fundBarData(colors) }]
          })
        }
      }, { deep: true })
    },

    initOrderChart() {
      const chart = this.$echarts.init(document.getElementById('orderChart'))
      this.charts.order = chart

      const option = {
        tooltip: { trigger: 'item' },
        color: ['#faad14', '#52c41a', '#909399'],
        series: [{
          name: '订单状态',
          type: 'pie',
          radius: '70%',
          center: ['50%', '50%'],
          data: [
            { value: this.monitoring.orders.pending_count, name: '待成交' },
            { value: this.monitoring.orders.filled_count, name: '已成交' },
            { value: this.monitoring.orders.cancelled_count, name: '已撤销' }
          ],
          itemStyle: {
            borderRadius: 6,
            borderColor: '#fff',
            borderWidth: 2
          },
          label: {
            formatter: '{b}\n{d}%',
            fontSize: 12
          },
          emphasis: {
            itemStyle: {
              shadowBlur: 10,
              shadowOffsetX: 0,
              shadowColor: 'rgba(0, 0, 0, 0.2)'
            }
          }
        }]
      }
      chart.setOption(option)

      this.$watch('monitoring.orders', () => {
        chart.setOption({
          series: [{
            data: [
              { value: this.monitoring.orders.pending_count, name: '待成交' },
              { value: this.monitoring.orders.filled_count, name: '已成交' },
              { value: this.monitoring.orders.cancelled_count, name: '已撤销' }
            ]
          }]
        })
      }, { deep: true })
    },

    initOlapChart() {
      const chart = this.$echarts.init(document.getElementById('olapChart'))
      this.charts.olap = chart

      const option = {
        tooltip: {
          trigger: 'axis',
          axisPointer: { type: 'shadow' }
        },
        grid: {
          left: '3%',
          right: '4%',
          bottom: '3%',
          top: '10%',
          containLabel: true
        },
        xAxis: {
          type: 'value',
          axisLine: { show: false },
          splitLine: { lineStyle: { color: '#e4e7ed', type: 'dashed' } }
        },
        yAxis: {
          type: 'category',
          data: ['任务状态'],
          axisLine: { lineStyle: { color: '#e4e7ed' } }
        },
        series: [
          {
            name: '待转换',
            type: 'bar',
            stack: 'total',
            data: [this.monitoring.storage.olap.pending_tasks],
            itemStyle: { color: '#faad14', borderRadius: [4, 0, 0, 4] }
          },
          {
            name: '转换中',
            type: 'bar',
            stack: 'total',
            data: [this.monitoring.storage.olap.converting_tasks],
            itemStyle: { color: '#1890ff' }
          },
          {
            name: '成功',
            type: 'bar',
            stack: 'total',
            data: [this.monitoring.storage.olap.success_tasks],
            itemStyle: { color: '#52c41a' }
          },
          {
            name: '失败',
            type: 'bar',
            stack: 'total',
            data: [this.monitoring.storage.olap.failed_tasks],
            itemStyle: { color: '#f5222d', borderRadius: [0, 4, 4, 0] }
          }
        ]
      }
      chart.setOption(option)

      this.$watch('monitoring.storage.olap', () => {
        chart.setOption({
          series: [
            { data: [this.monitoring.storage.olap.pending_tasks] },
            { data: [this.monitoring.storage.olap.converting_tasks] },
            { data: [this.monitoring.storage.olap.success_tasks] },
            { data: [this.monitoring.storage.olap.failed_tasks] }
          ]
        })
      }, { deep: true })
    }
  }
}
</script>

<style lang="scss" scoped>
// @yutiansut @quantaxis - 深色主题仪表盘样式
$dark-bg-primary: #0d1117;
$dark-bg-secondary: #161b22;
$dark-bg-card: #1c2128;
$dark-bg-tertiary: #21262d;
$dark-border: #30363d;
$dark-text-primary: #f0f6fc;
$dark-text-secondary: #8b949e;
$dark-text-muted: #6e7681;
$primary-color: #1890ff;
$success-color: #52c41a;
$warning-color: #faad14;
$danger-color: #f5222d;

.dashboard {
  padding: 0;
  background: $dark-bg-primary;
  min-height: 100%;
}

// 统计卡片
.stat-cards {
  display: grid;
  // 240px 在 1366 屏下排 4 列后每列仅剩 ~217px,装不下 17 位金额。
  // 放宽到 260 让 auto-fit 在窄屏自动降到 3 列,每列 ~300px。
  /* 260px 在 1920 下会排到 5 列(每列 314px),而字号 1.4vw 已涨到 26.88px,
     `¥16,172,531,333.19` 需要 291px > 272px 可用宽 —— **最宽的视口反而截断**。
     字号随视口线性增长快过 auto-fit 加列后每列变宽的速度。@yutiansut @quantaxis */
  grid-template-columns: repeat(auto-fit, minmax(320px, 1fr));
  gap: 20px;
  margin-bottom: 24px;
}

.stat-card {
  background: $dark-bg-card !important;
  border: 1px solid $dark-border;
  border-radius: 12px;
  padding: 20px;
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.3);
  transition: all 0.3s ease;

  &:hover {
    transform: translateY(-4px);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
    border-color: rgba($primary-color, 0.3);
  }

  .stat-header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    margin-bottom: 16px;
  }

  .stat-icon {
    width: 48px;
    height: 48px;
    border-radius: 12px;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 24px;
    color: white;

    &.accounts { background: linear-gradient(135deg, #667eea 0%, #764ba2 100%); }
    &.balance { background: linear-gradient(135deg, #f093fb 0%, #f5576c 100%); }
    &.margin { background: linear-gradient(135deg, #4facfe 0%, #00f2fe 100%); }
    &.orders { background: linear-gradient(135deg, #43e97b 0%, #38f9d7 100%); }
    &.trades { background: linear-gradient(135deg, #fa709a 0%, #fee140 100%); }
    &.storage { background: linear-gradient(135deg, #30cfd0 0%, #330867 100%); }
  }

  .stat-trend {
    font-size: 12px;
    padding: 4px 8px;
    border-radius: 4px;
    display: flex;
    align-items: center;
    gap: 4px;

    &.up {
      background: rgba($success-color, 0.15);
      color: $success-color;
    }
    &.down {
      background: rgba($danger-color, 0.15);
      color: $danger-color;
    }
    &.warning {
      background: rgba($warning-color, 0.15);
      color: $warning-color;
    }
    &.normal {
      background: rgba($dark-text-muted, 0.15);
      color: $dark-text-secondary;
    }
  }

  .stat-body {
    margin-bottom: 12px;

    .stat-value {
      // 金额位数会长到 ¥45,417,766,890.32(17 字符),28px 等宽字体需要 ~303px,
      // 而 1366 屏下卡片只有 ~217px → 数字被裁掉小数位,读成另一个数。
      // 用 clamp 随视口缩放:1366 屏约 18px(装得下),1920 屏回到 26px(该有的层次)。
      // 保留完整数字,不做亿/万缩写 —— 交易系统里精度不能省。
      // @yutiansut @quantaxis
      font-size: clamp(16px, 1.4vw, 22px);
      font-weight: 700;
      color: $dark-text-primary !important;
      line-height: 1.2;
      font-family: 'JetBrains Mono', monospace;
      min-width: 0;                 // 允许在栅格/flex 里收缩
      overflow: hidden;             // 万一还是装不下,给省略号而不是硬裁
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .stat-title {
      font-size: 14px;
      color: $dark-text-secondary !important;
      margin-top: 4px;
    }
  }

  .stat-footer {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding-top: 12px;
    border-top: 1px solid $dark-border;

    .label {
      font-size: 12px;
      color: $dark-text-muted !important;
    }

    .value {
      font-size: 13px;
      font-weight: 600;
      color: $dark-text-secondary !important;
      font-family: 'JetBrains Mono', monospace;
    }

    .progress-bar {
      flex: 1;
      height: 6px;
      background: $dark-bg-tertiary;
      border-radius: 3px;
      overflow: hidden;

      .progress {
        height: 100%;
        background: linear-gradient(90deg, $primary-color, #40a9ff);
        border-radius: 3px;
        transition: width 0.5s ease;
      }
    }
  }
}

// 图表区域
.charts-section {
  margin-bottom: 24px;
}

.chart-row {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: 20px;
  margin-bottom: 20px;

  &.full {
    grid-template-columns: 1fr;
  }
}

.chart-card {
  background: $dark-bg-card !important;
  border: 1px solid $dark-border;
  border-radius: 12px;
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.3);
  overflow: hidden;

  &.wide {
    grid-column: 1 / -1;
  }

  .chart-header {
    padding: 16px 20px;
    border-bottom: 1px solid $dark-border;
    display: flex;
    justify-content: space-between;
    align-items: center;
    background: $dark-bg-secondary !important;

    h3 {
      margin: 0;
      font-size: 16px;
      font-weight: 600;
      color: $dark-text-primary !important;
    }

    .chart-legend {
      display: flex;
      gap: 16px;

      .legend-item {
        font-size: 12px;
        color: $dark-text-secondary !important;

        &::before {
          content: '';
          display: inline-block;
          width: 8px;
          height: 8px;
          border-radius: 50%;
          margin-right: 6px;
        }

        &.pending::before { background: $warning-color; }
        &.filled::before { background: $success-color; }
        &.cancelled::before { background: $dark-text-muted; }
      }
    }

    .task-stats {
      display: flex;
      gap: 16px;

      .task-stat {
        font-size: 12px;
        color: $dark-text-secondary !important;
        display: flex;
        align-items: center;
        gap: 6px;

        .dot {
          width: 8px;
          height: 8px;
          border-radius: 50%;

          &.pending { background: $warning-color; }
          &.converting { background: $primary-color; }
          &.success { background: $success-color; }
          &.failed { background: $danger-color; }
        }
      }
    }
  }

  .chart-body {
    padding: 16px;
    background: $dark-bg-card !important;

    .chart {
      height: 280px;
    }
  }
}

// 单选按钮组 - 深色主题
::v-deep .el-radio-group {
  .el-radio-button__inner {
    background: $dark-bg-tertiary !important;
    border-color: $dark-border !important;
    color: $dark-text-secondary !important;
  }

  .el-radio-button__orig-radio:checked + .el-radio-button__inner {
    background: $primary-color !important;
    border-color: $primary-color !important;
    color: white !important;
  }
}

// 系统状态
.system-status {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
  gap: 16px;
}

.status-card {
  background: $dark-bg-card !important;
  border: 1px solid $dark-border;
  border-radius: 12px;
  padding: 16px 20px;
  display: flex;
  align-items: center;
  gap: 16px;
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.3);

  .status-icon {
    width: 44px;
    height: 44px;
    border-radius: 10px;
    background: rgba($primary-color, 0.15);
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 20px;
    color: $primary-color;

    &.healthy {
      background: rgba($success-color, 0.15);
      color: $success-color;
    }

    // 状态非 running 时用警告色;取不到状态时用中性灰。@yutiansut @quantaxis
    // 不加这两个类的话,`:class` 绑上去也没样式,图标会退回默认蓝 ——
    // 那仍然是「看起来一切正常」。
    &.warning {
      background: rgba(#e6a23c, 0.15);
      color: #e6a23c;
    }

    &.unknown {
      background: rgba(#909399, 0.15);
      color: #909399;
    }
  }

  .status-info {
    .status-title {
      font-size: 12px;
      color: $dark-text-muted !important;
      margin-bottom: 4px;
    }

    .status-value {
      font-size: 15px;
      font-weight: 600;
      color: $dark-text-primary !important;
    }
  }
}

// 响应式
@media (max-width: 768px) {
  .chart-row {
    grid-template-columns: 1fr;
  }

  .stat-card .stat-body .stat-value {
    font-size: 24px;
  }
}
</style>
