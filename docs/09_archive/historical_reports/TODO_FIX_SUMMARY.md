# QAExchange TODO/Mock 数据修复总结报告

## 执行时间
2025-10-05

## 修复状态概览

| 类别 | 总计 | 已修复 | 待修复 | 优先级 |
|------|------|--------|--------|--------|
| **后端 P0（高）** | 4 | 2 | 2 | 🔴 HIGH |
| **后端 P1（中）** | 4 | 0 | 4 | 🟡 MEDIUM |
| **后端 P2（低）** | 2 | 0 | 2 | 🟢 LOW |
| **前端 P0（高）** | 4 | 2 | 2 | 🔴 HIGH |
| **前端 P1（中）** | 2 | 0 | 2 | 🟡 MEDIUM |
| **前端 P2（低）** | 7 | 0 | 7 | 🟢 LOW |
| **总计** | 23 | 4 | 19 | - |

---

## 一、已完成修复 (4项)

### ✅ 1. handlers.rs - 持仓浮动盈亏计算

**文件**: `src/service/http/handlers.rs:251-253`

**问题**:
- 查询持仓时，profit_long 和 profit_short 返回硬编码的 0.0
- 用户无法看到实时浮动盈亏

**修复**:
```rust
// 修复前
profit_long: 0.0, // TODO: 计算浮动盈亏
profit_short: 0.0,

// 修复后
profit_long: pos.float_profit_long(),
profit_short: pos.float_profit_short(),
```

**影响**:
- ✅ GET /api/position/{user_id} 现在返回真实浮动盈亏
- ✅ 前端持仓页面能够显示实时盈亏数据

---

### ✅ 2. monitoring.rs - 存储统计数据

**文件**: `src/service/http/monitoring.rs:136-175`

**问题**:
- OLTP 和 OLAP 存储统计返回全部为 0 的 mock 数据
- 监控页面无法显示真实的存储系统状态

**修复**:
1. 修改 `StorageSubscriber::new()` 返回统计句柄
2. 在 `ExchangeServer` 中保存统计句柄
3. 在 `AppState` 中传递统计句柄
4. 监控 API 从真实句柄读取数据

**修复文件**:
- `src/storage/subscriber.rs:89` - 返回统计句柄
- `src/main.rs:107-110,181-182,299,344,401-402` - 保存并传递句柄
- `src/service/http/handlers.rs:15-16,23-24` - AppState 新增字段
- `src/service/http/monitoring.rs:136-175,220-264` - 使用真实数据

**影响**:
- ✅ GET /api/monitoring/system 返回真实 OLTP/OLAP 统计
- ✅ GET /api/monitoring/storage 返回真实存储数据

---

### ✅ 3. monitoring/index.vue - 监控页面真实数据加载

**文件**: `web/src/views/monitoring/index.vue`

**问题**:
- loadMonitoringData() 只有空实现 `console.log('加载监控数据')`
- 页面使用硬编码的假数据

**修复**:
- 调用 `getSystemMonitoring()` API
- 绑定真实数据到UI组件
- 添加自动刷新（每10秒）
- 添加手动刷新按钮
- 添加 loading 状态
- 显示账户、订单、成交、存储等全部监控指标

**影响**:
- ✅ 监控页面显示实时系统状态
- ✅ 用户可以实时查看账户数、订单数、成交额、存储统计
- ✅ OLAP 转换系统状态可见

---

### ✅ 4. service/http/mod.rs - HttpServer 构造函数

**文件**: `src/service/http/mod.rs:45-51`

**问题**: 编译错误 - AppState 缺少新增的字段

**修复**:
```rust
let app_state = Arc::new(AppState {
    order_router,
    account_mgr,
    trade_recorder,
    storage_stats: None,      // 新增
    conversion_mgr: None,     // 新增
});
```

**影响**: ✅ 编译通过

---

## 二、待修复项（按优先级）

### 🔴 P0 - 高优先级（需立即修复）

#### 5. settlement.rs:100 - 日终结算功能
**文件**: `src/exchange/settlement.rs:100-102`

**问题**:
```rust
// TODO: 实现 AccountManager::list_accounts() 方法
let total_accounts = 0;
let settled_accounts = 0;
```

**修复方案**:
1. AccountManager 已有 `get_all_accounts()` 方法
2. 修改 daily_settlement() 使用该方法
3. 遍历所有账户执行结算

**预计工作量**: 30分钟

---

#### 6. admin.rs:207 - 下市合约持仓检查
**文件**: `src/service/http/admin.rs:207-213`

