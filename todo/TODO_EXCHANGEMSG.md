# Exchange 回报重构 TODO（更新：2025-11-24）

> 目标：完成“交易所内部记录 ↔ 账户回报”双通道，彻底替换旧版 `handle_filled/handle_partially_filled` 路径，并让账户依据交易所回报自行判定订单状态。

## 已完成
- [x] Phase 1：创建 `ExchangeResponse/ExchangeOrderRecord/ExchangeTradeRecord`（见 `src/exchange/exchange_types.rs`）。
- [x] Phase 2：`ExchangeIdGenerator` 为每个合约维护统一序列，并在 `OrderRouter` 中使用（`src/exchange/id_generator.rs`、`src/exchange/order_router.rs:656-812`）。
- [x] Phase 5（部分）：订单/成交/账户回报已经写入 WAL（`trade_gateway.rs:541-724`）。

## 待处理事项

### 1. TradeGateway → 账户推送链路
- [x] 在 `handle_order_accepted_new / handle_order_rejected_new / handle_trade_new / handle_cancel_*_new` 中补全推送逻辑：
  - [x] 通过 `NotificationBroker` 或回退通道将 `ExchangeResponse` 推到 `Notification::OrderStatus` / `Notification::Trade`，并同步触发 DIFF Snapshot（`src/exchange/trade_gateway.rs:560-937`）。
  - [x] 引入统一的 `emit_order_status()` / `emit_trade_notification()` 工具函数，避免在 handler 内重复序列化逻辑（`src/exchange/trade_gateway.rs:1187-1234`）。
- [ ] 落地账户侧 `ExchangeResponseRecord` replay：重启时从 `__ACCOUNT__/{user_id}` WAL 读取回报，恢复订阅端最新状态（`src/storage/recovery.rs` 仍未消费 `ExchangeResponseRecord`）。

### 2. 移除旧的账户判定逻辑
- [ ] 删除 `handle_filled/handle_partially_filled` 以及依赖 `OrderStatusNotification` 的旧代码和文档引用（旧逻辑仍保留在 `src/exchange/trade_gateway.rs:201-409`，文档示例也未更新）。
- [ ] 将所有调用方（含示例、文档、脚本）更新为只使用新的 `handle_order_*_new / handle_trade_new / handle_cancel_*_new` API（如 `docs/05_integration/diff_protocol.md` 中仍可看到旧 API）。

### 3. 账户端状态机（Phase 4）
- [ ] 账户收到 `TRADE` 回报后自行调用 `QAOrder.trade()` 更新 `volume_left`，并根据值推导 `FILLED/PARTIAL_FILLED`，不再依赖交易所推送旧状态（当前 `trade_gateway::update_account` 仍直接更新订单状态）。
- [ ] 覆盖保证金、冻结资金、风险度的刷新逻辑，确保同一个 `ExchangeResponse` 只消费一次（WAL replay 流程仍缺失去重标记）。
- [ ] 为 `OrderAccepted/CancelAccepted` 等回报补齐账户侧的待处理队列，避免重放过程中状态丢失。

### 4. 测试矩阵（Phase 7）
- [ ] 单元测试：
  - 交易所回报推送（5 种类型）是否写入 WAL 并触发通知。
  - 账户端消费回调后，`volume_left` 与风险度是否与预期一致。
- [ ] 集成测试：
  - 下单→接受→成交→账户更新→回放。
  - 撤单→接受/拒绝。
  - WAL 恢复后，账户依旧能收到未消费完的回报。

### 5. 文档/协议更新（Phase 8）
- [ ] `CLAUDE.md`：补充交易所回报机制和账户自判逻辑。
- [ ] `docs/05_integration/diff_protocol.md`、`docs/04_api/websocket/quick_start.md`：替换旧的 `handle_filled` 示例，描述 5 种回报及字段含义（目前仍引用旧 API）。
- [ ] 清理 `CHANGELOG.md`、`docs/02_architecture/system_overview.md` 等仍提及旧回报流程的章节。

> 完成以上事项后，交易所与账户间的职责划分才能彻底生效，并为后续的多副本/回放能力打下基础。

