# 管理端 API 集成指南

## 📋 概述

本文档说明如何集成已实现的管理端 API 功能到 qaexchange-rs 服务中。

**已完成的工作**:
- ✅ 合约管理业务逻辑层（扩展 `InstrumentRegistry`）
- ✅ 结算管理业务逻辑层（扩展 `SettlementEngine`）
- ✅ 管理端 HTTP API 处理器（`src/service/http/admin.rs`）

**待集成**:
- ⏳ 在 `main.rs` 中配置 `AdminAppState`
- ⏳ 启用管理端路由
- ⏳ 风控监控 API（可选）

---

## 🔧 集成步骤

### 步骤 1：理解新架构

#### 业务逻辑层
```
src/exchange/
├── instrument_registry.rs   ← 扩展完成（合约生命周期管理）
├── settlement.rs             ← 扩展完成（结算历史查询）
└── account_mgr.rs            ← 已有（无需修改）
```

#### HTTP API 层
```
src/service/http/
├── admin.rs                  ← 新增（管理端 API 处理器）
├── routes.rs                 ← 已更新（注释掉的路由）
└── mod.rs                    ← 已更新（导入 admin 模块）
```

---

### 步骤 2：修改 `src/main.rs`

#### 2.1 导入 `AdminAppState`

在 `main.rs` 顶部添加导入：

```rust
use qaexchange::service::http::admin::AdminAppState;
use qaexchange::exchange::SettlementEngine;
```

#### 2.2 扩展 `ExchangeServer` 结构

在 `ExchangeServer` 结构中添加 `settlement_engine`：

```rust
struct ExchangeServer {
    config: ExchangeConfig,
    account_mgr: Arc<AccountManager>,
    matching_engine: Arc<ExchangeMatchingEngine>,
    instrument_registry: Arc<InstrumentRegistry>,
    trade_gateway: Arc<TradeGateway>,
    order_router: Arc<OrderRouter>,
    market_broadcaster: Arc<MarketDataBroadcaster>,

    // 新增：结算引擎
    settlement_engine: Arc<SettlementEngine>,
}
```

#### 2.3 初始化 `SettlementEngine`

在 `ExchangeServer::new()` 方法中初始化：

```rust
fn new(config: ExchangeConfig) -> Self {
    log::info!("Initializing Exchange Server...");

    // 现有组件初始化...
    let account_mgr = Arc::new(AccountManager::new());
    let matching_engine = Arc::new(ExchangeMatchingEngine::new());
    let instrument_registry = Arc::new(InstrumentRegistry::new());
    let trade_gateway = Arc::new(TradeGateway::new(account_mgr.clone()));
    let market_broadcaster = Arc::new(MarketDataBroadcaster::new());

    // 新增：结算引擎
    let settlement_engine = Arc::new(SettlementEngine::new(account_mgr.clone()));

    // ...省略其他代码

    Self {
        config,
        account_mgr,
        matching_engine,
        instrument_registry,
        trade_gateway,
        order_router,
        market_broadcaster,
        settlement_engine,  // 新增
    }
}
```

#### 2.4 修改 HTTP 服务器启动

找到 HTTP 服务器启动部分（通常在 `run()` 或 `start_http_server()` 方法中）：

```rust
// 创建 AdminAppState
let admin_state = Arc::new(AdminAppState {
    instrument_registry: self.instrument_registry.clone(),
    settlement_engine: self.settlement_engine.clone(),
    account_mgr: self.account_mgr.clone(),
});

// 启动 HTTP 服务器
ActixHttpServer::new(move || {
    App::new()
        .app_data(web::Data::new(app_state.clone()))
        .app_data(web::Data::new(market_service.clone()))
        .app_data(web::Data::new(admin_state.clone()))  // 新增
        // ... 省略其他配置
        .configure(routes::configure)
})
.bind(&self.config.http_address)?
.run()
.await
```

---

### 步骤 3：启用管理端路由

#### 3.1 取消注释路由配置

编辑 `src/service/http/routes.rs`，取消注释管理端路由：

```rust
// 删除 TODO 注释，取消注释以下路由：
.service(
    web::scope("/api/admin")
        // 合约管理
        .route("/instruments", web::get().to(admin::get_all_instruments))
        .route("/instrument/create", web::post().to(admin::create_instrument))
        .route("/instrument/{id}/update", web::put().to(admin::update_instrument))
        .route("/instrument/{id}/suspend", web::put().to(admin::suspend_instrument))
        .route("/instrument/{id}/resume", web::put().to(admin::resume_instrument))
        .route("/instrument/{id}/delist", web::delete().to(admin::delist_instrument))
        // 结算管理
        .route("/settlement/set-price", web::post().to(admin::set_settlement_price))
        .route("/settlement/batch-set-prices", web::post().to(admin::batch_set_settlement_prices))
        .route("/settlement/execute", web::post().to(admin::execute_settlement))
        .route("/settlement/history", web::get().to(admin::get_settlement_history))
        .route("/settlement/detail/{date}", web::get().to(admin::get_settlement_detail))
);
```

#### 3.2 添加 admin 模块引用

在 `routes.rs` 顶部添加：

```rust
use super::admin;
```

---

### 步骤 4：编译和测试

#### 4.1 编译检查

```bash
cd /home/quantaxis/qaexchange-rs
cargo check --lib
```

如果有编译错误，根据提示修复。

#### 4.2 运行服务器