**问题**:
```rust
// TODO: 检查是否有未平仓持仓
// let has_open_positions = check_open_positions(&instrument_id);
```

**修复方案**:
```rust
// 遍历所有账户，检查是否有该合约的持仓
let accounts = state.account_mgr.get_all_accounts();
for account in accounts {
    let acc = account.read();
    if let Some(pos) = acc.get_position(&instrument_id) {
        if pos.volume_long_unmut() > 0.0 || pos.volume_short_unmut() > 0.0 {
            return Ok(HttpResponse::BadRequest().json(
                ApiResponse::<()>::error("Cannot delist: open positions exist".to_string())
            ));
        }
    }
}
```

**预计工作量**: 15分钟

---

#### 7. admin/instruments.vue - 合约管理 mock 数据
**文件**: `web/src/views/admin/instruments.vue:276-350`

**问题**: 使用 mock 数据数组

**修复方案**:
- 调用 `getAllInstruments()` API 加载真实合约
- 实现创建、更新、暂停、恢复、下市操作的 API 调用
- 移除所有 mock 数据

**预计工作量**: 1小时

---

#### 8. admin/settlement.vue - 结算管理 mock 数据
**文件**: `web/src/views/admin/settlement.vue:281-350`

**问题**: 使用 mock 结算历史数据

**修复方案**:
- 调用 `getSettlementHistory()` API
- 调用 `setSettlementPrice()` API
- 调用 `executeSettlement()` API
- 移除所有 mock 数据

**预计工作量**: 1小时

---

### 🟡 P1 - 中优先级（本周完成）

#### 9-12. trade_gateway.rs - 订单剩余量
**文件**: `src/exchange/trade_gateway.rs:253,277`

**问题**: `remaining_volume: 0.0` 硬编码

**修复方案**:
```rust
// 从订单查询剩余量
let remaining = order.volume_orign - order.volume_traded;
```

**预计工作量**: 20分钟

---

#### 13. settlement.rs:180 - 手续费统计
**文件**: `src/exchange/settlement.rs:180`

**问题**: `let commission = 0.0; // TODO: 从成交记录统计`

**修复方案**:
- 从账户的 trades 表统计当日手续费
- 或使用 account.commission 字段

**预计工作量**: 30分钟

---

#### 14. settlement.rs:199 - 强平逻辑
**文件**: `src/exchange/settlement.rs:199`

**问题**: 检测到高风险账户但未执行强平

**修复方案**:
- 调用 OrderRouter 提交平仓订单
- 记录强平日志到 RiskMonitor

**预计工作量**: 2小时

---

#### 15. order_router.rs:652 - 撤单实现
**文件**: `src/exchange/order_router.rs:652`

**问题**: 只更新状态，未从撮合引擎撤单

**修复方案**:
```rust
// 从撮合引擎撤单
self.matching_engine.cancel_order(instrument_id, order_id)?;
```

**预计工作量**: 30分钟

---

#### 16. trade_gateway.rs:166 - 撮合失败处理
**文件**: `src/exchange/trade_gateway.rs:166`

**问题**: 只打印日志 `log::warn!("Match failed: {:?}", failed);`

**修复方案**:
- 发送 OrderStatus(REJECTED) 通知
- 解冻账户资金/持仓

**预计工作量**: 1小时

---

#### 17-18. 前端持仓可用量 & 资金曲线
**文件**:
- `web/src/views/trade/components/CloseForm.vue:105`
- `web/src/views/user/account-curve.vue:194`

**修复方案**: 需要后端支持，见下文

---

### 🟢 P2 - 低优先级（后续优化）

#### 19. user_mgr.rs:99 - 密码加密
**问题**: 密码明文存储

**修复方案**: 使用 bcrypt
```rust
use bcrypt::{hash, verify, DEFAULT_COST};
password_hash: hash(&req.password, DEFAULT_COST)?,
```

**预计工作量**: 1小时（含测试）

---

#### 20. user_mgr.rs:173 - JWT验证
**问题**: Token 验证过于简单

**修复方案**: 使用 jsonwebtoken crate

**预计工作量**: 2小时（含测试）

---

#### 21-27. 其他前端增强功能
- Excel 导出
- K线图表
- 详情对话框
- 等等

**预计工作量**: 5-10小时

---

## 三、新增 API 需求

### ❌ 待实现 API

#### GET /api/account/{user_id}/equity-curve
**用途**: 查询账户权益曲线