---

## 2026-11-08 hft_maker 接入(DIFF 回报 QIFI 化 + 日终结算 + CTP 订单语义)@yutiansut @quantaxis

> 协议已与 hft_maker 会话逐条确认(对照稿 v0 + 其反馈);依据 qaultraoms `docs/EIFI.md`(§4 状态机 / §5 ID / §6 去重 / §7.3 DIFF 映射)、
> QIFI 标准结构、CTP `error.xml`(LocalCTP ctp_file/current)。用户已拍板:**每个自然日做日终结算**。

### 已完成(第一期:行情)
- [x] DIFF quotes 完整快照(10 档 + 累计量额 + 昨收/涨跌停 + datetime 严格递增),订阅即补发
- [x] 实时推送:事件标脏 + 10ms 合并 + 500ms 空闲心跳,独立线程(实测 p99 10.8ms / 峰值 63 条/秒)
- [x] 主运行时去阻塞:快照落盘 spawn_blocking、KLineActor 独立 Arbiter、持仓量后台刷新

### 2a 回报 ✅ 2026-11-08 06:16 上线(端到端 7 步通过)
- [x] router:`client_order_id` 进 SubmitOrderRequest / OrderRouteInfo;索引 (account, client_id) → 交易所单号;重复 → 拒 22
- [x] DIFF insert_order 透传 order_id;下单确认改走统一的订单补丁(不再直发一份不同结构的)
- [x] gateway 补丁**同步有序**写入(去掉逐条 tokio::spawn),推入属主用户快照的 `trade.<account_id>.{orders,trades,positions,accounts}`(QIFI 切片,键翻译成客户端编号);旧的账户 ID 顶层补丁保留兼容
- [x] 状态:`status_code`(EIFI §4.1)+ QIFI `status`(ALIVE/FINISHED)+ `volume_left` + `last_msg`;FAK 零成/部成 → 5(用 volume_left 区分);FOK 未满足 → 4;拒单 → 7 + `error_id`
- [x] 顺序:成交补丁先于使订单 FINISHED 的订单补丁;`seqno` 账户内单调;trade 键 `order_id|exchange_trade_id`
- [x] 撤单:认客户端编号或 exchange_order_id;user_id 按账户归属校验;失败 notify `cancel_error{order_id,error_id,content}`(24/25/26)
- [x] 单测 + 隔离实例联调 → 通知 hft_maker 后上线

- [x] 重启后单号不变(QIFI exchange_order_id 持久化 + client_order_ids.jsonl)
- [x] 平仓冻结实测通过,回归测试已加

### 2b 日终结算(用户拍板:每个自然日)—— 2026-11-08 22:46 上线;11-09 00:00 首次生产日终通过(详见 CHANGELOG)
- [x] 每自然日北京 00:00 自动结算(`[settlement] auto_daily / time`;定时任务与手动接口共用 `run_end_of_day`)
- [x] 结算前先闭市(新单拒 4100 → CTP 91,撤单照常)+ 撤光活单(GFD 日终失效)—— 修掉「结算 → 撤单 / 成交」
      qars panic、panic=abort 整进程退出(隔离实例复现;网关两处守卫做纵深防御,单测验证去掉任一守卫即 panic)
- [x] 结算后:今仓→昨仓、清当日委托/成交(结算前快照硬链接归档到 `snapshots/history/<交易日>/`)、退冻结;
      客户端编号查重表按交易日清空、映射文件归档
- [x] 隔离实例演练(生产快照 4.9GB 复制到 /dev/shm + 跨日终压单):持仓逐户不变、冻结全退、快照 4.898GB → 4MB、重启逐户一致
- [x] 首次生产日终(11-09 00:00):闭市 69 秒、撤单 2388、408 户结算 499ms 失败 0;持仓逐户守恒(281 个账户×合约);
      仿真端新单拒「日终结算中」全在闭市窗口内,开市后照常成交