```bash
cargo run --bin qaexchange-server
```

#### 4.3 测试 API

**获取所有合约**:
```bash
curl http://127.0.0.1:8094/api/admin/instruments
```

**创建新合约**:
```bash
curl -X POST http://127.0.0.1:8094/api/admin/instrument/create \
  -H "Content-Type: application/json" \
  -d '{
    "instrument_id": "IF2501",
    "instrument_name": "沪深300股指期货2501",
    "instrument_type": "index_future",
    "exchange": "CFFEX",
    "contract_multiplier": 300,
    "price_tick": 0.2,
    "margin_rate": 0.12,
    "commission_rate": 0.0001,
    "limit_up_rate": 0.1,
    "limit_down_rate": 0.1,
    "list_date": "2024-09-16",
    "expire_date": "2025-01-17"
  }'
```

**设置结算价**:
```bash
curl -X POST http://127.0.0.1:8094/api/admin/settlement/set-price \
  -H "Content-Type: application/json" \
  -d '{
    "instrument_id": "IF2501",
    "settlement_price": 3856.8
  }'
```

**执行日终结算**:
```bash
curl -X POST http://127.0.0.1:8094/api/admin/settlement/execute
```

**获取结算历史**:
```bash
curl http://127.0.0.1:8094/api/admin/settlement/history
```

---

## 📝 API 文档

### 合约管理 API

#### 1. 获取所有合约
```
GET /api/admin/instruments
```

**响应**:
```json
{
  "success": true,
  "data": [
    {
      "instrument_id": "IF2501",
      "instrument_name": "沪深300股指期货2501",
      "instrument_type": "index_future",
      "exchange": "CFFEX",
      "contract_multiplier": 300,
      "price_tick": 0.2,
      "margin_rate": 0.12,
      "commission_rate": 0.0001,
      "limit_up_rate": 0.1,
      "limit_down_rate": 0.1,
      "status": "active",
      "list_date": "2024-09-16",
      "expire_date": "2025-01-17",
      "created_at": "2025-10-04 12:00:00",
      "updated_at": "2025-10-04 12:00:00"
    }
  ],
  "error": null
}
```

#### 2. 创建/上市新合约
```
POST /api/admin/instrument/create
```

**请求体**: 见上述测试示例

**响应**:
```json
{
  "success": true,
  "data": null,
  "error": null
}
```

#### 3. 更新合约信息
```
PUT /api/admin/instrument/{instrument_id}/update
```

**请求体**:
```json
{
  "margin_rate": 0.15,
  "commission_rate": 0.0002
}
```

#### 4. 暂停交易
```
PUT /api/admin/instrument/{instrument_id}/suspend
```

#### 5. 恢复交易
```
PUT /api/admin/instrument/{instrument_id}/resume
```

#### 6. 下市合约
```
DELETE /api/admin/instrument/{instrument_id}/delist
```

---

### 结算管理 API

#### 1. 设置结算价
```
POST /api/admin/settlement/set-price
```

**请求体**:
```json
{
  "instrument_id": "IF2501",
  "settlement_price": 3856.8
}
```

#### 2. 批量设置结算价
```
POST /api/admin/settlement/batch-set-prices
```

**请求体**:
```json
{
  "prices": [
    {"instrument_id": "IF2501", "settlement_price": 3856.8},
    {"instrument_id": "IH2501", "settlement_price": 2345.6}
  ]
}
```

#### 3. 执行日终结算
```
POST /api/admin/settlement/execute
```

**响应**:
```json
{
  "success": true,
  "data": {
    "settlement_date": "2025-10-04",
    "total_accounts": 100,
    "settled_accounts": 98,
    "failed_accounts": 2,
    "force_closed_accounts": ["user009", "user010"],
    "total_commission": 12500.0,
    "total_profit": 580000.0
  },
  "error": null
}
```

#### 4. 获取结算历史
```
GET /api/admin/settlement/history
```

#### 5. 获取结算详情
```
GET /api/admin/settlement/detail/{date}
```

---

## 🔍 故障排查

### 问题 1：编译错误 "AdminAppState not found"

**原因**: 未导入 AdminAppState

**解决**: 在 `main.rs` 中添加：
```rust
use qaexchange::service::http::admin::AdminAppState;
```

### 问题 2：运行时错误 "No data for AdminAppState"

**原因**: 未在 Actix App 中注册 AdminAppState

**解决**: 确保在 `App::new()` 中添加：
```rust
.app_data(web::Data::new(admin_state.clone()))
```

### 问题 3：404 错误

**原因**: 路由未启用

**解决**: 取消注释 `routes.rs` 中的管理端路由配置

---

## 🚀 下一步

### 可选扩展

#### 1. 风控监控 API
创建 `src/risk/risk_monitor.rs`：
- 实时风险账户查询
- 强平记录
- 风险预警

#### 2. 权限控制
添加 JWT Token 验证：
- 管理员权限检查
- API 访问日志
- Rate Limiting

#### 3. 数据持久化
将结算历史保存到数据库：
- MongoDB 集成
- 结算记录归档
- 数据恢复

---

## 📚 相关文档

- [前端管理端页面](../web/ENHANCEMENT_PLAN.md)
- [QIFI 数据格式](../web/src/utils/qifi.js)
- [后端架构说明](../CLAUDE.md)

---

**最后更新**: 2025-10-04
**状态**: ✅ 业务逻辑和 API 已完成，待集成到 main.rs