**返回格式**:
```json
{
  "success": true,
  "data": {
    "dates": ["2025-10-01", "2025-10-02", "2025-10-03"],
    "equity": [1000000.0, 1005000.0, 998000.0],
    "balance": [1000000.0, 1004500.0, 997500.0]
  }
}
```

**实现方案**:
1. 从 TradeRecorder 或 ClickHouse 查询历史账户快照
2. 按日期聚合
3. 返回时间序列数据

**预计工作量**: 3小时

---

## 四、编译测试结果

### ✅ 后端编译
```bash
$ cargo check --lib
Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.98s
```

**状态**: ✅ 通过（17个warning，0个error）

### 🔄 前端编译
**状态**: 未测试（建议运行 `npm run build`）

---

## 五、下一步行动计划

### 今天完成（剩余 P0 项）:
1. [ ] 修复 admin/instruments.vue - 合约管理（1小时）
2. [ ] 修复 admin/settlement.vue - 结算管理（1小时）
3. [ ] 修复 admin/risk.vue - 风控监控（1小时）
4. [ ] 修复 admin.rs - 下市检查（15分钟）
5. [ ] 修复 settlement.rs - 日终结算（30分钟）

**总计**: 约4小时

### 本周完成（P1 项）:
6. [ ] 订单剩余量（20分钟）
7. [ ] 手续费统计（30分钟）
8. [ ] 撤单实现（30分钟）
9. [ ] 撤合失败处理（1小时）
10. [ ] 强平逻辑（2小时）

**总计**: 约5小时

### 下周计划（P2 + 新功能）:
11. [ ] 资金曲线 API（3小时）
12. [ ] 密码加密（1小时）
13. [ ] JWT验证（2小时）
14. [ ] 其他前端增强（5-10小时）

**总计**: 约11-16小时

---

## 六、风险与依赖

### 依赖项
1. ✅ qars2 - Position::float_profit_long/short() 方法（已可用）
2. ✅ AccountManager::get_all_accounts() 方法（已可用）
3. ❌ TradeRecorder 历史数据查询（需实现）
4. ❌ ClickHouse 集成（可选，用于历史数据）

### 技术风险
1. **强平逻辑** - 复杂度高，需仔细测试防止资金损失
2. **JWT验证** - 需要处理 token 刷新、过期等逻辑
3. **资金曲线** - 如果无历史数据，需从WAL恢复

### 兼容性
- ✅ 所有修改向后兼容
- ✅ API 格式未变
- ✅ 前端使用渐进增强

---

## 七、修复文件清单

### 后端文件（已修改）
1. `src/service/http/handlers.rs` - ✅ 浮动盈亏计算
2. `src/service/http/monitoring.rs` - ✅ 存储统计
3. `src/storage/subscriber.rs` - ✅ 返回统计句柄
4. `src/main.rs` - ✅ 保存统计句柄
5. `src/service/http/mod.rs` - ✅ 修复编译错误

### 前端文件（已修改）
1. `web/src/views/monitoring/index.vue` - ✅ 监控数据加载

### 待修改文件（后端）
1. `src/service/http/admin.rs` - ⏳ 下市检查
2. `src/exchange/settlement.rs` - ⏳ 日终结算、手续费、强平
3. `src/exchange/trade_gateway.rs` - ⏳ 订单剩余量、失败处理
4. `src/exchange/order_router.rs` - ⏳ 撤单实现
5. `src/exchange/user_mgr.rs` - ⏳ 密码加密、JWT

### 待修改文件（前端）
1. `web/src/views/admin/instruments.vue` - ⏳ 合约管理
2. `web/src/views/admin/settlement.vue` - ⏳ 结算管理
3. `web/src/views/admin/risk.vue` - ⏳ 风控监控
4. `web/src/views/user/account-curve.vue` - ⏳ 资金曲线
5. `web/src/views/trade/components/CloseForm.vue` - ⏳ 可用量

---

## 八、总结

### 本次修复成果
- ✅ **修复了4个关键问题**（浮动盈亏、存储统计、监控页面）
- ✅ **后端编译通过**
- ✅ **创建了详细的修复计划和技术文档**
- ✅ **识别了23个TODO/Mock项**

### 剩余工作
- 19个待修复项
- 预计总工作量：20-30小时
- 优先级明确，可分批完成

### 建议
1. **立即修复** P0 高优先级项（约4小时）
2. **本周完成** P1 中优先级项（约5小时）
3. **渐进优化** P2 低优先级项（后续进行）
4. **定期测试** 每完成一批修复后进行端到端测试

---

**报告生成时间**: 2025-10-05
**生成工具**: @yutiansut
**项目**: qaexchange-rs