- [ ] 结算价改当日成交量加权均价(现为撮合簿最新价);quotes 增加 pre_settlement(CFFEX 涨跌停基准 = 昨结算价)
- [ ] 日 K / 当日累计量额的切日改为北京 00:00,与结算对齐 —— 会改 hft_maker 成交流特征(flow1/4/20/120)的输入,**先和对方对齐**
- [ ] 闭市时长:首次生产 69 秒 = 扫单 14 秒(2602 笔撤单逐笔走 WAL)+ 结算前落盘 4.8GiB **54 秒** + 结算 0.5 秒。
      按 11-08 全天的量(委托 436 万 / 成交 147 万)每晚仍要落 GB 级 → 根治在 E 的「状态快照 + 当日历史追加写」;扫单可改批量
- [ ] 扫单「失败」把属主已撤的也算进去:首次生产 214 笔里 175 笔是扫单期间属主自己撤了(无害),真失败是 39 笔
      OrderNotFound(见 H 恢复交叉)→ 单独计数,只把真失败记 WARN
- [ ] `snapshots/history/` 没有保留期限(每天一份结算前快照,硬链接不额外占盘,但旧 inode 会一直留着)

### 2c CTP 订单语义
- [ ] price_type:LIMIT / ANY / BEST / FIVELEVEL;time_condition / volume_condition 按交易所规则拒单(不再静默降级)
- [ ] MIN + min_volume;GTD + gtd_date 校验;contingent_condition(TOUCH / TOUCH_PROFIT / LAST|ASK1|BID1 比较)本地触发,复用条件单扫描器
- [ ] 规则矩阵按交易所/合约配置(CFFEX 市价+五档 / SHFE·INE 无市价 / DCE·CZCE 市价剩余撤;GFS/GTD/GTC 交易所层拒)
- [ ] CTP 错误码:15 / 16 / 22 / 30 / 31 / 50 / 51 / 62 / 80 / 91 / 95 / 116(撤单 24 / 25 / 26)
- [ ] CLOSE 今昨口径按交易所配置:SHFE/INE = 平昨(不足拒 51),其余 = 先昨后今
- [ ] 平仓冻结实测(CLOSE 活单冻结可平量、超量平仓拒 30)

### I/O
- [x] WAL 组提交真正生效(sync_mode=async + 全局后台刷盘 + 停机屏障),下单往返 p50 0.6ms
- [x] DIFF 待发队列有界(2000 条折叠全量快照)+ 只推给活跃会话
- [ ] 账户状态「WAL 增量 + 定期 checkpoint」(结算之后做)

### 2d 其余
- [ ] 账户当日报单数 / 撤单数(流控统计,默认只统计不拒)
- [ ] 每交易所行情发布延迟参数(默认 0)
- [ ] `GET /api/account/{id}/qifi`(键翻译成客户端编号,供逐笔对账)
- [ ] I/O:WAL 组提交 / 存储放 tmpfs —— 提给用户决定(hft_maker 建议沙盒不需要实盘级持久性)

### 2026-11-08 已完成(补记)
- [x] 同一 user_id 多条 DIFF 连接按连接扇出(06:40 上线;hft_maker 1 手联调 5 步通过)
- [x] 快照落盘互斥 / 首单新建 WAL 不在下单路径 fsync / 优雅停机 5 秒(07:24 上线)
- [x] 仿真做市 tick 口径 + 吃单流降到 1/5:生产实测 1t 52% / 2t 45% / ≥3t 3%、一档中位 3、每 500ms 均值 1.87
      (hft_maker 复测一致)。待办:往真实主力 35/40/25 再调 —— 二档做稀、补价放慢

---

## 第三期:真实柜台对齐(CTP / xlight / OES + QIFI)—— 用户定的使命 @yutiansut @quantaxis

> 用户原话:「qaexchange的目的是为了更加贴合真实交易环境 … 还需要更加扫描真实对接的交易 ctp/xlight/oes
> 同时 也要考虑我们qifi账户体系 这才是我们做qaexchangers的使命」

**依据**(2026-11-08 三路只读调研,结论均带 file:line,原文在会话记录里):
- CTP:`/home/quantaxis/ctp_test/log/`(**真实柜台日志**,broker 2176 测试环境,CTP 6.7.7)、LocalCTP
  (`/home/quantaxis/qactppro/LocalCTP`)、生产网关 `qatradegateway/open-trade-ctpsed15`(48/51 家期货公司用的是它,
  不是 EIFI 引用的 `open-trade-ctp`)
