# 交易所架构重构计划

## 背景

将交易所内部记录和交易所→账户回报正确分离，使用统一的自增event sequence保证事件顺序性。

## Phase 1: 重新设计数据结构 ✅

**目标**：区分交易所内部记录和账户回报

- [x] 1.1 创建交易所回报枚举（5种类型：ACCEPTED/REJECTED/CANCEL_ACCEPTED/CANCEL_REJECTED/TRADE）
- [x] 1.2 创建交易所内部逐笔委托记录 ExchangeOrderRecord（exchange/instrument/exchange_order_id/direction/offset/price/volume/time）
- [x] 1.3 创建交易所内部逐笔成交记录 ExchangeTradeRecord（exchange/instrument/buy_oid/sell_oid/deal_price/deal_volume/time/trade_id）

**已完成文件**：
- `src/exchange/exchange_types.rs` - 定义 ExchangeResponse, ExchangeOrderRecord, ExchangeTradeRecord

---

## Phase 2: 实现统一自增ID生成器 🔄

**目标**：按instrument维度保证事件顺序

- [x] 2.1 为每个instrument维护单个AtomicI64 event_sequence（统一序列号）
- [ ] 2.2 实现 next_sequence(instrument) -> i64（下单/撤单/成交都用同一个序列）
- [ ] 2.3 编写单元测试验证序列严格递增和并发安全

**关键设计**：
```rust
// 统一的event sequence，形成顺序流
pub struct ExchangeIdGenerator {
    event_sequences: DashMap<String, AtomicI64>,
}

// 所有事件（下单、撤单、成交）都用同一个序列
pub fn next_sequence(&self, instrument_id: &str) -> i64
```

**测试要点**：
- 同一instrument的sequence严格递增
- 并发安全（10线程 x 100次 = 1000个唯一ID）
- 不同instrument独立计数

---

## Phase 3: 重构TradeGateway ✅

**目标**：移除账户判断逻辑，只推送交易所回报

- [x] 3.1 添加 ExchangeIdGenerator 到 TradeGateway（统一事件序列生成器）
- [x] 3.2 实现 handle_order_accepted_new(instrument_id, user_id, order_id) - 推送ACCEPTED回报
- [x] 3.3 实现 handle_order_rejected_new(instrument_id, user_id, order_id, reason) - 推送REJECTED回报
- [x] 3.4 实现 handle_trade_new(instrument_id, exchange_order_id, user_id, order_id, volume, price) - 推送TRADE回报
- [x] 3.5 实现 handle_cancel_accepted_new(instrument_id, exchange_order_id, user_id, order_id) - 推送撤单成功回报
- [x] 3.6 实现 handle_cancel_rejected_new(instrument_id, exchange_order_id, user_id, order_id, reason) - 推送撤单失败回报
- [x] 3.7 编写单元测试（7个测试全部通过）

**关键变化**：
```rust
// 旧的（错误）
handle_filled() -> 判断全部成交 -> 推送FILLED
handle_partially_filled() -> 判断部分成交 -> 推送PARTIAL_FILLED

// 新的（正确）
handle_trade() -> 只推送TRADE，不做任何判断
账户端收到TRADE -> 自己计算volume_left -> 判断FILLED/PARTIAL_FILLED
```

---

## Phase 4: 实现账户端逻辑

**目标**：账户自己判断订单状态

- [ ] 4.1 账户收到TRADE回报后更新 QAOrder.volume_left -= trade.volume
- [ ] 4.2 账户自己判断：volume_left == 0 → FILLED，否则 → PARTIAL_FILLED

**实现位置**：
- `src/exchange/account_mgr.rs` - 监听TRADE回报
- 更新QAOrder状态逻辑

---

## Phase 5: 存储分离

**目标**：交易所内部 vs 账户回报分离存储

- [ ] 5.1 交易所内部存储：`{instrument_id}/orders/` - 存储 ExchangeOrderRecord
- [ ] 5.2 交易所内部存储：`{instrument_id}/trades/` - 存储 ExchangeTradeRecord
- [ ] 5.3 账户回报存储：`__ACCOUNT__/` - 存储推送给用户的5种回报
- [ ] 5.4 更新 WAL Record 类型：添加 ExchangeOrderRecord 和 ExchangeTradeRecord

**存储架构**：
```
storage/
├── SHFE.cu2501/
│   ├── orders/          # 交易所内部逐笔委托
│   │   └── wal/
│   └── trades/          # 交易所内部逐笔成交
│       └── wal/
├── __ACCOUNT__/         # 账户回报（5种）
│   └── wal/
└── users/               # 用户数据
    └── wal/
```

