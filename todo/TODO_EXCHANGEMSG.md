Phase 1: 重新设计数据结构 - 区分交易所内部记录和账户回报
     ☒ 1.1 创建交易所回报枚举（5种类型：ACCEPTED/REJECTED/CANCEL_ACCEPTED/CANCEL_REJECTED/TRADE）
     ☒ 1.2 创建交易所内部逐笔委托记录 
       ExchangeOrderRecord（exchange/instrument/exchange_order_id/direction/offset/price/volume/time）
     ☒ 1.3 创建交易所内部逐笔成交记录 
       ExchangeTradeRecord（exchange/instrument/buy_oid/sell_oid/deal_price/deal_volume/time/trade_id）
     ☐ Phase 2: 实现自增ID生成器 - 按instrument维度保证时间自增
     ☐ 2.1 为每个instrument维护独立的AtomicI64 sequence counter
     ☐ 2.2 实现 generate_exchange_order_id(instrument) -> i64（自增）
     ☐ 2.3 实现 generate_trade_id(instrument) -> i64（自增）
     ☐ Phase 3: 重构TradeGateway - 移除账户判断逻辑
     ☐ 3.1 移除 handle_filled/handle_partially_filled（这是账户判断的，不是交易所回报）
     ☐ 3.2 实现 handle_order_accepted(exchange_order_id) - 推送ACCEPTED回报给账户
     ☐ 3.3 实现 handle_order_rejected(exchange_order_id, reason) - 推送REJECTED回报
     ☐3.4 实现 handle_trade(trade_id, exchange_order_id, volume, price) -
      推送TRADE回报（不判断全部/部分）
     ☐ 3.5 实现 handle_cancel_accepted(exchange_order_id) - 推送撤单成功回报
     ☐ 3.6 实现 handle_cancel_rejected(exchange_order_id, reason) - 推送撤单失败回报
     ☐ Phase 4: 实现账户端逻辑 - 账户自己判断订单状态
     ☐ 4.1 账户收到TRADE回报后更新 QAOrder.volume_left -= trade.volume
     ☐ 4.2 账户自己判断：volume_left == 0 → FILLED，否则 → PARTIAL_FILLED
     ☐ Phase 5: 存储分离 - 交易所内部 vs 账户回报
     ☐ 5.1 交易所内部存储：{instrument_id}/orders/ - 存储 ExchangeOrderRecord
     ☐ 5.2 交易所内部存储：{instrument_id}/trades/ - 存储 ExchangeTradeRecord
     ☐ 5.3 账户回报存储：__ACCOUNT__/ - 存储推送给用户的5种回报
     ☐ 5.4 更新 WAL Record 类型：添加 ExchangeOrderRecord 和 ExchangeTradeRecord
     ☐ Phase 6: 更新 OrderRouter 对接新的回报机制
     ☐ 6.1 撮合成功 Success::Accepted → handle_order_accepted
     ☐ 6.2 撮合成功 Success::Filled/PartiallyFilled → handle_trade（只发TRADE，不区分全部/部分）
     ☐ 6.3 撤单成功 Success::Cancelled → handle_cancel_accepted
     ☐ 6.4 撮合失败 Failed → handle_order_rejected
     ☐ Phase 7: 测试完整流程
     ☐ 7.1 单元测试：测试自增ID生成器（同一instrument ID严格递增）
     ☐ 7.2 单元测试：测试交易所5种回报推送
     ☐ 7.3 单元测试：测试账户端判断逻辑（TRADE → FILLED/PARTIAL_FILLED）
     ☐ 7.4 集成测试：完整下单流程（提交→接受→成交→账户更新）
     ☐ 7.5 集成测试：撤单流程（提交→接受→撤单→撤单成功/失败）
     ☐ 7.6 集成测试：验证交易所内部存储和账户回报存储分离
     ☐ Phase 8: 更新文档和清理废弃代码
     ☐ 8.1 更新 CLAUDE.md 文档 - 说明新的交易所回报机制
     ☐ 8.2 删除废弃的 handle_filled/handle_partially_filled 方法
     ☐ 8.3 更新 WebSocket 协议文档 - 说明5种交易所回报