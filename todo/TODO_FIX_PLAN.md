# QAExchange 前后端 TODO/Mock 数据修复计划

## 扫描时间
2025-10-05

## 发现统计
- 后端TODO项：10个
- 前端TODO/Mock项：23个
- 总计：33个待修复项

---

## 一、后端 TODO 项

### P0 - 高优先级（影响功能完整性）

#### 1. ✅ **已完成** - handlers.rs:251 - 计算持仓浮动盈亏
- **位置**: `src/service/http/handlers.rs:251`
- **问题**: 查询持仓时，profit_long/profit_short 返回 0.0
- **修复**: 使用 qars Position 的 float_profit 字段

#### 2. settlement.rs:100 - 实现日终结算
- **位置**: `src/exchange/settlement.rs:100`
- **问题**: AccountManager 缺少 list_accounts() 方法
- **修复**: 在 AccountManager 中添加 get_all_accounts() 方法（已存在）

#### 3. settlement.rs:180 - 统计手续费
- **位置**: `src/exchange/settlement.rs:180`
- **问题**: 结算时手续费写死为 0.0
- **修复**: 从 TradeRecorder 或账户成交记录中统计当日手续费

#### 4. admin.rs:207 - 下市合约检查
- **位置**: `src/service/http/admin.rs:207`
- **问题**: 下市合约时未检查是否有未平仓持仓
- **修复**: 遍历所有账户检查该合约的持仓

### P1 - 中优先级（功能增强）

#### 5. trade_gateway.rs:253,277 - 订单剩余量
- **位置**: `src/exchange/trade_gateway.rs:253,277`
- **问题**: OrderStatus 通知中 remaining_volume 写死为 0.0
- **修复**: 从 orders 表中查询订单的 volume_left

#### 6. settlement.rs:199 - 强平逻辑
- **位置**: `src/exchange/settlement.rs:199`
- **问题**: 检测到风险账户但未执行强平
- **修复**: 调用 OrderRouter 平掉所有持仓

#### 7. order_router.rs:652 - 撤单实现
- **位置**: `src/exchange/order_router.rs:652`
- **问题**: cancel_order 只更新状态，未从撮合引擎撤单
- **修复**: 调用 matching_engine.cancel_order()

#### 8. trade_gateway.rs:166 - 撮合失败处理
- **位置**: `src/exchange/trade_gateway.rs:166`
- **问题**: 撮合失败只打印日志
- **修复**: 发送 OrderStatus(REJECTED) 通知

### P2 - 低优先级（安全性改进）

#### 9. user_mgr.rs:99 - 密码加密
- **位置**: `src/exchange/user_mgr.rs:99`
- **问题**: 密码明文存储
- **修复**: 使用 bcrypt 加密密码

#### 10. user_mgr.rs:173 - JWT验证
- **位置**: `src/exchange/user_mgr.rs:173`
- **问题**: Token验证过于简单
- **修复**: 使用 jsonwebtoken crate 实现真正的 JWT

---

## 二、前端 TODO/Mock 项

### P0 - 高优先级（后端API已存在，只需调用）

#### 11. monitoring/index.vue:126 - 监控数据
- **位置**: `web/src/views/monitoring/index.vue:126`
- **问题**: loadMonitoringData() 空实现
- **API**: `GET /api/monitoring/system`
- **修复**: 调用 API.monitoring.getSystemMonitoring()

#### 12. admin/instruments.vue:276 - 合约管理
- **位置**: `web/src/views/admin/instruments.vue:276`
- **问题**: 使用 mock 数据
- **API**:
  - `GET /api/admin/instruments` - 查询合约列表
  - `POST /api/admin/instrument/create` - 创建合约
  - `PUT /api/admin/instrument/{id}/update` - 更新合约
  - `PUT /api/admin/instrument/{id}/suspend` - 暂停合约
  - `PUT /api/admin/instrument/{id}/resume` - 恢复合约
  - `DELETE /api/admin/instrument/{id}/delist` - 下市合约
- **修复**: 替换为真实 API 调用

#### 13. admin/settlement.vue:281 - 结算管理
- **位置**: `web/src/views/admin/settlement.vue:281`
- **问题**: 使用 mock 数据
- **API**:
  - `GET /api/admin/settlement/history` - 结算历史
  - `POST /api/admin/settlement/set-price` - 设置结算价
  - `POST /api/admin/settlement/execute` - 执行日终结算