- xlight:SDK v1.7.1.7(`qabroker_xlight/cpp/include/xlt_api/`)+ 适配器 + T1/T2/生产三套环境的回调日志
- OES:SDK 0.18.8.2(`qabrokertradeoes/OesApi_0.18.8.2-20250617/`)
- 用户给的标准 QIFI 样本本身就是真实柜台数据(cyqhx),可反推真实保证金率

### A. 期货(CTP 口径)差距 —— P0:直接影响仿真结论
- [ ] **价格校验**:现在**不校验 tick 整数倍、不校验涨跌停**(registry 有 `price_tick` / `limit_up_rate`,下单路径零引用)
      → 非整 tick 拒 95「不支持的价格」;超涨跌停按交易所拒;CFFEX 涨跌停基准 = **昨结算价**(不是昨收)
- [ ] **保证金率**:qars 预设 IH 5%;真实(QIFI 样本反推)IH 12.0%、T 2.0%、TF 1.2%、TL 3.5%、au 16%
      → 按品种配置真实保证金率(合约表里配,不改 qars)
- [ ] **时间条件静默降级**:GFS / GTD / GTC / GFA 能解析但按 GFD 处理 → 中国期货交易所只收 GFD / IOC,其余拒
- [x] **日终**:GFD 单日终撤销(真实交易所收盘撤;LocalCTP 不撤是它的缺陷)—— 2b 已上线

### A'. 期货 —— P1
- [ ] 回报顺序:真实 CTP 全成时「状态 '0' 先于成交回报」(真实日志 2/2);我们是「先成交后 FINISHED」
      (与 hft_maker 约定 A3)→ **待用户定**:保持约定 or 改成真实顺序(EIFI 两种都能处理)
- [ ] 平仓今昨:非 SHFE/INE「先昨后今」(LocalCTP;生产网关 pos_* 是先今后昨,二者矛盾,以交易所规则为准);
      SHFE/INE CLOSE = 平昨,不足拒 51;平今不足拒 50
- [ ] 平仓盈亏口径:CTP 昨仓对昨结算价、今仓对开仓价(盯市);qars 一律对开仓价 —— 结算(2b)后才有差别
- [ ] 冻结手续费:真实 CTP 下单即冻结手续费(真实日志冻结 6.0),qars 不冻结;available = 权益 − 保证金 − 冻结保证金 − 冻结手续费
- [ ] 交易时段:已实现但默认关闭(7×24)→ **待用户定**(hft_maker 现在用「每小时切一段」适配 7×24)
- [ ] 市价单按交易所:CFFEX 收(最优五档剩撤 / 剩转限价),SHFE/INE 拒 80
- [ ] 单号格式:真实 CTP 报单编号 / 成交编号是 12 位右对齐空格填充(用户 QIFI 样本同样)—— 可选对齐
- [ ] 风控「Position ratio 名义 / 权益 ≤ 50%」不是柜台规则(CTP 只按可用资金拒 31)→ **待用户定**

### B. 股票(xlight / OES 口径)—— qaexchange 现在没有股票合约 → **待用户定是否纳入**
- 若纳入,关键语义:T+1 可卖量;100 股整手(科创买 ≥200);价格笼子(买 ≤ max(102%×参考价, 参考价+10 tick));
  涨跌停 10% / 20%;卖出印花税 0.05%;佣金 max(5 元, 费率);订单类型(限价 / 本方最优 / 对手方最优 /
  五档剩撤 / 五档转限价仅沪 / IOC、FOK 仅深);撤单终态 leaves=0 + cancel_quantity;拒单分柜台 5xx·6xx(英文、快)
  与交易所 101·102(中文);9:20~9:25 不可撤

### C. QIFI 账户体系对齐
- [ ] 对照 qatradegateway / qabroker_xlight 产出的 QIFI:trades 键 `order_id|exchange_trade_id` ✓、ALIVE/FINISHED ✓、
      last_msg 中文 ✓;核对 insert_date_time 口径、frozen_* 字段、available 公式、WithdrawQuota

