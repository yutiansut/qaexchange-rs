# 文档重组计划

**生成时间**: 2025-10-05
**状态**: 📋 P0任务 - 文档组织和主README更新
**目标**: 解决文档分散问题，建立清晰的导航体系，详细介绍各模块

---

## 📊 当前文档现状分析

### 1. 文档总数统计

| 类别 | 数量 | 文件列表 |
|------|------|----------|
| **核心文档** | 3 | README.md, CLAUDE.md, CHANGELOG.md |
| **快速开始** | 2 | DEPLOYMENT.md, FRONTEND_INTEGRATION.md |
| **架构设计** | 2 | ARCHITECTURE.md, FEATURE_MATRIX.md ⭐ |
| **API参考** | 3 | API_REFERENCE.md, ADMIN_API_REFERENCE.md ⭐, WEBSOCKET_PROTOCOL.md |
| **数据模型** | 2 | DATA_MODELS.md ⭐, ERROR_CODES.md |
| **存储系统** | 7 | storage/*.md |
| **通知系统** | 2 | notification/README.md, SERIALIZATION_GUIDE.md |
| **Phase实现** | 2 | PHASE6_7_IMPLEMENTATION.md, PHASE8_QUERY_ENGINE.md |
| **开发指南** | 3 | DEVELOPMENT.md, TESTING.md, PERFORMANCE.md |
| **文档中心** | 1 | docs/README.md |
| **待办事项** | 5 | todo/*.md |
| **总计** | **32+** | - |

⭐ = 新创建文档（2025-10-05）

### 2. 主README.md问题诊断

**当前版本**: 未标注（内容显示Phase 1-8已完成）
**问题点**:
1. ❌ 未更新到v0.4.0（docs/README.md显示v0.4.0）
2. ❌ 未显示功能完成度统计（95%, 38/41）
3. ❌ API设计标注"(计划)"但实际已实现42个API
4. ❌ 缺少管理端功能介绍（6个管理端页面 + 22个API）
5. ❌ 缺少新文档的导航链接（FEATURE_MATRIX等3个新文档）
6. ❌ 缺少详细的模块功能介绍
7. ❌ 缺少前端系统说明（Vue应用架构）

### 3. 文档分散问题

**分散现象**:
- 存储系统文档在 `docs/storage/`（7个文件）
- 通知系统文档在 `docs/notification/`（1个文件）+ 根目录（1个文件）
- Phase实现文档在根目录（2个文件）
- 开发指南在根目录（3个文件）
- API文档在 `docs/`（3个文件）

**导航问题**:
- 主README.md未提供完整文档导航
- docs/README.md是文档中心，但主README未链接
- 用户难以找到特定文档

---

## 🎯 重组目标

### 核心原则
1. **单一入口**: 主README.md作为项目总入口
2. **分层导航**: 主README → docs/README.md → 子文档
3. **分类清晰**: 按功能模块分类（快速开始/架构/API/开发）
4. **完整更新**: 反映最新功能（v0.4.0, 95%完成度）

### 具体目标
1. ✅ 主README.md更新为完整的项目门户
2. ✅ 所有新文档（FEATURE_MATRIX等）链接到主入口
3. ✅ 详细介绍所有核心模块（8大模块）
4. ✅ 提供前后端功能对照
5. ✅ 更新API状态（已实现vs计划中）

---

## 📁 文档分类体系

### 分类1: 📘 快速开始（Quick Start）
**目标用户**: 新用户、评估者
**文档**:
- README.md（主入口） ⭐ 需更新
- DEPLOYMENT.md（部署指南）
- FRONTEND_INTEGRATION.md（前端对接指南）

**内容要求**:
- 5分钟快速了解项目
- 10分钟运行第一个示例
- 30分钟完成本地部署

---

### 分类2: 🏗️ 系统架构（Architecture）
**目标用户**: 架构师、后端开发者
**文档**:
- docs/ARCHITECTURE.md（系统架构设计） ⭐ 已更新
- docs/FEATURE_MATRIX.md（功能映射矩阵） ⭐ 新文档
- CHANGELOG.md（变更历史） ⭐ 已更新

**内容要点**:
- 分层架构设计（Service/Business/Core/Data/Storage）
- 前后端功能对照（17页面 ↔ 42API）
- 管理端架构（InstrumentRegistry/SettlementEngine/RiskMonitor）
- 数据流图

---

### 分类3: 📡 API参考（API Reference）
**目标用户**: 前端开发者、API集成者
**文档**:
- docs/API_REFERENCE.md（用户端REST API - 10个）
- docs/ADMIN_API_REFERENCE.md（管理端REST API - 25个） ⭐ 新文档
- docs/WEBSOCKET_PROTOCOL.md（WebSocket协议 - 8消息）
- docs/DATA_MODELS.md（数据模型定义） ⭐ 新文档
- docs/ERROR_CODES.md（错误码说明）

**内容特色**:
- 每个API含curl/JS/Python示例
- 完整请求/响应示例
- Rust + TypeScript双语言定义

---

### 分类4: 💾 存储系统（Storage System）
**目标用户**: 存储引擎开发者、性能优化者
**文档**:
- docs/storage/README.md（存储系统概览）
- docs/storage/01_STORAGE_ARCHITECTURE.md（架构设计）
- docs/storage/02_DISTRIBUTION_ARCHITECTURE.md（零拷贝分发）
- docs/storage/03_RECOVERY_DESIGN.md（WAL恢复机制）
- docs/storage/07_HYBRID_OLTP_OLAP_DESIGN.md（OLTP+OLAP双体系）
- docs/storage/06_INTEGRATED_IMPLEMENTATION_PLAN.md（Phase 1-8计划）

**技术要点**:
- WAL + MemTable + SSTable架构
- OLTP (SkipMap) + OLAP (Arrow2)双体系
- Bloom Filter + mmap零拷贝优化
- P99 < 50ms写入，78K entries/sec吞吐

---

### 分类5: 📢 通知系统（Notification System）
**目标用户**: 实时通信开发者
**文档**:
- docs/notification/README.md（通知系统概览）
- docs/SERIALIZATION_GUIDE.md（rkyv零拷贝序列化）

**技术要点**:
- rkyv零拷贝序列化（125x vs JSON）
- ~20ns零拷贝反序列化
- 50M ops/s吞吐

---

### 分类6: 🔄 高可用与查询（HA & Query）
**目标用户**: 分布式系统开发者、分析引擎开发者
**文档**:
- docs/PHASE6_7_IMPLEMENTATION.md（主从复制 + Bloom Filter）
- docs/PHASE8_QUERY_ENGINE.md（Polars查询引擎）

**技术要点**:
- Raft-inspired主从复制（< 10ms复制延迟）
- Polars SQL查询（< 10ms查询延迟）
- Parquet列式扫描（> 1GB/s吞吐）

---

### 分类7: 🛠️ 开发指南（Development）
**目标用户**: 贡献者、二次开发者
**文档**:
- CLAUDE.md（项目约定和开发规范）
- docs/DEVELOPMENT.md（开发环境搭建）
- docs/TESTING.md（测试指南）
- docs/PERFORMANCE.md（性能调优）

**内容要点**:
- 复用qars优先原则
- Rust 1.91.0 + Actix-web开发环境
- 单元测试 + 集成测试策略

---

### 分类8: 📋 文档中心（Documentation Hub）
**目标用户**: 所有用户
**文档**:
- docs/README.md（文档导航中心） ⭐ 已更新

**作用**:
- 统一文档入口
- 分类导航
- 推荐阅读顺序

---

## 📝 主README.md更新计划

### 更新内容清单

#### 1. 版本与状态（第1-10行）
```markdown
# QAEXCHANGE-RS

**版本**: v0.4.0 (管理端功能完善)
**更新日期**: 2025-10-05
**功能完成度**: ✅ 95% (38/41 功能)
**开发状态**: Phase 1-8 已完成，Phase 9-10 计划中

高性能量化交易所系统 - 基于 QARS 核心架构构建
```

#### 2. 快速导航（新增章节）
```markdown
## 📚 快速导航

### 我是...
- **新用户/评估者** → [快速开始](#快速开始) | [核心特性](#核心特性)
- **前端开发者** → [前端对接指南](docs/FRONTEND_INTEGRATION.md) ⭐ | [用户端API](docs/API_REFERENCE.md) | [管理端API](docs/ADMIN_API_REFERENCE.md)
- **后端开发者** → [系统架构](docs/ARCHITECTURE.md) | [开发指南](docs/DEVELOPMENT.md)
- **架构师** → [功能映射矩阵](docs/FEATURE_MATRIX.md) ⭐ | [数据模型](docs/DATA_MODELS.md) ⭐
- **完整文档** → [文档中心](docs/README.md) (60+ 文档)
```

#### 3. 核心特性（更新）
添加管理端功能:
```markdown
✅ **管理端功能**: (v0.4.0 新增)
  - 合约管理: 上市/下市/修改合约 (6 API)
  - 结算管理: 批量设价/执行结算/查询结算 (5 API)
  - 风控管理: 查询风险账户/强平查询 (3 API，部分实现)
  - 系统监控: 存储状态/账户统计/资金汇总 (6 API)
  - 市场数据: 订单簿/成交记录/活跃合约 (5 API)
```

#### 4. 功能完成度统计（新增章节）
```markdown
## 📊 功能完成度

| 模块 | 进度 | 已完成 | 计划 | 说明 |
|------|------|--------|------|------|
| **用户端** | 95% | 9/9 页面 | - | 账户/交易/持仓/历史 |
| **管理端** | 100% | 6/6 页面 | - | 合约/结算/风控/监控 |
| **用户API** | 100% | 20/20 | - | REST API |
| **管理API** | 88% | 22/25 | 3 风控 | 部分风控待实现 |
| **WebSocket** | 100% | 8/8 | - | 实时消息 |
| **总计** | **95%** | **38/41** | **3** | ⭐ 生产可用 |

详见 [功能映射矩阵](docs/FEATURE_MATRIX.md)
```

#### 5. 模块详细介绍（新增章节）
```markdown
## 🧩 核心模块详解

### 1. 交易所核心（Exchange Core）
**位置**: `src/exchange/`
**模块**:
- **AccountManager** (`account_mgr.rs`): 账户生命周期管理
  - 开户/入金/出金/查询
  - 多账户并发访问 (DashMap<String, Arc<RwLock<QA_Account>>>)
  - 账户快照和恢复

- **OrderRouter** (`order_router.rs`): 订单路由与验证
  - 订单接收/验证/路由
  - 盘前风控检查
  - 订单状态追踪

- **TradeGateway** (`trade_gateway.rs`): 成交回报网关
  - 实时成交推送 (WebSocket)
  - 账户更新通知
  - rkyv零拷贝序列化 (125x vs JSON)

- **SettlementEngine** (`settlement.rs`): 日终结算引擎
  - 盯市盈亏计算
  - 手续费结算
  - 强平检测 (风险度 >= 100%)
  - 批量账户结算

- **InstrumentRegistry** (`instrument_registry.rs`): 合约注册表
  - 合约上市/下市
  - 交易时间管理
  - 保证金率配置

**性能**: > 100K orders/sec 订单吞吐, P99 < 100μs 撮合延迟

---

### 2. 撮合引擎（Matching Engine）
**位置**: `src/matching/`
**复用**: 98% 复用 `qars::qamarket::matchengine::Orderbook`
**功能**:
- 价格-时间优先撮合
- 集合竞价 (`auction.rs`)
- 连续交易
- 成交记录 (`trade_recorder.rs`)

**性能**: 基于qars撮合引擎, P99 < 100μs

---

### 3. 存储系统（Storage System）
**位置**: `src/storage/`
**架构**: WAL + MemTable + SSTable (LSM-Tree)
**模块**:
- **WAL** (`wal/`): Write-Ahead Log
  - 崩溃恢复 (CRC32 校验)
  - P99 < 50ms 写入延迟 (HDD)
  - 批量吞吐 > 78K entries/sec

- **MemTable** (`memtable/`): 内存表
  - OLTP: SkipMap (P99 < 10μs 写入)
  - OLAP: Arrow2 列式格式

- **SSTable** (`sstable/`): 持久化表
  - OLTP: rkyv 零拷贝序列化
  - OLAP: Parquet 列式存储
  - Bloom Filter: 1% FP rate, ~100ns 查找
  - mmap Reader: 零拷贝读取 (P99 < 50μs)

- **Compaction** (`compaction/`): 后台压缩
  - Leveled compaction 策略

- **Checkpoint** (`checkpoint/`): 快照管理
  - 账户快照创建/恢复

**文档**: [存储系统概览](docs/storage/README.md)

---

### 4. 查询引擎（Query Engine）✨ Phase 8
**位置**: `src/query/`
**基础**: Polars 0.51 DataFrame
**功能**:
- SQL查询 (SQLContext)
- 结构化查询 (select, filter, aggregate, sort, limit)
- 时间序列查询 (granularity 聚合)
- Parquet 文件扫描

**性能**:
- SQL 查询 (100行): < 10ms
- Parquet 扫描: > 1GB/s
- 聚合查询: < 50ms

**文档**: [Phase 8 查询引擎](docs/PHASE8_QUERY_ENGINE.md)

---

### 5. 主从复制（Replication）✨ Phase 6
**位置**: `src/replication/`
**协议**: Raft-inspired 选主算法
**模块**:
- **LogReplicator** (`log_replicator.rs`): 批量日志复制
- **RoleManager** (`role_manager.rs`): Master/Slave/Candidate 角色管理
- **Heartbeat** (`heartbeat.rs`): 心跳检测

**性能**:
- 复制延迟: < 10ms
- 心跳间隔: 100ms
- 故障切换: < 500ms

**文档**: [Phase 6-7 实现总结](docs/PHASE6_7_IMPLEMENTATION.md)

---

### 6. 风控系统（Risk Management）
**位置**: `src/risk/`
**功能**:
- 盘前风控检查 (`pre_trade_check.rs`)
  - 资金充足性检查
  - 持仓限额检查
  - 自成交防范
- 实时风险监控
  - 风险度计算 (margin_used / balance)
  - 强平触发 (risk >= 100%)

**集成**: OrderRouter → PreTradeCheck → MatchingEngine

---

### 7. 服务层（Service Layer）
**位置**: `src/service/`
**模块**:
- **HTTP Server** (`http/`): REST API
  - 用户端: 10 API (账户/订单/持仓/历史)
  - 管理端: 25 API (合约/结算/风控/监控/市场)
  - Actix-web 4.4 框架

- **WebSocket Server** (`websocket/`): 实时通信
  - 交易通道 (下单/撤单/成交回报)
  - 行情通道 (订单簿/逐笔成交)
  - 心跳机制 (10s 超时)

**文档**:
- [用户端API参考](docs/API_REFERENCE.md)
- [管理端API参考](docs/ADMIN_API_REFERENCE.md)
- [WebSocket协议](docs/WEBSOCKET_PROTOCOL.md)

---

### 8. 通知系统（Notification System）
**位置**: `src/notification/`
**技术**: rkyv 零拷贝序列化
**性能**:
- 序列化: 125x faster than JSON
- 反序列化: ~20ns (零拷贝)
- 吞吐: 50M ops/s

**使用场景**:
- 成交通知 (Trade)
- 订单状态 (OrderStatus)
- 账户更新 (AccountUpdate)
- 订单簿快照 (OrderBook)

**文档**: [序列化指南](docs/SERIALIZATION_GUIDE.md)

---

### 9. 前端应用（Frontend Application）
**位置**: `web/`
**技术栈**: Vue 2.6.11 + Element UI + vxe-table + ECharts
**功能**:

**用户端页面** (9个):
- 登录页 (`login.vue`)
- 账户管理 (`account.vue`) - 开户/入金/出金/查询
- 下单页 (`trade.vue`) - 下单/撤单
- 持仓查询 (`positions.vue`) - 实时持仓
- 订单查询 (`orders.vue`) - 历史订单
- 成交查询 (`trades.vue`) - 成交记录
- 实时行情 (`market.vue`) - WebSocket行情
- 账户历史 (`history.vue`) - 历史记录
- 风险监控 (`risk-monitor.vue`) - 风险指标

**管理端页面** (6个):
- 仪表盘 (`admin/dashboard.vue`) - 系统概览
- 合约管理 (`admin/instruments.vue`) - 上市/下市/修改
- 结算管理 (`admin/settlement.vue`) - 日终结算
- 风控管理 (`admin/risk.vue`) - 风险账户监控
- 系统监控 (`admin/monitoring.vue`) - 存储/性能监控
- 账户管理 (`admin/accounts.vue`) - 账户列表

**文档**: [前端对接指南](docs/FRONTEND_INTEGRATION.md)

---

### 📊 模块依赖关系

```
┌─────────────────────────────────────────────┐
│              前端应用 (Vue)                  │
│    用户端(9页面) + 管理端(6页面)              │
└──────────────┬──────────────────────────────┘
               │
        ┌──────┴───────┐
        │              │
   HTTP REST      WebSocket
        │              │
        └──────┬───────┘
               │
┌──────────────▼──────────────────────────────┐
│          Service Layer (服务层)              │
│   HTTP Server + WebSocket Server            │
└──────────────┬──────────────────────────────┘
               │
┌──────────────▼──────────────────────────────┐
│        Business Layer (业务层)               │
│  OrderRouter → PreTradeCheck → Gateway      │
└──────────────┬──────────────────────────────┘
               │
┌──────────────▼──────────────────────────────┐
│          Core Layer (核心层)                 │
│  AccountManager + MatchingEngine + Registry │
└──────────────┬──────────────────────────────┘
               │
┌──────────────▼──────────────────────────────┘
│      Data Layer (数据层 - 复用qars)          │
│  QA_Account + QAOrder + Orderbook           │
└──────────────┬──────────────────────────────┘
               │
┌──────────────▼──────────────────────────────┐
│      Storage Layer (存储层)                  │
│  WAL → MemTable → SSTable → Compaction      │
└──────────────┬──────────────────────────────┘
               │
         ┌─────┴─────┐
         │           │
   Replication   QueryEngine
   (Phase 6)     (Phase 8)
```
```

#### 6. API状态更新（修改现有章节）
```markdown
## 📡 API 概览

### 用户端 API (20个) ✅ 已实现
**账户管理** (4个)
- POST `/api/account/open` - 开户
- POST `/api/account/deposit` - 入金
- POST `/api/account/withdraw` - 出金
- GET `/api/account/{user_id}` - 查询账户

**订单管理** (4个)
- POST `/api/order/submit` - 下单
- POST `/api/order/cancel` - 撤单
- GET `/api/order/{order_id}` - 查询订单
- GET `/api/order/user/{user_id}` - 列出用户订单

**持仓/其他** (12个)
- 详见 [用户端API参考](docs/API_REFERENCE.md)

---

### 管理端 API (25个) ⭐ v0.4.0 新增
**合约管理** (6个)
- GET `/admin/instruments` - 查询所有合约
- POST `/admin/instrument/create` - 上市合约
- PUT `/admin/instrument/{id}` - 修改合约
- DELETE `/admin/instrument/{id}` - 下市合约 (含持仓检查)
- ...

**结算管理** (5个)
- POST `/admin/settlement/set-price` - 设置结算价
- POST `/admin/settlement/batch-set-prices` - 批量设价
- POST `/admin/settlement/execute` - 执行日终结算
- GET `/admin/settlement/history` - 结算历史
- GET `/admin/settlement/result/{date}` - 结算结果

**系统监控** (6个)
- GET `/admin/monitoring/storage` - 存储状态
- GET `/admin/monitoring/accounts` - 账户统计
- GET `/admin/monitoring/capital` - 资金汇总
- ...

**详见**: [管理端API参考](docs/ADMIN_API_REFERENCE.md) ⭐

---

### WebSocket 协议 (8消息类型) ✅ 已实现
- Auth / Subscribe / SubmitOrder / CancelOrder
- Trade / OrderStatus / AccountUpdate / OrderBook

**详见**: [WebSocket协议文档](docs/WEBSOCKET_PROTOCOL.md)
```

#### 7. 文档导航（新增章节，放在文档末尾）
```markdown
## 📚 完整文档导航

### 快速开始
- [主文档 README.md](.) - 项目概览 ⭐ 当前文档
- [前端对接指南](docs/FRONTEND_INTEGRATION.md) - 前端开发者必读
- [部署指南](docs/DEPLOYMENT.md) - 快速部署

### 架构与设计
- [系统架构](docs/ARCHITECTURE.md) - 完整架构设计 (含管理端)
- [功能映射矩阵](docs/FEATURE_MATRIX.md) ⭐ 前后端功能对照
- [数据模型](docs/DATA_MODELS.md) ⭐ Rust + TypeScript定义

### API 参考
- [用户端API](docs/API_REFERENCE.md) - 20个REST API
- [管理端API](docs/ADMIN_API_REFERENCE.md) ⭐ 25个REST API
- [WebSocket协议](docs/WEBSOCKET_PROTOCOL.md) - 8个消息类型
- [错误码说明](docs/ERROR_CODES.md) - 所有错误码

### 存储与查询
- [存储系统概览](docs/storage/README.md) - WAL + MemTable + SSTable
- [查询引擎](docs/PHASE8_QUERY_ENGINE.md) - Polars SQL查询
- [主从复制](docs/PHASE6_7_IMPLEMENTATION.md) - 高可用设计

### 开发指南
- [开发规范](CLAUDE.md) - 项目约定 (复用qars优先)
- [开发环境](docs/DEVELOPMENT.md) - 环境搭建
- [测试指南](docs/TESTING.md) - 单元测试/集成测试

### 其他
- [变更日志](CHANGELOG.md) - 版本历史
- [文档中心](docs/README.md) - 完整文档索引 (60+ 文档)

⭐ = v0.4.0 新增文档
```

---

## 🔄 执行步骤

### Step 1: 更新主README.md ⏰ 预计45分钟
**修改文件**: `/home/quantaxis/qaexchange-rs/README.md`
**修改内容**:
1. ✅ 更新版本和状态 (v0.4.0, 95%完成度)
2. ✅ 添加快速导航章节
3. ✅ 更新核心特性 (添加管理端功能)
4. ✅ 添加功能完成度统计表格
5. ✅ 添加核心模块详解章节 (9个模块)
6. ✅ 更新API概览 (标注已实现vs计划)
7. ✅ 添加完整文档导航章节

**验证**:
- [ ] 所有新文档链接正确
- [ ] 所有模块介绍准确
- [ ] 功能统计与FEATURE_MATRIX.md一致

---

### Step 2: 验证文档链接 ⏰ 预计15分钟
**检查清单**:
- [ ] 主README.md → docs/README.md
- [ ] 主README.md → 各API文档
- [ ] 主README.md → 各架构文档
- [ ] docs/README.md → 所有子文档
- [ ] FEATURE_MATRIX.md中的文档引用

**工具**: 手动检查或使用markdown link checker

---

### Step 3: 更新CHANGELOG.md ⏰ 预计10分钟
**添加内容**:
```markdown
### [v0.4.0] - 2025-10-05

#### Changed
- **文档**: 重组文档体系，更新主README.md
  - 添加功能完成度统计 (95%, 38/41)
  - 添加9大核心模块详解
  - 添加快速导航和完整文档索引
  - 更新API状态（标注已实现vs计划）
```

---

### Step 4: 创建文档索引验证报告 ⏰ 预计10分钟
**生成文件**: `todo/DOCUMENT_INDEX_VERIFICATION.md`
**内容**:
- 所有文档列表（32+个）
- 链接验证结果
- 分类验证结果
- 遗漏文档检查

---

## 📊 时间预算

| 步骤 | 预计时间 | 责任人 |
|------|----------|--------|
| Step 1: 更新主README.md | 45分钟 | Claude |
| Step 2: 验证文档链接 | 15分钟 | Claude |
| Step 3: 更新CHANGELOG | 10分钟 | Claude |
| Step 4: 创建验证报告 | 10分钟 | Claude |
| **总计** | **1小时20分钟** | - |

---

## ✅ 成功标准

1. **主README.md完整性**
   - ✅ 包含v0.4.0版本信息
   - ✅ 显示95%功能完成度
   - ✅ 包含9大模块详解（每个模块 > 100字介绍）
   - ✅ 包含完整文档导航
   - ✅ 标注所有新文档（⭐标记）

2. **文档可发现性**
   - ✅ 任何类型用户都能在5秒内找到入口
   - ✅ 从主README可到达所有重要文档
   - ✅ 分类清晰（8大类）

3. **文档准确性**
   - ✅ 功能统计与FEATURE_MATRIX一致
   - ✅ API状态与实际代码一致
   - ✅ 模块介绍与ARCHITECTURE一致

4. **用户体验**
   - ✅ 新用户: 5分钟了解项目
   - ✅ 前端开发: 直达API文档
   - ✅ 后端开发: 直达架构设计
   - ✅ 架构师: 直达功能映射

---

## 📋 待解决问题

### 问题1: 部分文档未列出
**现象**: todo/下有5个文档未在导航中
**建议**:
- DOCUMENTATION_AUDIT_PLAN.md → 仅供内部参考
- TODO_FIX_COMPLETION_REPORT.md → 归档到 docs/development/
- DOCUMENTATION_UPDATE_SUMMARY.md → 归档
- DOCUMENTATION_PROGRESS_REPORT.md → 归档

### 问题2: 风控API部分实现
**现象**: 3个风控API标注TODO
**建议**:
- 在FEATURE_MATRIX中明确标注 "🔄 部分实现"
- 在ADMIN_API_REFERENCE中标注 "⚠️ 未实现"

---

**计划生成时间**: 2025-10-05
**计划执行者**: Claude
**预计完成时间**: 2025-10-05 (1.5小时内)
