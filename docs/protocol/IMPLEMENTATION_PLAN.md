# 交易所功能扩展实现计划

> @yutiansut @quantaxis
> 版本: v1.0
> 日期: 2025-12-15

---

## 一、总体目标

补充交易所缺失的关键功能，将接口完整度从 87% 提升到 98%。

## 二、功能模块划分

### Phase 1: 高优先级 - 交易核心 (预计 4-5 个文件)

| 功能 | 后端文件 | 前端文件 | 说明 |
|------|----------|----------|------|
| 银期转账 | `src/service/http/transfer.rs` | `views/user/transfer.vue` | 银期转账记录和操作 |
| 条件单 | `src/exchange/conditional_order.rs` | `views/trade/components/ConditionalForm.vue` | 止损止盈条件单 |
| 批量下单 | 扩展 `order_router.rs` + `handlers.rs` | `views/trade/components/BatchOrderForm.vue` | 程序化交易支持 |
| 订单修改 | 扩展 `order_router.rs` | 扩展 `OrderForm.vue` | 价格/数量修改 |

### Phase 2: 中优先级 - 用户体验 (预计 3-4 个文件)

| 功能 | 后端文件 | 前端文件 | 说明 |
|------|----------|----------|------|
| 密码管理 | 扩展 `auth.rs` | `views/user/settings.vue` | 修改/重置密码 |
| 手续费查询 | `src/service/http/fee.rs` | `views/admin/fees.vue` | 手续费率管理 |
| 保证金率管理 | 扩展 `admin.rs` | `views/admin/margin-rates.vue` | 保证金设置 |

### Phase 3: 低优先级 - 运营增强 (预计 2-3 个文件)

| 功能 | 后端文件 | 前端文件 | 说明 |
|------|----------|----------|------|
| 账户冻结/解冻 | 扩展 `management.rs` | 扩展 `admin/accounts.vue` | 风控操作 |
| 审计日志 | `src/service/http/audit.rs` | `views/admin/audit-logs.vue` | 操作记录 |
| 系统公告 | `src/service/http/notification.rs` | `views/notifications.vue` | 通知系统 |

---

## 三、Phase 1 详细设计

### 3.1 银期转账功能

#### 后端接口设计

```rust
// POST /api/account/transfer
pub struct TransferRequest {
    pub account_id: String,
    pub bank_id: String,
    pub amount: f64,        // > 0 转入, < 0 转出
    pub bank_password: String,
    pub future_password: String,
}

// GET /api/account/{account_id}/banks
// 返回签约银行列表

// GET /api/account/{account_id}/transfers
pub struct TransferQuery {
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}
```

#### 数据结构 (QIFI 兼容)

```rust
pub struct Bank {
    pub id: String,
    pub name: String,
}

pub struct Transfer {
    pub id: String,
    pub datetime: i64,
    pub currency: String,
    pub amount: f64,
    pub error_id: i32,
    pub error_msg: String,
}
```

#### 前端页面

- `views/user/transfer.vue` - 银期转账页面
  - 银行选择
  - 转账金额输入
  - 转账记录列表

---

### 3.2 条件单功能

#### 后端接口设计

```rust
// POST /api/order/conditional
pub struct ConditionalOrderRequest {
    pub account_id: String,
    pub instrument_id: String,
    pub direction: String,           // BUY/SELL
    pub offset: String,              // OPEN/CLOSE
    pub volume: f64,
    pub order_type: String,          // LIMIT/MARKET
    pub limit_price: Option<f64>,
    pub condition_type: String,      // STOP_LOSS/TAKE_PROFIT/PRICE_TOUCH
    pub trigger_price: f64,          // 触发价格
    pub trigger_condition: String,   // GE (>=) / LE (<=)
    pub valid_until: Option<i64>,    // 有效期（时间戳）
}

// GET /api/order/conditional/list?account_id=xxx
// 返回条件单列表

// DELETE /api/order/conditional/{conditional_order_id}
// 删除条件单
```

#### 条件单引擎

```rust
// src/exchange/conditional_order.rs
pub struct ConditionalOrderEngine {
    orders: DashMap<String, ConditionalOrder>,
    market_data: Arc<MarketDataCache>,
}

impl ConditionalOrderEngine {
    // 检查条件是否触发
    pub fn check_triggers(&self, tick: &TickData) -> Vec<ConditionalOrder>;

    // 触发后转为普通订单
    pub fn trigger_order(&self, order: ConditionalOrder) -> SubmitOrderRequest;
}
```