### E. 柜台性能 / 持久化(2026-11-08 12:00 生产实测)
- [ ] **快照 I/O 风暴(P0)**:408 个账户快照共 3.24GB,每轮全部重写;sda 忙 97%、写 75MB/s,进程 08:24 起 3.5 小时写盘
      641GB(≈ 4.4TB/天);WAL 组提交 803 轮告警,每轮 p50 4.2s / p90 6.0s / 最长 57s(= 崩溃时可能丢的窗口)。
      根因:QIFI 背着跨天全部委托 / 成交(最大账户 112,741 委托 + 67,813 成交 = 101MB),资金 + 持仓 + 活单只有 8.4KB;
      紧凑 JSON 只省 24%,不是解法。→ 方案:**状态快照(每轮重写,KB 级)+ 当日历史追加写(JSONL)**,恢复时合并;配合 2b 日结清历史
      日结上线后实测:11-09 00:01 结算后 1.8MB,开市后 **~150MB/小时** 再涨 → 每天傍晚又回到 GB 级(日结只把增长封顶在一天)
- [x] 每笔委托 3~5 行 INFO(~260 行/秒、1.1GB/天,在下单线程上同步写)→ 降为 debug(9 处;22:46 上线)
- [ ] 每账户一个 gateway WAL(220 个目录)→ 组提交每轮平均 84 次 fsync → 合并成少量 WAL
- [ ] 时延(生产负载下,同一 WS 连接):DIFF 下单→ALIVE p50 1.36ms / p90 3.05ms / p99 31.7ms;撤单→FINISHED p50 0.43ms;
      HTTP 下单往返 p50 1.26ms / p99 80ms —— 下单比撤单多出的 ~0.9ms 在下单路径自身,待分段计时定位
- [ ] **延迟模型(真实性)**:实盘 xLight 单笔确认 ~5ms、撤单 ~4ms(`qatdbroker/tasks/lessons.md:265`),
      仿真零延迟进簿、1.4ms 回 ALIVE —— 比实盘乐观(排队位置、逆向选择被高估)
      → 2d 的「每交易所行情发布延迟」扩成「委托进簿延迟 + 回报延迟」,按交易所配置,默认 0
- [ ] 下单链路分段时延直方图(风控 / 冻结 / 撮合 / 回报推送)进 `/api/monitoring`

### F. QIFI 字段(2026-11-08 生产快照逐字段对照标准样本)
- [x] 成交 `trade_date_time` 恒早 8 小时(UTC 墙钟当北京时间传给 qars `parse_dt_ns`;HTTP 成交查询同样早 8h)(22:46 上线)
- [x] 委托 `time_condition` 恒为 "AND"、`volume_condition` 恒为 "GFD"、`exchange_id` 空(qars sim 分支写死)→ 按请求回填(22:46 上线)
- [x] 成交 `exchange_id` / `exchange_trade_id`、持仓 `exchange_id` 空 → 回填(22:46 上线)
- [x] 持仓 `position_price_*` 漂移(qars 平仓公式;IH 做市账户 1096.10 vs 应为 1804.82)、平完 NaN → 快照 null(84 个文件)
      → 按 position_cost 反推,恢复时也校正(22:46 上线)
- [ ] 持仓今仓口径:qars `volume_long_today` **不含**冻结、`volume_long = 今 + 昨 + 冻结`;CTP / qatradegateway(`tradectp.cpp:3734`)
      是今仓含冻结、`volume_long = 今 + 昨` → 投影改 CTP 口径,**先和 hft_maker 确认**(前端用 volume_long − frozen,不受影响)
      **日结后天天可见(提级)**:qars 平仓的冻结 / 成交一律记在今仓上、昨仓保持日初值 —— 11-09 00:05 快照 484 个持仓方向
      151 个今仓为负(如 SA2701 多头 今 −760 / 昨 20010 / 冻结 750 / 总 20000)。总量 = 今 + 昨 + 冻结 恒成立(0 例外),
      平仓校验按总量、不会平超;但平过昨仓后 CLOSETODAY 会被少放行 → 与 2c「CLOSE 今昨口径按交易所配置」一起做
      (hft_maker 11-09 确认:网关只读 volume_long / volume_short 总量、FG/SA/IH 只用 CLOSE,不受影响;沙盒交易 SHFE / INE 前再做)
