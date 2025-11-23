# Exchange 回报重构 TODO（更新：2025-10-08）

> 目标：完成“交易所内部记录 ↔ 账户回报”双通道，彻底替换旧版 `handle_filled/handle_partially_filled` 路径，并让账户依据交易所回报自行判定订单状态。

## 已完成
- [x] Phase 1：创建 `ExchangeResponse/ExchangeOrderRecord/ExchangeTradeRecord`（见 `src/exchange/exchange_types.rs`）。
- [x] Phase 2：`ExchangeIdGenerator` 为每个合约维护统一序列，并在 `OrderRouter` 中使用（`src/exchange/id_generator.rs`、`src/exchange/order_router.rs:656-812`）。
- [x] Phase 5（部分）：订单/成交/账户回报已经写入 WAL（`trade_gateway.rs:541-724`）。

## 待处理事项

### 1. TradeGateway → 账户推送链路
- [x] 在 `handle_order_accepted_new / handle_order_rejected_new / handle_trade_new / handle_cancel_*_new` 中补全推送逻辑：
  - [x] 通过 `NotificationBroker` 或回退通道将 `ExchangeResponse` 推到 `Notification::OrderStatus` / `Notification::Trade`，并同步触发 DIFF Snapshot（`src/exchange/trade_gateway.rs`）。
  - [x] 引入统一的 `emit_order_status()` / `emit_trade_notification()` 工具函数，避免在 handler 内重复序列化逻辑。
- [ ] 落地账户侧 `ExchangeResponseRecord` replay：重启时从 `__ACCOUNT__/{user_id}` WAL 读取回报，恢复订阅端最新状态。

### 2. 移除旧的账户判定逻辑
- [ ] 删除 `handle_filled/handle_partially_filled` 以及依赖 `OrderStatusNotification` 的旧代码和文档引用（`src/exchange/trade_gateway.rs:201-409` 以及 `docs/05_integration/diff_protocol.md` 等）。
- [ ] 将所有调用方（含示例、文档、脚本）更新为只使用新的 `handle_order_*_new / handle_trade_new / handle_cancel_*_new` API。

### 3. 账户端状态机（Phase 4）
- [ ] 账户收到 `TRADE` 回报后自行调用 `QAOrder.trade()` 更新 `volume_left`，并根据值推导 `FILLED/PARTIAL_FILLED`，不再依赖交易所推送旧状态。
- [ ] 覆盖保证金、冻结资金、风险度的刷新逻辑，确保同一个 `ExchangeResponse` 只消费一次。
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
- [ ] `docs/05_integration/diff_protocol.md`、`docs/04_api/websocket/quick_start.md`：替换旧的 `handle_filled` 示例，描述 5 种回报及字段含义。
- [ ] 清理 `CHANGELOG.md`、`docs/02_architecture/system_overview.md` 等仍提及旧回报流程的章节。

> 完成以上事项后，交易所与账户间的职责划分才能彻底生效，并为后续的多副本/回放能力打下基础。