#### 前端组件

- `views/trade/components/ConditionalForm.vue` - 条件单表单
  - 条件类型选择 (止损/止盈/触价)
  - 触发价格设置
  - 触发条件 (>=, <=)
  - 有效期设置

---

### 3.3 批量下单功能

#### 后端接口设计

```rust
// POST /api/order/batch
pub struct BatchOrderRequest {
    pub account_id: String,
    pub orders: Vec<SingleOrderRequest>,
}

pub struct SingleOrderRequest {
    pub instrument_id: String,
    pub direction: String,
    pub offset: String,
    pub volume: f64,
    pub price: f64,
    pub order_type: String,
}

// 响应
pub struct BatchOrderResponse {
    pub total: usize,
    pub success: usize,
    pub failed: usize,
    pub results: Vec<SingleOrderResult>,
}

pub struct SingleOrderResult {
    pub index: usize,
    pub success: bool,
    pub order_id: Option<String>,
    pub error: Option<String>,
}

// POST /api/order/batch-cancel
pub struct BatchCancelRequest {
    pub account_id: String,
    pub order_ids: Vec<String>,
}
```

#### 前端组件

- `views/trade/components/BatchOrderForm.vue` - 批量下单表单
  - 表格编辑模式
  - CSV 导入功能
  - 批量结果展示

---

### 3.4 订单修改功能

#### 后端接口设计

```rust
// PUT /api/order/{order_id}
pub struct ModifyOrderRequest {
    pub account_id: String,
    pub new_price: Option<f64>,
    pub new_volume: Option<f64>,
}

// 实现逻辑：撤单 + 新下单（原子操作）
```

#### 前端组件

- 扩展现有 `OrderForm.vue`，添加修改模式

---

## 四、Phase 2 详细设计

### 4.1 密码管理

#### 后端接口设计

```rust
// POST /api/auth/change-password
pub struct ChangePasswordRequest {
    pub old_password: String,
    pub new_password: String,
}

// POST /api/auth/reset-password (管理员)
pub struct ResetPasswordRequest {
    pub user_id: String,
    pub new_password: String,
}
```

#### 前端页面

- `views/user/settings.vue` - 用户设置页面
  - 修改密码
  - 个人信息展示

---

### 4.2 手续费管理

#### 后端接口设计

```rust
// GET /api/admin/commission/{instrument_id}
pub struct CommissionInfo {
    pub instrument_id: String,
    pub open_ratio_by_money: f64,    // 开仓手续费率（按金额）
    pub open_ratio_by_volume: f64,   // 开仓手续费（按手数）
    pub close_ratio_by_money: f64,
    pub close_ratio_by_volume: f64,
    pub close_today_ratio_by_money: f64,  // 平今手续费
    pub close_today_ratio_by_volume: f64,
}

// PUT /api/admin/commission/{instrument_id}
// 修改手续费率
```

---

### 4.3 保证金率管理

#### 后端接口设计

```rust
// GET /api/admin/margin-rate/{instrument_id}
pub struct MarginRateInfo {
    pub instrument_id: String,
    pub long_margin_ratio: f64,   // 多头保证金率
    pub short_margin_ratio: f64,  // 空头保证金率
}

// PUT /api/admin/margin-rate/{instrument_id}
// 修改保证金率
```

---

## 五、Phase 3 详细设计

### 5.1 账户冻结/解冻

```rust
// PUT /api/management/account/{account_id}/freeze
// PUT /api/management/account/{account_id}/unfreeze
```

### 5.2 审计日志

```rust
// GET /api/admin/audit-logs
pub struct AuditLogQuery {
    pub user_id: Option<String>,
    pub action: Option<String>,  // LOGIN/ORDER/TRANSFER/ADMIN
    pub start_time: Option<i64>,
    pub end_time: Option<i64>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

pub struct AuditLog {
    pub id: String,
    pub user_id: String,
    pub action: String,
    pub detail: String,
    pub ip_address: String,
    pub timestamp: i64,
}
```

### 5.3 系统公告