- [ ] 账户 `trading_day` 日结后不前进:qars 用账户私有 `time`(最后一笔委托 / 成交时刻)查交易日历得出(account.rs:868–876),
      `settle()` 不动它 → 11-09 日结后 408 户里 54 个不活跃账户仍是 20260907 / 20260904,活跃户要等下一笔委托才变 20261109;
      CTP 的 TradingDay 结算后全体前进。`time` 无 Rust 公开 setter(只有 pyo3 `set_time`)→ 在 qaexchange 的 QIFI 投影 / 快照处
      按交易所当前交易日改写,不改 qars
- [ ] `seqno` 重启归零:QIFI 的来自 qars 私有 `event_id`(恢复置 0);DIFF 的 `diff_seqno` 只在内存 —— 客户端按 EIFI §6
      水位线去重时,重启后的回报会被丢 → 起点改为启动时刻基数(先确认 hft_maker 按 int64 解析)
- [ ] DIFF 登录只补资金 / 持仓 / 活单,不补当日已终结委托与成交(CTP RESTART / qatradegateway 重连全量重推)—— 日结后再做
- [ ] HTTP `/api/order/user` 返回自定义结构(filled_volume / submit_time …),不是 QIFI Order 字段
- [ ] HTTP 成交查询每笔列两遍(QIFI `T{纳秒}` 与 TradeRecorder `T{序号}` 按 trade_id 去重永不命中;隔离实例实测 2 笔成交返回 4 行)
      → 按「路由单号(QIFI 委托的 exchange_order_id)+ 价 + 量」做多重集合匹配去重,保留 TradeRecorder 行的对手单号
- [ ] 挂单 / 撤单后不推 accounts / positions(只在成交时推)—— DIFF 截面里 available / frozen_margin 到下一笔成交前都是旧值

### H. 柜台正确性 / 安全(2026-11-08 下单路径只读审计 + 逐条核实)
- [ ] **(P0,待用户定)账户归属不校验**:`verify_account_ownership` 在 user_id 为空或等于 account_id 时直接放行
      (`account_mgr.rs:540`),`/api/order` 无鉴权中间件,`/api/management/accounts` 可列出全部账户号,服务监听 0.0.0.0
      → 局域网内任何人可替任何账户下单 / 撤单。真实柜台:登录 + 会话 token(xLight v1.7.1.5 起登录必须带 pub_key,
      PB 逐账户 `auth_trade` 拿 trade_token)。改成强制鉴权会影响 80 个 HTTP 仿真参与者的接入方式
- [ ] 账户状态从不检查:冻结 / 停用账户照样能下单(`account_admin::can_trade` 无调用方)
- [ ] IOC 剩余量:撮合后**另取一次锁**才撤(`order_router.rs` `handle_ioc_remaining`),中间剩余量挂在簿上、可被其他 worker
      成交;真实交易所 FAK 剩余量由撮合原子撤销。仿真噪声交易者大量用分片 IOC
- [ ] MARKET 单以限价进簿,不带 IOC 时会挂单(真实:CFFEX 最优五档剩撤 / 剩转限价,其余交易所市价剩余撤)—— 并入 2c
- [ ] 自成交只拦 OPEN(平仓单可以和本账户挂着的开仓单成交);范围是账户,不是交易编码 / 整户
- [ ] HTTP 下单量是 f64、不校验整数(1.5 手能下)→ 非整数拒 15「报单字段有误」
- [ ] 竞态(代码层面,**生产未观测到**):挂单的撮合号映射在松开簿锁之后才登记,另一 worker 的吃单若先撮到它,
      maker 一侧按「外部订单」跳过、不入账。12:15 / 12:32 两次守恒检查:FG/SA 四个合约多空精确相等(17 万手量级);
      IC/IF/IH 的 −143~−184 两次一致、不漂,是历史遗留(9 月丢账户事故)