- **修复**: 替换为真实 API 调用

#### 14. admin/risk.vue:296,363,384 - 风控监控
- **位置**: `web/src/views/admin/risk.vue:296,363,384`
- **问题**: 使用 mock 数据
- **API**:
  - `GET /api/management/risk-accounts` - 风险账户
  - `GET /api/management/margin-summary` - 保证金监控
  - `GET /api/management/liquidation-records` - 强平记录
- **修复**: 替换为真实 API 调用

### P1 - 中优先级（需要部分后端支持）

#### 15. user/account-curve.vue:194 - 资金曲线
- **位置**: `web/src/views/user/account-curve.vue:194`
- **问题**: 资金曲线 API 不存在
- **后端**: 需要新增查询账户历史权益的 API
- **修复**:
  1. 后端：添加 `GET /api/account/{user_id}/equity-curve`
  2. 前端：调用新 API

#### 16. trade/components/CloseForm.vue:105 - 可用量
- **位置**: `web/src/views/trade/components/CloseForm.vue:105`
- **问题**: availableVolume 写死为 10
- **修复**: 从父组件传入持仓数据，动态计算可用量

### P2 - 低优先级（增强功能）

#### 17-23. 其他TODO项
- **user/account-curve.vue:378** - Excel 导出
- **chart/index.vue:40** - K线图表初始化
- **admin/risk.vue:237,449** - 持仓明细、强平操作
- **admin/transactions.vue:286** - Excel导出
- **admin/settlement.vue:191,386,435** - 图表展示、CSV导入、详情对话框

---

## 三、修复优先级排序

### 第一批（立即修复）：
1. ✅ handlers.rs - 浮动盈亏计算
2. monitoring/index.vue - 监控数据加载
3. admin/instruments.vue - 合约管理API调用
4. admin/settlement.vue - 结算管理API调用
5. admin/risk.vue - 风控监控API调用

### 第二批（本周完成）：
6. settlement.rs - 日终结算完善
7. admin.rs - 下市合约检查
8. trade_gateway.rs - 订单剩余量
9. trade/CloseForm.vue - 可用量动态获取

### 第三批（下周计划）：
10. settlement.rs - 强平逻辑
11. order_router.rs - 撤单实现
12. user/account-curve.vue - 资金曲线
13. user_mgr.rs - 密码加密 & JWT验证

---

## 四、API 清单（确认已实现）

### 监控类
- ✅ GET /api/monitoring/system
- ✅ GET /api/monitoring/accounts
- ✅ GET /api/monitoring/orders
- ✅ GET /api/monitoring/trades
- ✅ GET /api/monitoring/storage

### 管理类
- ✅ GET /api/admin/instruments
- ✅ POST /api/admin/instrument/create
- ✅ PUT /api/admin/instrument/{id}/update
- ✅ PUT /api/admin/instrument/{id}/suspend
- ✅ PUT /api/admin/instrument/{id}/resume
- ✅ DELETE /api/admin/instrument/{id}/delist
- ✅ POST /api/admin/settlement/set-price
- ✅ POST /api/admin/settlement/batch-set-prices
- ✅ POST /api/admin/settlement/execute
- ✅ GET /api/admin/settlement/history
- ✅ GET /api/admin/settlement/detail/{date}

### 资金管理类
- ✅ GET /api/management/accounts
- ✅ GET /api/management/account/{user_id}
- ✅ POST /api/management/deposit
- ✅ POST /api/management/withdraw
- ✅ GET /api/management/transactions/{user_id}
- ✅ GET /api/management/risk-accounts
- ✅ GET /api/management/margin-summary
- ✅ GET /api/management/liquidation-records

### 待添加API
- ❌ GET /api/account/{user_id}/equity-curve - 资金曲线查询

---

## 五、执行计划

**今天**：
- [x] 完成扫描和计划制定
- [ ] 修复后端 handlers.rs 浮动盈亏
- [ ] 修复前端 monitoring/index.vue
- [ ] 修复前端 admin/instruments.vue

**明天**：
- [ ] 修复前端 admin/settlement.vue
- [ ] 修复前端 admin/risk.vue
- [ ] 修复后端 settlement.rs 日终结算
- [ ] 修复后端 admin.rs 下市检查