```rust
// POST /api/admin/notifications
pub struct CreateNotificationRequest {
    pub title: String,
    pub content: String,
    pub level: String,  // INFO/WARNING/CRITICAL
    pub valid_until: Option<i64>,
}

// GET /api/notifications
// 获取公告列表
```

---

## 六、WebSocket 协议扩展

### 6.1 新增客户端消息

```json
// 银期转账
{
  "aid": "req_transfer",
  "account_id": "xxx",
  "bank_id": "xxx",
  "amount": 10000.0,
  "currency": "CNY"
}

// 条件单
{
  "aid": "insert_conditional_order",
  "account_id": "xxx",
  "instrument_id": "cu2512",
  "condition_type": "STOP_LOSS",
  "trigger_price": 74000.0,
  "trigger_condition": "LE",
  "direction": "SELL",
  "offset": "CLOSE",
  "volume": 10,
  "price_type": "MARKET"
}
```

### 6.2 新增服务端推送

条件单触发通知将通过 `rtn_data` 的 `notify` 字段推送。

---

## 七、实现顺序

```
Week 1: Phase 1 后端实现
├── Day 1-2: 银期转账后端 + 条件单引擎
├── Day 3-4: 批量下单 + 订单修改
└── Day 5: 集成测试

Week 2: Phase 1 前端实现 + Phase 2 开始
├── Day 1-2: 银期转账页面 + 条件单表单
├── Day 3-4: 批量下单界面 + 订单修改
└── Day 5: 密码管理 + 手续费管理

Week 3: Phase 2 完成 + Phase 3
├── Day 1-2: 保证金管理 + 账户冻结
├── Day 3-4: 审计日志 + 系统公告
└── Day 5: 最终测试 + 文档更新
```

---

## 八、文件修改清单

### 后端新增文件

```
src/service/http/
├── transfer.rs        # 银期转账 API
├── fee.rs             # 手续费 API
├── audit.rs           # 审计日志 API
└── notification.rs    # 系统公告 API

src/exchange/
└── conditional_order.rs  # 条件单引擎
```

### 后端修改文件

```
src/service/http/
├── mod.rs             # 注册新模块
├── routes.rs          # 添加新路由
├── models.rs          # 添加新数据结构
├── handlers.rs        # 添加批量下单、订单修改
├── auth.rs            # 添加密码管理
├── admin.rs           # 添加保证金率管理
└── management.rs      # 添加账户冻结

src/service/websocket/
├── diff_messages.rs   # 添加新消息类型
└── diff_handler.rs    # 处理新消息
```

### 前端新增文件

```
web/src/views/
├── user/
│   ├── transfer.vue    # 银期转账
│   └── settings.vue    # 用户设置
├── trade/components/
│   ├── ConditionalForm.vue   # 条件单表单
│   └── BatchOrderForm.vue    # 批量下单表单
├── admin/
│   ├── fees.vue        # 手续费管理
│   ├── margin-rates.vue # 保证金管理
│   └── audit-logs.vue  # 审计日志
└── notifications.vue   # 系统公告

web/src/api/
└── index.js           # 添加新 API
```

### 前端修改文件

```
web/src/
├── router/index.js    # 添加新路由
├── api/index.js       # 添加新 API 定义
└── views/trade/components/OrderForm.vue  # 添加修改订单功能
```

---

## 九、验收标准

### 功能验收

- [ ] 银期转账：能够完成入金/出金操作，查询转账记录
- [ ] 条件单：能够创建/删除条件单，触发后自动下单
- [ ] 批量下单：能够一次提交多个订单，返回汇总结果
- [ ] 订单修改：能够修改未成交订单的价格和数量
- [ ] 密码管理：能够修改密码，管理员能够重置用户密码
- [ ] 手续费管理：能够查询和修改手续费率
- [ ] 保证金管理：能够查询和修改保证金率
- [ ] 账户冻结：能够冻结/解冻账户
- [ ] 审计日志：能够查询操作日志
- [ ] 系统公告：能够发布和查看公告

### 技术验收

- [ ] 所有新接口有完整的 API 文档
- [ ] 所有新功能有单元测试
- [ ] 前后端联调通过
- [ ] 压力测试通过（批量下单 > 1000 单/秒）

---

**计划审批**: 待确认