- [ ] 一次撮合两侧的 exchange_trade_id 各取一号(真实交易所一笔成交双方同一成交编号)
- [ ] WAL 每个文件缓冲满 1000 条时由**下单线程**就地 write + fsync(背压设计);后台一轮 4~57s 时繁忙合约会撞上
- [x] 平昨(CLOSEYESTERDAY)成交不入账 → 修复 + 回归测试(22:46 上线)
- [x] 未上市合约冻结资金永不释放 → 冻结前拒 4006 / CTP 16(22:46 上线)
- [x] DIFF 客户端编号落盘每笔 open + 多次 write → 常驻句柄一次写(22:46 上线)
- [x] 手动结算接口(生产未开管理端鉴权)一调即可让交易所退出 → 改走完整日终(22:46 上线)
- [ ] **启动恢复挂回的委托彼此交叉时成交不入账**:`restore_orders_from_accounts` 把账户里所有 ALIVE 委托重新挂进撮合簿,
      `process_order` 的结果只认 `Accepted` —— 恢复单(含早已不在簿上的陈旧 ALIVE 单)互相交叉时撮合簿里成交了,
      双方账户都不入账,撮合簿与账户从此对不上。隔离实例日终那 64 笔「撤单失败 OrderNotFound」即此;生产每次重启都走这里
      (11-09 首次生产日终的 39 笔 OrderNotFound 也是 22:46 重启恢复的这批)。
      修法方向:恢复时按价格优先挂回且不撮合(qars 挂单接口)或把恢复期成交走正常入账

### G. 对照 xLight 柜台的功能项(**待用户定**)
- 子账户 / MOM:xLight 的子账户是客户端按 client_id
  记的策略账本,柜台账户共用 —— T+1 可卖、资金、自成交、报单速率按**整户**约束(`qatdbroker/docs/09-multi-strategy-qifi.md`);
  我们是「用户 → N 个独立账户」,自成交按账户判
- 报撤单流控:xLight 每市场日报单上限 20,000(开户时定,`reset_order_limit` 不能超)、309「请求过快」→ 并入 2d 计数,可选拒单
- 外部接入:登录交易日必须匹配、`on_auth_trade_invalid` 强制下线不自动重授权、终端信息采集(CTP 看穿式 AppID / AuthCode)
- 算法单:母单 / 子单,子单 `account_index` = 母单账户
- A 股仿真(若纳入,规格见 B):另加 xLight 实测怪癖 —— 沪市确认 leaves=0、撤单是独立 xid、撤未知单不回执、重复 order_sequence 不回执

### D. 顺带发现的其他仓库问题(不在本仓库,**需用户决定谁来修**)
1. **qatradegateway 生产变体 `open-trade-ctpsed15/tradectp.cpp:2741-2744`**:状态判断把 `==` 写成了 `=`
   (`pOrder->OrderStatus = THOST_FTDC_OST_PartTradedNotQueueing`)→ 网关副本的状态被覆盖成 '2':
   「撤单成功」永远不发、交易所拒单不发「下单失败」、条件单撤单后续不触发、内部 map 泄漏、每个回报都触发重查。
   同样存在于 `-local` / `-rohon` / `qatragateway-jess`;`open-trade-ctp` 没有。QIFI 订单字段本身不受影响。
2. qatradegateway:成交手续费恒为 0(`tradectp.cpp:2989`);强平 offset '2' 未映射 → 按开仓计入持仓(`:1502-1511`)
3. **qabroker_oes 从未编译通过**(读不存在的字段、`OesAsyncApi_AddChannel` 参数个数错);状态码表整张错号
   (全成显示为 PARTIAL_CANCELLED、部成显示 UNKNOWN);EIFI 没有 OES 映射
4. qabroker_xlight:IOC 残量撤单被短暂显示为活单;撤单后迟到的成交把订单「复活」;手续费重复计(生产首日 7927 vs 43)
5. EIFI.md:引用的行号来自非生产文件;「状态先于成交」在真实 CTP 是常态;xlight 撤单失败的回调位置、`trade_fee` 语义与实测不符