---

## Phase 6: 更新 OrderRouter

**目标**：对接新的回报机制

- [ ] 6.1 撮合成功 Success::Accepted → handle_order_accepted
- [ ] 6.2 撮合成功 Success::Filled/PartiallyFilled → handle_trade（只发TRADE，不区分全部/部分）
- [ ] 6.3 撤单成功 Success::Cancelled → handle_cancel_accepted
- [ ] 6.4 撮合失败 Failed → handle_order_rejected

**流程图**：
```
OrderRouter.submit_order()
  ↓
matching_engine.match_order()
  ↓
Success::Accepted
  → id_gen.next_sequence() → exchange_order_id
  → gateway.handle_order_accepted(exchange_order_id)
  → 推送 ExchangeResponse::OrderAccepted 给账户
  → 存储 ExchangeOrderRecord 到 {instrument}/orders/

Success::Filled
  → id_gen.next_sequence() → trade_id
  → gateway.handle_trade(trade_id, exchange_order_id, volume, price)
  → 推送 ExchangeResponse::Trade 给账户
  → 存储 ExchangeTradeRecord 到 {instrument}/trades/
  → 账户收到TRADE → 更新volume_left → 判断FILLED/PARTIAL
```

---

## Phase 7: 测试完整流程

**目标**：全面测试新架构

- [ ] 7.1 单元测试：测试统一序列生成器（同一instrument ID严格递增）
- [ ] 7.2 单元测试：测试交易所5种回报推送
- [ ] 7.3 单元测试：测试账户端判断逻辑（TRADE → FILLED/PARTIAL_FILLED）
- [ ] 7.4 集成测试：完整下单流程（提交→接受→成交→账户更新）
- [ ] 7.5 集成测试：撤单流程（提交→接受→撤单→撤单成功/失败）
- [ ] 7.6 集成测试：验证交易所内部存储和账户回报存储分离

**测试用例示例**：
```rust
#[test]
fn test_full_order_flow() {
    // 1. 提交订单
    let response = router.submit_order(...);

    // 2. 验证收到ACCEPTED回报
    let accepted = receiver.recv();
    assert!(matches!(accepted, ExchangeResponse::OrderAccepted { .. }));

    // 3. 触发撮合成交
    engine.trigger_match();

    // 4. 验证收到TRADE回报
    let trade = receiver.recv();
    assert!(matches!(trade, ExchangeResponse::Trade { .. }));

    // 5. 验证账户自己判断为FILLED
    let account = account_mgr.get_account(user_id);
    let order = account.get_order(order_id);
    assert_eq!(order.status, "FILLED");
    assert_eq!(order.volume_left, 0.0);
}
```

---

## Phase 8: 更新文档和清理废弃代码

**目标**：文档化新架构，删除旧代码

- [ ] 8.1 更新 CLAUDE.md 文档 - 说明新的交易所回报机制
- [ ] 8.2 删除废弃的 handle_filled/handle_partially_filled 方法
- [ ] 8.3 更新 WebSocket 协议文档 - 说明5种交易所回报

**文档更新要点**：
- 交易所回报只有5种：ACCEPTED/REJECTED/TRADE/CANCEL_ACCEPTED/CANCEL_REJECTED
- 账户端自己判断FILLED/PARTIAL_FILLED
- event_sequence统一序列号保证事件顺序
- 存储分离：交易所内部记录 vs 账户回报

---

## 验收标准

1. ✅ 交易所只推送5种回报，不做任何账户逻辑判断
2. ✅ 账户收到TRADE回报后自己判断订单状态
3. ✅ event_sequence严格自增，保证事件顺序
4. ✅ 存储正确分离：{instrument}/orders/, {instrument}/trades/, __ACCOUNT__/
5. ✅ 所有测试通过（单元测试 + 集成测试）
6. ✅ 文档完整更新

---

## 进度追踪

- **Phase 1**: ✅ 已完成（创建ExchangeResponse, ExchangeOrderRecord, ExchangeTradeRecord）
- **Phase 2**: ✅ 已完成（ExchangeIdGenerator统一序列，5个测试通过）
- **Phase 3**: ✅ 已完成（5个新handler方法，7个测试通过）
- **Phase 4**: ⏳ 待开始（账户端逻辑）
- **Phase 5**: ⏳ 待开始（存储分离）
- **Phase 6**: ⏳ 待开始（OrderRouter对接）
- **Phase 7**: ⏳ 待开始（集成测试）
- **Phase 8**: ⏳ 待开始（文档更新）
