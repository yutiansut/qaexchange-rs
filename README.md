# QAEXCHANGE-RS

**版本**: v1.5.0 (生产就绪版)

**更新日期**: 2025-12-21

**功能完成度**: ✅ 100% (Phase 1-13 已完成)

**开发状态**: 生产就绪，持续优化中

高性能量化交易所系统 - 基于 QARS 核心架构构建

---

## 📸 系统截图

### 仪表盘与监控
![Dashboard](docs/pics/dashboard.png)
![Monitor](docs/pics/monitor.png)

### 账户管理
![Accounts](docs/pics/accounts.png)
![Accounts History](docs/pics/accountshistory.png)

### 交易功能
![Trading](docs/pics/trading.png)
![Order](docs/pics/order.png)
![Batch Order](docs/pics/batchorder.png)
![Conditional Order](docs/pics/conditionalorder.png)

### 持仓与风控
![Holding](docs/pics/holding.png)
![Risk](docs/pics/risk.png)

### K线与行情
![KLine](docs/pics/kline.png)

### 历史记录
![Trade History](docs/pics/tradehistory.png)
![Transfer](docs/pics/transfer.png)

---

## 🆕 v1.5.0 更新亮点

### 新增功能
- **二级索引系统** ✨: 时间序列/合约/记录类型三级索引，查询延迟 < 10μs
- **智能压缩策略** ✨: ZSTD/LZ4/Snappy 按数据类型自动选择，压缩率提升 40%
- **因子 WAL 持久化** ✨: 异步持久化 > 200K ops/s，崩溃恢复 < 500ms
- **TLS 安全通信** 🔒: 主从复制支持 TLS 1.3 加密
- **可观测性增强** 📊: Prometheus 指标 + OpenTelemetry 追踪
- **2025-2026 技术路线图** 📋: 完整的演进计划

### 改进优化
- **Clippy 警告清零**: qaexchange-rs 源码无 Clippy 警告
- **环境变量配置化**: ADMIN_TOKEN、WebSocket 超时等支持环境变量
- **测试覆盖增强**: 879 测试通过，覆盖率持续提升
- **文档体系完善**: 70+ 文档，新增安全指南、可观测性指南

---

## 📚 快速导航

### 我是...
- **新用户/评估者** → [快速开始](#快速开始) | [核心特性](#核心特性) | [功能完成度](#-功能完成度)
- **前端开发者** → [前端对接指南](docs/05_integration/frontend/integration_guide.md) ⭐ | [用户端API](docs/04_api/http/user_api.md) | [管理端API](docs/04_api/http/admin_api.md) ⭐
- **后端开发者** → [系统架构](docs/02_architecture/system_overview.md) | [开发指南](docs/06_development/README.md) | [核心模块详解](#-核心模块详解)
- **架构师** → [功能映射矩阵](docs/07_reference/feature_matrix.md) ⭐ | [数据模型](docs/02_architecture/data_models.md) ⭐ | [系统架构](docs/02_architecture/system_overview.md)
- **运维人员** → [TLS 安全指南](docs/06_development/tls_security_guide.md) 🆕 | [可观测性指南](docs/06_development/observability_guide.md) 🆕 | [部署指南](docs/06_development/deployment.md)
- **完整文档** → [文档中心](docs/README.md) (70+ 文档)

⭐ = v0.4.0 新增文档 | ✨ = v1.5.0 新增功能 | 🆕 = 最新文档

---

## 项目概述

`qaexchange-rs` 是一个完整的交易所系统实现，支持万级账户并发交易，提供真实交易所级别的功能。

### 核心特性

✅ **用户管理**: Phase 10 完整实现
  - JWT Token 认证 (HS256, 24小时有效期)
  - bcrypt 密码加密 (12轮成本)
  - 用户注册/登录/注销
  - WebSocket JWT 认证
  - 用户-账户绑定关系
  - WAL 持久化和恢复

✅ **账户管理**: 开户/入金/出金/查询 (复用 qars::qaaccount)

✅ **订单系统**: 下单/撤单/订单路由 (复用 qars::QAOrder)

✅ **撮合引擎**: 价格时间优先/集合竞价/连续交易 (复用 qars::matchengine)

✅ **成交回报**: 实时成交推送/账户更新 (rkyv 零拷贝序列化)

✅ **通知系统**: 零延迟消息推送/多订阅者支持 (基于 rkyv)

✅ **存储系统**: WAL + MemTable + SSTable 混合架构
  - WAL: 崩溃恢复，P99 < 50ms 写入延迟
  - MemTable: SkipMap OLTP / Arrow2 OLAP 双体系
  - SSTable: rkyv 零拷贝 / Parquet 列式存储
  - Compaction: 分层压缩策略
  - Bloom Filter: 1% 误报率高效查找
  - **二级索引** ✨: 时间/合约/类型三级索引 (Phase 11)
  - **智能压缩** ✨: ZSTD/LZ4/Snappy 按类型选择 (Phase 11)

✅ **因子系统**: 流批统一因子引擎 ✨ Phase 11
  - 增量计算引擎 (RollingMean, EMA, RSI...)
  - DAG 依赖图管理
  - 状态检查点与恢复
  - **WAL 异步持久化** ✨ (> 200K ops/s)

✅ **主从复制**: Raft-inspired 自动故障转移
  - 批量日志复制 (< 10ms 延迟)
  - 心跳检测 (100ms 间隔)
  - 自动选主 (< 500ms 故障切换)
  - **TLS 加密** 🔒 (Phase 12)
  - **节点监控** 📊 (CPU/内存/磁盘)

✅ **查询引擎**: Polars DataFrame 分析引擎 (Phase 8)
  - SQL 查询支持
  - 结构化查询 (select, filter, aggregate)
  - 时间序列查询 (granularity 聚合)
  - Parquet 列式扫描 (> 1GB/s 吞吐)

✅ **行情推送系统**: Phase 9 完善
  - WAL 行情持久化 (TickData, OrderBookSnapshot, OrderBookDelta)
  - L1 缓存优化 (DashMap, 100ms TTL, < 10μs 查询)
  - WebSocket 批量推送 (100条/批, 背压控制)
  - 崩溃恢复机制 (< 5s 恢复时间)
  - crossbeam 高性能分发

✅ **市场快照生成器**: Phase 9 新增
  - 每秒级别市场快照生成 (35+ 字段)
  - OHLC 自动计算 (日内高开低收)
  - 买卖五档行情 (实时订单簿深度)
  - 成交统计 (累计成交量/成交额)
  - 零拷贝订阅 (crossbeam channel, 无限订阅者)

✅ **结算系统**: 日终结算/盯市盈亏/强平处理
  - 完整集合竞价算法 (最大成交量原则 + tie-breaking)
  - 自动强平逻辑 (风险比率 >= 100%)
  - 结算价设置和账户清算

✅ **风控系统**: 盘前风控/持仓限额/自成交防范
  - 并发下单竞态条件修复
  - 自成交防范 (同账户对手单检测)
  - 从撮合引擎撤单 (完整撤单流程)
  - 订阅过滤机制 (减少网络流量)

✅ **对外服务**: WebSocket + HTTP API (Actix-web)
  - DIFF 协议完整实现 (登录/行情/下单/撤单/K线)
  - WebSocket JWT 认证
  - **环境变量配置化** ✨ (超时、认证等)

✅ **管理端功能**: v0.4.0 完善
  - **合约管理**: 上市/下市/修改合约 (6 API)
  - **结算管理**: 批量设价/执行结算/查询结算 (5 API)
  - **风控管理**: 查询风险账户/强平查询 (3 API)
  - **系统监控**: 存储状态/账户统计/资金汇总 (6 API)
  - **市场数据**: 订单簿/成交记录/活跃合约 (5 API)

✅ **可观测性**: 🆕 Phase 12 新增
  - Prometheus 指标导出
  - OpenTelemetry 分布式追踪
  - 结构化日志 (tracing + tracing-subscriber)
  - Grafana 仪表盘模板

## 快速开始

### 编译项目

```bash
cd /home/quantaxis/qaexchange-rs

# 编译库
cargo build --lib

# 编译服务器
cargo build --bin qaexchange-server

# 编译所有示例
cargo build --examples
```

### 运行示例

```bash
# 运行账户开户示例
cargo run --example start_exchange

# 预期输出:
# === QAEXCHANGE Demo ===
#
# ✓ Account opened: demo_user
#   Balance: 1000000
#   Available: 1000000
#
# Demo completed.
```

### 启动服务器

```bash
# 基础启动
cargo run --bin qaexchange-server

# 使用环境变量配置
QAEXCHANGE_ADMIN_TOKEN=your_secure_token \
QAEXCHANGE_WS_HEARTBEAT_SECS=5 \
QAEXCHANGE_WS_TIMEOUT_SECS=10 \
cargo run --bin qaexchange-server
```

---

## 📊 功能完成度

### 开发阶段完成情况 (v1.5.0)

| Phase | 功能 | 状态 | 说明 |
|-------|------|------|------|
| Phase 1-2 | 核心交易 + 存储基础 | ✅ | WAL/MemTable/SSTable |
| Phase 3-5 | 压缩/IPC/检查点 | ✅ | Compaction/iceoryx2/Recovery |
| Phase 6-7 | 主从复制 + 性能优化 | ✅ | Raft/Bloom/mmap |
| Phase 8 | 查询引擎 | ✅ | Polars DataFrame |
| Phase 9 | 行情推送增强 | ✅ | L1 Cache/WAL/Recovery |
| Phase 10 | 用户管理 | ✅ | JWT/bcrypt/用户系统 |
| **Phase 11** | **性能优化 II** | ✅ | **二级索引/压缩/因子WAL** ✨ |
| **Phase 12** | **生产就绪** | ✅ | **TLS/可观测性/监控** 🆕 |
| **Phase 13** | **网络层** | 🚧 | gRPC 实现中 |

### 模块完成度

| 模块 | 进度 | 已完成 | 待实现 | 说明 |
|------|------|--------|--------|------|
| **用户管理** | 100% | Phase 10 | - | JWT/bcrypt/用户注册登录 |
| **用户端页面** | 100% | 9/9 | - | 账户/交易/持仓/历史/风险监控 |
| **管理端页面** | 100% | 6/6 | - | 合约/结算/风控/监控/仪表盘 |
| **用户端API** | 100% | 20/20 | - | REST API (账户/订单/持仓/历史) |
| **管理端API** | 100% | 25/25 | - | 合约/结算/风控/监控/市场 |
| **WebSocket** | 100% | 8/8 | - | 实时消息 + JWT认证 |
| **行情推送** | 100% | 4/4 | - | WAL持久化/L1缓存/批量推送/恢复 |
| **存储系统** | 100% | Phase 1-11 | - | WAL/MemTable/SSTable/Index/Compression |
| **因子系统** ✨ | 100% | Phase 11 | - | 流批统一/DAG/WAL持久化 |
| **可观测性** 🆕 | 100% | Phase 12 | - | Prometheus/OpenTelemetry/Grafana |
| **总计** | **100%** | **Phase 1-12** | **Phase 13** | ⭐ 生产就绪 |

**详细功能映射**: [功能映射矩阵](docs/07_reference/feature_matrix.md) ⭐

---

## 项目架构

```
qaexchange-rs/
├── src/
│   ├── lib.rs                    # 库入口
│   ├── main.rs                   # 服务器入口
│   │
│   ├── core/                     # 核心模块 (复用 qars)
│   │   ├── account_ext.rs        # 账户扩展
│   │   └── order_ext.rs          # 订单扩展
│   │
│   ├── matching/                 # 撮合引擎
│   │   ├── engine.rs             # 撮合引擎封装
│   │   ├── auction.rs            # 集合竞价
│   │   └── trade_recorder.rs     # 成交记录器
│   │
│   ├── user/                     # 用户管理系统 (Phase 10)
│   │   ├── mod.rs                # 用户实体和请求/响应类型
│   │   ├── user_manager.rs       # 用户生命周期管理
│   │   └── recovery.rs           # WAL 用户数据恢复
│   │
│   ├── exchange/                 # 交易所业务
│   │   ├── account_mgr.rs        # 账户管理中心
│   │   ├── capital_mgr.rs        # 资金管理
│   │   ├── order_router.rs       # 订单路由
│   │   ├── trade_gateway.rs      # 成交回报网关
│   │   ├── settlement.rs         # 结算系统
│   │   └── instrument_registry.rs # 合约注册表
│   │
│   ├── risk/                     # 风控系统 (自成交防范)
│   ├── market/                   # 行情推送 (Phase 9)
│   │   ├── broadcaster.rs        # 行情广播器 (crossbeam channel)
│   │   ├── snapshot_broadcaster.rs # 订单簿快照广播
│   │   ├── snapshot_generator.rs # 市场快照生成器
│   │   ├── cache.rs              # L1 行情缓存 (DashMap, 100ms TTL)
│   │   ├── recovery.rs           # 行情数据恢复器
│   │   └── mod.rs                # MarketDataService
│   ├── service/                  # 对外服务
│   │   ├── websocket/            # WebSocket 服务
│   │   └── http/                 # HTTP API
│   ├── storage/                  # 持久化存储
│   │   ├── wal/                  # Write-Ahead Log
│   │   ├── memtable/             # 内存表 (SkipMap OLTP / Arrow2 OLAP)
│   │   ├── sstable/              # 持久化表 (rkyv / Parquet + Bloom Filter)
│   │   ├── index/                # 二级索引 ✨ Phase 11
│   │   │   ├── time_series.rs    # 时间序列索引
│   │   │   ├── instrument.rs     # 合约索引 (字符串驻留池)
│   │   │   ├── record_type.rs    # 记录类型索引 (位掩码)
│   │   │   └── mod.rs            # CompositeIndexManager
│   │   ├── hybrid/               # 混合存储管理器
│   │   ├── compaction/           # 后台压缩
│   │   ├── checkpoint/           # 快照管理
│   │   └── subscriber.rs         # 异步存储订阅器
│   ├── replication/              # 主从复制 (Phase 6 + 12)
│   │   ├── log_replicator.rs     # 批量日志复制
│   │   ├── role_manager.rs       # Master/Slave/Candidate
│   │   ├── heartbeat.rs          # 心跳检测
│   │   └── tls.rs                # TLS 加密 🔒 Phase 12
│   ├── query/                    # 查询引擎 (Phase 8)
│   │   ├── types.rs              # 查询请求/响应类型
│   │   ├── scanner.rs            # SSTable 扫描器
│   │   └── engine.rs             # Polars 查询引擎
│   ├── factor/                   # 因子系统 ✨ Phase 11
│   │   ├── engine.rs             # 流批统一引擎
│   │   ├── dag.rs                # DAG 依赖图
│   │   ├── state.rs              # 状态存储与检查点
│   │   ├── view.rs               # 物化视图
│   │   ├── wal_persister.rs      # WAL 异步持久化 ✨
│   │   └── operators/            # 增量算子
│   ├── ipc/                      # iceoryx2 零拷贝 IPC (可选)
│   ├── notification/             # 通知系统 (rkyv)
│   ├── protocol/                 # 协议层 (QIFI/TIFI/MIFI)
│   └── utils/                    # 工具模块
│
├── examples/                     # 示例代码
│   ├── start_exchange.rs         # 启动示例
│   ├── client_demo.rs            # 客户端示例
│   └── stress_test.rs            # 压力测试
│
└── tests/                        # 集成测试
```

## 核心复用能力

| 模块 | qars 复用 | 复用度 | 说明 |
|------|----------|--------|------|
| 账户系统 | `QA_Account` | ⭐⭐⭐⭐⭐ 95% | 完整复用账户/持仓管理 |
| 订单系统 | `QAOrder` + QIFI | ⭐⭐⭐⭐⭐ 90% | 订单结构和协议 |
| 撮合引擎 | `Orderbook` | ⭐⭐⭐⭐⭐ 98% | 价格时间优先撮合 |
| 协议层 | QIFI/TIFI/MIFI | ⭐⭐⭐⭐⭐ 100% | 完全复用 |
| 数据广播 | `broadcast_hub` | ⭐⭐⭐⭐⭐ 95% | 零拷贝行情推送 |

---

## 🧩 核心模块详解

### 1. 交易所核心（Exchange Core）
**位置**: `src/exchange/`

**模块**:

**AccountManager** (`account_mgr.rs`): 账户生命周期管理
- 开户/入金/出金/查询
- 多账户并发访问 (`DashMap<String, Arc<RwLock<QA_Account>>>`)
- 账户快照和恢复

**OrderRouter** (`order_router.rs`): 订单路由与验证
- 订单接收/验证/路由
- 盘前风控检查集成
- 订单状态追踪

**TradeGateway** (`trade_gateway.rs`): 成交回报网关
- 实时成交推送 (WebSocket)
- 账户更新通知
- rkyv零拷贝序列化 (125x vs JSON)

**SettlementEngine** (`settlement.rs`): 日终结算引擎
- 盯市盈亏计算
- 手续费结算
- 强平检测 (风险度 >= 100%)
- 批量账户结算

**InstrumentRegistry** (`instrument_registry.rs`): 合约注册表
- 合约上市/下市 (含持仓安全检查)
- 交易时间管理
- 保证金率配置

**性能**: > 100K orders/sec 订单吞吐, P99 < 100μs 撮合延迟

---

### 2. 撮合引擎（Matching Engine）
**位置**: `src/matching/`
**复用**: 98% 复用 `qars::qamarket::matchengine::Orderbook`

**功能**:
- 价格-时间优先撮合算法
- 集合竞价 (`auction.rs`)
- 连续交易
- 成交记录 (`trade_recorder.rs`)

**性能**: 基于qars撮合引擎, P99 < 100μs

---

### 3. 存储系统（Storage System）
**位置**: `src/storage/`
**架构**: WAL + MemTable + SSTable (LSM-Tree) + 二级索引

**模块**:

**WAL** (`wal/`): Write-Ahead Log
- 崩溃恢复机制 (CRC32 校验)
- P99 < 50ms 写入延迟 (HDD)
- 批量吞吐 > 78K entries/sec

**MemTable** (`memtable/`): 内存表
- **OLTP**: SkipMap (P99 < 10μs 写入)
- **OLAP**: Arrow2 列式格式

**SSTable** (`sstable/`): 持久化表
- **OLTP**: rkyv 零拷贝序列化
- **OLAP**: Parquet 列式存储
- **Bloom Filter**: 1% FP rate, ~100ns 查找
- **mmap Reader**: 零拷贝读取 (P99 < 50μs)
- **智能压缩** ✨: ZSTD(3)/LZ4/Snappy 按类型选择

**二级索引** ✨ (`index/`): Phase 11 新增
- **TimeSeriesIndex**: BTreeMap 时间范围查询 O(log n + k)
- **InstrumentIndex**: 按合约分区 + 字符串驻留池
- **RecordTypeIndex**: 位掩码 O(1) 类型匹配
- **CompositeIndexManager**: 智能查询路径选择

**Compaction** (`compaction/`): 后台压缩
- Leveled compaction 策略

**Checkpoint** (`checkpoint/`): 快照管理
- 账户快照创建/恢复

**详细文档**: [WAL 设计](docs/03_core_modules/storage/wal.md) | [MemTable 实现](docs/03_core_modules/storage/memtable.md) | [SSTable 格式](docs/03_core_modules/storage/sstable.md) | [二级索引](docs/03_core_modules/storage/index.md) ✨

---

### 4. 因子系统（Factor System）✨ Phase 11
**位置**: `src/factor/`
**架构**: 流批统一计算引擎

**模块**:
- **FactorEngine** (`engine.rs`): 增量计算引擎
- **FactorDAG** (`dag.rs`): 依赖图管理
- **StateStorage** (`state.rs`): 状态检查点 (ZSTD Level 3)
- **MaterializedView** (`view.rs`): 物化视图
- **WalPersister** (`wal_persister.rs`) ✨: 异步 WAL 持久化

**性能**:
- 因子 WAL 吞吐: > 200K ops/s
- 因子恢复: < 500ms/100K 条目
- 检查点保存: ~5ms/100KB
- 检查点加载: ~2ms/100KB

**详细文档**: [因子系统](docs/03_core_modules/factor/README.md) | [WAL 持久化](docs/03_core_modules/factor/wal_persister.md) ✨

---

### 5. 查询引擎（Query Engine）Phase 8
**位置**: `src/query/`
**基础**: Polars 0.51 DataFrame

**功能**:
- **SQL查询**: SQLContext 支持标准SQL
- **结构化查询**: select, filter, aggregate, sort, limit
- **时间序列查询**: granularity 时间粒度聚合
- **Parquet扫描**: 列式文件高效读取
- **索引加速** ✨: 使用二级索引优化查询路径

**性能**:
- SQL 查询 (100行): < 10ms
- Parquet 扫描: > 1GB/s 吞吐
- 聚合查询: < 50ms
- 时间序列聚合: < 100ms

**详细文档**: [Phase 8 查询引擎](docs/08_advanced/phase_reports/phase_8.md)

---

### 6. 主从复制（Replication）Phase 6 + 12
**位置**: `src/replication/`
**协议**: Raft-inspired 选主算法

**模块**:
- **LogReplicator** (`log_replicator.rs`): 批量日志复制
- **RoleManager** (`role_manager.rs`): Master/Slave/Candidate 角色管理
- **Heartbeat** (`heartbeat.rs`): 心跳检测
- **TLS** (`tls.rs`) 🔒: TLS 1.3 加密通信 (Phase 12)

**性能**:
- 复制延迟: < 10ms
- 心跳间隔: 100ms
- 故障切换: < 500ms

**安全特性** 🔒:
- TLS 1.3 加密
- 证书验证
- 双向认证 (mTLS)

**详细文档**: [Phase 6-7 实现总结](docs/08_advanced/phase_reports/phase_6_7.md) | [TLS 安全指南](docs/06_development/tls_security_guide.md) 🆕

---

### 7. 风控系统（Risk Management）
**位置**: `src/risk/`

**功能**:
- **盘前风控检查** (`pre_trade_check.rs`)
  - 资金充足性检查
  - 持仓限额检查
  - 自成交防范
- **实时风险监控**
  - 风险度计算 (margin_used / balance)
  - 强平触发 (risk >= 100%)

**集成**: OrderRouter → PreTradeCheck → MatchingEngine

---

### 8. 服务层（Service Layer）
**位置**: `src/service/`

**HTTP Server** (`http/`): REST API
- **用户端**: 20 API (账户/订单/持仓/历史)
- **管理端**: 25 API (合约/结算/风控/监控/市场)
- **框架**: Actix-web 4.4
- **认证**: 环境变量配置 `QAEXCHANGE_ADMIN_TOKEN`

**WebSocket Server** (`websocket/`): 实时通信
- 交易通道 (下单/撤单/成交回报)
- 行情通道 (订单簿/逐笔成交)
- 心跳机制 (可配置超时)
- JWT 认证

**配置环境变量**:
```bash
QAEXCHANGE_ADMIN_TOKEN=your_secure_token      # 管理员令牌
QAEXCHANGE_WS_HEARTBEAT_SECS=5                # 心跳间隔 (默认5秒)
QAEXCHANGE_WS_TIMEOUT_SECS=10                 # 超时时间 (默认10秒)
```

**API文档**:
- [用户端API参考](docs/04_api/http/user_api.md) - 20个REST API
- [管理端API参考](docs/04_api/http/admin_api.md) ⭐ - 25个REST API
- [WebSocket协议](docs/04_api/websocket/protocol.md) - 8个消息类型

---

### 9. 可观测性（Observability）🆕 Phase 12
**位置**: 分布在各模块

**指标导出** (Prometheus):
- 交易指标: 订单数/成交数/延迟分布
- 存储指标: WAL 大小/MemTable 条目数/SSTable 文件数
- 复制指标: 复制延迟/心跳状态/节点角色
- 系统指标: CPU/内存/磁盘使用率

**分布式追踪** (OpenTelemetry):
- 请求链路追踪
- 跨服务调用关联
- 延迟分析

**日志** (tracing):
- 结构化日志输出
- 日志级别控制
- 上下文信息传递

**详细文档**: [可观测性指南](docs/06_development/observability_guide.md) 🆕

---

### 10. 通知系统（Notification System）
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

**详细文档**: [序列化指南](docs/05_integration/serialization.md)

---

### 11. 前端应用（Frontend Application）
**位置**: `web/`
**技术栈**: Vue 2.6.11 + Element UI + vxe-table + ECharts

**用户端页面** (9个):
- `login.vue` - 登录页
- `account.vue` - 账户管理 (开户/入金/出金/查询)
- `trade.vue` - 下单页 (下单/撤单)
- `positions.vue` - 持仓查询 (实时持仓)
- `orders.vue` - 订单查询 (历史订单)
- `trades.vue` - 成交查询 (成交记录)
- `market.vue` - 实时行情 (WebSocket行情)
- `history.vue` - 账户历史
- `risk-monitor.vue` - 风险监控

**管理端页面** (6个) ⭐ v0.4.0:
- `admin/dashboard.vue` - 仪表盘 (系统概览)
- `admin/instruments.vue` - 合约管理 (上市/下市/修改)
- `admin/settlement.vue` - 结算管理 (日终结算)
- `admin/risk.vue` - 风控管理 (风险账户监控)
- `admin/monitoring.vue` - 系统监控 (存储/性能监控)
- `admin/accounts.vue` - 账户管理 (账户列表)

**详细文档**: [前端对接指南](docs/05_integration/frontend/integration_guide.md)

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
│   + Prometheus Metrics + OpenTelemetry      │ 🆕
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
│  + FactorEngine (因子系统) ✨                │
└──────────────┬──────────────────────────────┘
               │
┌──────────────▼──────────────────────────────┐
│      Data Layer (数据层 - 复用qars)          │
│  QA_Account + QAOrder + Orderbook           │
└──────────────┬──────────────────────────────┘
               │
┌──────────────▼──────────────────────────────┐
│      Storage Layer (存储层)                  │
│  WAL → MemTable → SSTable → Compaction      │
│  + SecondaryIndex ✨ + Compression ✨        │
└──────────────┬──────────────────────────────┘
               │
         ┌─────┴─────┐
         │           │
   Replication   QueryEngine
   (Phase 6+12)  (Phase 8)
   + TLS 🔒      + Index ✨
```

---

## 性能目标

基于 qars 性能基准测试和已实现的存储层:

| 指标 | 目标值 | 状态 | 依据 |
|------|--------|------|------|
| **交易引擎** |
| 订单吞吐量 | > 100K orders/sec | ✅ | 复用 `Orderbook` |
| 撮合延迟 | P99 < 100μs | ✅ | qars 撮合引擎 |
| 行情推送延迟 | P99 < 10μs | ✅ | `broadcast_hub` |
| Tick查询延迟 (缓存命中) | < 10μs | ✅ | L1 DashMap 缓存 (Phase 9) |
| WebSocket批量推送 | 100条/批 | ✅ | 批量序列化 (Phase 9) |
| 行情恢复时间 | < 5s | ✅ | WAL回放 (Phase 9) |
| **存储系统** |
| WAL 写入延迟 | P99 < 50ms (HDD) | ✅ | Phase 1 完成 |
| WAL 批量吞吐 | > 78K entries/sec | ✅ | 批量写入测试 |
| MemTable 写入延迟 | P99 < 10μs | ✅ | SkipMap (Phase 2) |
| SSTable 读取延迟 | P99 < 50μs | ✅ | mmap 零拷贝 (Phase 7) |
| Bloom Filter 查找 | ~100ns | ✅ | 7 hash functions (Phase 7) |
| 通知序列化 | 125x vs JSON | ✅ | rkyv 零拷贝 |
| **二级索引** ✨ |
| 索引 add() | ~50ns | ✅ | BTreeMap + HashMap (Phase 11) |
| 时间范围查询 | ~1μs | ✅ | BTreeMap range (Phase 11) |
| 合约查询 | ~500ns | ✅ | 字符串驻留池 (Phase 11) |
| 类型查询 | ~800ns | ✅ | 位掩码匹配 (Phase 11) |
| 复合查询 | ~2μs | ✅ | 多索引交集 (Phase 11) |
| **压缩** ✨ |
| ZSTD(3) 压缩 | 250 MB/s | ✅ | 默认平衡 (Phase 11) |
| LZ4 压缩 | 800 MB/s | ✅ | 热数据 (Phase 11) |
| ZSTD(9) 压缩 | 40 MB/s | ✅ | 冷数据归档 (Phase 11) |
| **因子引擎** ✨ |
| 因子 WAL 吞吐 | > 200K ops/s | ✅ | 异步通道 + 批量 (Phase 11) |
| 因子恢复 | < 500ms/100K | ✅ | WAL 回放 (Phase 11) |
| 检查点保存 | ~5ms/100KB | ✅ | ZSTD Level 3 |
| 检查点加载 | ~2ms/100KB | ✅ | ZSTD 解压 |
| **主从复制** |
| 日志复制延迟 | < 10ms | ✅ | 批量复制 (Phase 6) |
| 心跳间隔 | 100ms | ✅ | 可配置 (Phase 6) |
| 故障切换时间 | < 500ms | ✅ | 自动选主 (Phase 6) |
| **查询引擎** |
| SQL 查询 (100 行) | < 10ms | ✅ | Polars LazyFrame (Phase 8) |
| Parquet 扫描吞吐 | > 1GB/s | ✅ | Arrow2 列式 (Phase 8) |
| 聚合查询 | < 50ms | ✅ | Polars 优化 (Phase 8) |
| 时间序列粒度聚合 | < 100ms | ✅ | Group-by 优化 (Phase 8) |
| **并发能力** |
| 并发账户数 | > 10,000 | ✅ | DashMap 无锁并发 |
| 并发订阅者 | > 1,000 | 🚧 | iceoryx2 框架 (Phase 4) |
| **复制监控** 🆕 |
| 节点状态采集 | < 10ms | ✅ | CPU + 内存 + 磁盘使用率 |

---

## 📡 API 概览

### 用户端 HTTP API (20个) ✅ 已实现

**账户管理** (4个)
```
POST   /api/account/open               # 开户
POST   /api/account/deposit            # 入金
POST   /api/account/withdraw           # 出金
GET    /api/account/{user_id}          # 查询账户
```

**订单管理** (4个)
```
POST   /api/order/submit               # 下单
POST   /api/order/cancel               # 撤单
GET    /api/order/{order_id}           # 查询订单
GET    /api/order/user/{user_id}       # 列出用户订单
```

**持仓查询** (1个)
```
GET    /api/position/{user_id}         # 查询持仓
```

**其他** (11个)
- 成交记录查询
- 账户历史查询
- 合约信息查询
- 等...

**完整文档**: [用户端API参考](docs/04_api/http/user_api.md)

---

### 管理端 HTTP API (25个) ⭐ 已实现

**合约管理** (6个)
```
GET    /admin/instruments              # 查询所有合约
POST   /admin/instrument/create        # 上市合约
PUT    /admin/instrument/{id}          # 修改合约
DELETE /admin/instrument/{id}          # 下市合约 (含持仓检查)
GET    /admin/instrument/{id}          # 查询合约详情
POST   /admin/instrument/{id}/status   # 修改合约状态
```

**结算管理** (5个)
```
POST   /admin/settlement/set-price     # 设置结算价
POST   /admin/settlement/batch-set-prices  # 批量设价
POST   /admin/settlement/execute       # 执行日终结算
GET    /admin/settlement/history       # 结算历史
GET    /admin/settlement/result/{date} # 结算结果
```

**风控管理** (3个)
```
GET    /admin/risk/accounts            # 查询风险账户
GET    /admin/risk/liquidation         # 查询强平记录
GET    /admin/risk/position-limits     # 持仓限额配置
```

**系统监控** (6个)
```
GET    /admin/monitoring/storage       # 存储状态
GET    /admin/monitoring/accounts      # 账户统计
GET    /admin/monitoring/capital       # 资金汇总
GET    /admin/monitoring/performance   # 性能指标
GET    /admin/monitoring/orderbooks    # 订单簿状态
GET    /admin/monitoring/system        # 系统状态
```

**市场数据** (5个)
```
GET    /admin/market/trades            # 所有成交记录
GET    /admin/market/orderbook/{id}    # 订单簿快照
GET    /admin/market/instruments/active # 活跃合约
GET    /admin/market/statistics        # 市场统计
GET    /admin/market/depth/{id}        # 市场深度
```

**完整文档**: [管理端API参考](docs/04_api/http/admin_api.md) ⭐

---

### WebSocket 协议 (8消息类型) ✅ 已实现

**连接方式**:
```
ws://host:port/ws?user_id={user_id}
```

**客户端→服务器** (6个)
- `Auth` - 认证
- `Subscribe` - 订阅频道
- `SubmitOrder` - 提交订单
- `CancelOrder` - 撤销订单
- `QueryAccount` - 查询账户
- `Ping` - 心跳

**服务器→客户端** (8个)
- `AuthResponse` - 认证响应
- `Trade` - 成交通知
- `OrderStatus` - 订单状态更新
- `AccountUpdate` - 账户更新
- `OrderBook` - 订单簿快照
- `Error` - 错误消息
- `Pong` - 心跳响应

**完整文档**: [WebSocket协议文档](docs/04_api/websocket/protocol.md)

---

## 数据流

```
客户端 (WebSocket/HTTP)
    ↓
Service Layer (service/)
    ↓
OrderRouter (订单路由)
    ├─> PreTradeCheck (风控检查)
    └─> ExchangeMatchingEngine (撮合)
            ↓
        TradeGateway (成交回报)
            ↓
        DataBroadcaster (广播推送)
            ↓
        订阅者 (客户端/监控系统)
```

## 开发状态

### ✅ Phase 1-10: 核心功能完成

**Phase 1-2**: 核心交易系统 + 存储基础
- [x] WAL (Write-Ahead Log) + 崩溃恢复
- [x] MemTable + SSTable (OLTP/OLAP)
- [x] 账户管理 + 订单路由 + 撮合引擎

**Phase 3-5**: 压缩/IPC/检查点
- [x] Leveled compaction + Bloom Filter
- [x] iceoryx2 零拷贝 IPC (optional)
- [x] Checkpoint 快照恢复

**Phase 6-7**: 主从复制 + 性能优化
- [x] Raft-inspired 选主 + 批量复制
- [x] mmap 零拷贝 + Bloom Filter 优化

**Phase 8**: 查询引擎
- [x] Polars DataFrame + SQL 查询

**Phase 9**: 行情推送增强
- [x] L1 缓存 + WAL 行情 + 批量推送

**Phase 10**: 用户管理系统
- [x] JWT 认证 + bcrypt 加密 + 用户注册登录

### ✅ Phase 11: 性能优化 II ✨ (已完成 2025-11-26)

- [x] **二级索引系统**: TimeSeriesIndex + InstrumentIndex + RecordTypeIndex
- [x] **智能压缩策略**: ZSTD/LZ4/Snappy 按数据类型动态选择
- [x] **因子 WAL 集成**: 异步持久化 (crossbeam channel) + 崩溃恢复
- [x] **CompositeIndexManager**: 智能查询路径选择
- [x] **字符串驻留池**: 内存高效的合约索引
- [x] **位掩码匹配**: O(1) 记录类型查询

### ✅ Phase 12: 生产就绪 🆕 (已完成 2025-12-18)

- [x] **TLS 加密**: 主从复制 TLS 1.3 支持
- [x] **可观测性**: Prometheus + OpenTelemetry + 结构化日志
- [x] **节点监控**: CPU/内存/磁盘使用率采集
- [x] **文档完善**: TLS 安全指南 + 可观测性指南 + 2025-2026 路线图
- [x] **代码质量**: Clippy 警告清零 + 环境变量配置化

### 🚧 Phase 13: 网络层 (进行中)

- [ ] gRPC 服务实现
- [ ] Master-slave 网络通信
- [ ] 完整 TLS 双向认证

## 技术栈

### 核心依赖

**交易引擎**
- **qars (qa-rs)**: 核心账户/订单/撮合引擎 (本地依赖)
- **Actix-web**: Web 框架
- **Tokio**: 异步运行时
- **DashMap**: 无锁并发 HashMap
- **parking_lot**: 高性能锁

**存储系统**
- **rkyv**: 零拷贝序列化 (通知系统 + OLTP SSTable)
- **crossbeam-skiplist**: 无锁 SkipMap (OLTP MemTable)
- **arrow2**: 列式内存格式 (OLAP MemTable + Parquet)
- **crc32fast**: CRC32 校验 (WAL 数据完整性)
- **memmap2**: mmap 零拷贝文件映射 (SSTable 读取)
- **zstd/lz4**: 压缩算法 ✨

**查询引擎 (Phase 8)**
- **polars**: DataFrame 库 (v0.51, SQL + LazyFrame)
  - Features: lazy, sql, parquet, dtype-full, is_in

**分布式系统**
- **iceoryx2**: 零拷贝 IPC (optional feature, Phase 4)
- **rcgen**: TLS 证书生成 🔒

**可观测性 🆕**
- **prometheus**: 指标导出
- **opentelemetry**: 分布式追踪
- **tracing**: 结构化日志

### 数据协议

- **QIFI**: 账户数据格式
- **TIFI**: 交易指令格式
- **MIFI**: 市场数据格式

## 参考真实交易所

设计参考:
- **CTP**: 上期技术综合交易平台
- **上交所**: 上海证券交易所
- **深交所**: 深圳证券交易所

核心流程:
1. 账户注册 → 2. 入金 → 3. 下单 → 4. 撮合 → 5. 成交回报 → 6. 结算

## 许可证

MIT

---

## 📚 完整文档导航

### 快速开始
- [主文档 README.md](.) - 项目概览和快速开始 ⭐ 当前文档
- [快速入门指南](docs/01_getting_started/quick_start.md) - 5分钟快速上手
- [前端对接指南](docs/05_integration/frontend/integration_guide.md) - 前端开发者必读
- [部署指南](docs/06_development/deployment.md) - 快速部署到生产环境

### 架构与设计
- [系统架构](docs/02_architecture/system_overview.md) - 完整系统设计 ⭐ 已更新
- [高性能架构](docs/02_architecture/high_performance.md) - P99 < 100μs 延迟设计
- [功能映射矩阵](docs/07_reference/feature_matrix.md) ⭐ - 前后端功能对照表
- [数据模型](docs/02_architecture/data_models.md) ⭐ - QIFI/TIFI/DIFF 协议详解
- [解耦存储架构](docs/02_architecture/decoupled_storage.md) - 零拷贝 + WAL 持久化
- [2025-2026 技术路线图](docs/08_advanced/ROADMAP_2025_2026.md) 🆕 - 未来演进计划

### 核心模块
- [存储系统](docs/03_core_modules/README.md) - WAL + MemTable + SSTable 架构
  - [WAL 设计](docs/03_core_modules/storage/wal.md) - Write-Ahead Log 崩溃恢复
  - [MemTable 实现](docs/03_core_modules/storage/memtable.md) - OLTP/OLAP 内存表
  - [SSTable 格式](docs/03_core_modules/storage/sstable.md) - rkyv/Parquet 持久化
  - [二级索引](docs/03_core_modules/storage/index.md) ✨ - 时间/合约/类型索引
  - [压缩策略](docs/03_core_modules/storage/compression.md) ✨ - 智能压缩选择
  - [查询引擎](docs/03_core_modules/storage/query_engine.md) - Polars SQL 查询
  - [复制系统](docs/03_core_modules/storage/replication.md) - 主从复制与故障转移
- [因子系统](docs/03_core_modules/factor/README.md) ✨ - 流批统一因子引擎
  - [WAL 持久化](docs/03_core_modules/factor/wal_persister.md) ✨ - 异步持久化与恢复
- [市场数据模块](docs/03_core_modules/market/README.md) - 行情数据服务总览
  - [快照生成器](docs/03_core_modules/market/snapshot_generator.md) - 每秒级别市场快照
  - [测试指南](docs/03_core_modules/market/testing.md) - 完整测试流程
- [通知系统](docs/03_core_modules/notification/architecture.md) - 零拷贝通知推送

### API 参考
- [用户端API参考](docs/04_api/http/user_api.md) - 20个REST API
- [管理端API参考](docs/04_api/http/admin_api.md) ⭐ - 25个REST API
- [WebSocket协议](docs/04_api/websocket/protocol.md) - DIFF 协议完整定义
- [DIFF 协议详解](docs/04_api/websocket/diff_protocol.md) - 差分同步机制
- [WebSocket 快速开始](docs/04_api/websocket/quick_start.md) - WebSocket 客户端示例
- [错误码说明](docs/04_api/error_codes.md) - 所有错误码详解

### 集成指南
- [前端集成指南](docs/05_integration/frontend/integration_guide.md) - Vue.js 集成示例
- [DIFF 协议集成](docs/05_integration/diff_protocol.md) - DIFF 协议接入详解
- [序列化指南](docs/05_integration/serialization.md) - rkyv/JSON 序列化最佳实践

### 开发指南
- [开发规范](CLAUDE.md) - 项目约定（复用qars优先）
- [开发环境](docs/06_development/README.md) - 环境搭建
- [WebSocket 集成](docs/06_development/websocket_integration.md) - DIFF 协议接入详解
- [测试指南](docs/06_development/testing.md) - 单元测试和集成测试
- [部署指南](docs/06_development/deployment.md) - 生产环境部署
- [TLS 安全指南](docs/06_development/tls_security_guide.md) 🆕 - TLS 配置与证书管理
- [可观测性指南](docs/06_development/observability_guide.md) 🆕 - 监控与追踪

### 参考资料
- [术语表](docs/07_reference/glossary.md) - 专业术语解释
- [常见问题 FAQ](docs/07_reference/faq.md) - 常见问题解答
- [性能基准](docs/07_reference/benchmarks.md) - 性能测试数据
- [性能指标](docs/07_reference/performance.md) - 性能调优建议

### 高级主题
- [Phase 6-7 实现总结](docs/08_advanced/phase_reports/phase_6_7.md) - 主从复制与性能优化
- [Phase 8 查询引擎](docs/08_advanced/phase_reports/phase_8.md) - Polars DataFrame分析引擎
- [Phase 12-13 实现报告](docs/08_advanced/phase_reports/phase_12_13.md) 🆕 - 生产就绪与网络层
- [2025-2026 技术路线图](docs/08_advanced/ROADMAP_2025_2026.md) 🆕 - 完整演进计划
- [市场数据增强实现](docs/08_advanced/technical_deep_dive/market_data_enhancement.md) - L1 缓存与 WAL 恢复
- [市场数据实现总结](docs/08_advanced/implementation_summaries/market_data.md) - Phase 9 市场数据增强
- [管理功能实现](docs/08_advanced/implementation_summaries/management_features.md) - Phase 10 用户管理
- [DIFF 测试报告](docs/08_advanced/diff_test_reports/main_report.md) - DIFF 协议测试结果

### 其他
- [变更日志](CHANGELOG.md) - 版本历史和更新记录
- [文档中心](docs/README.md) - 完整文档索引 (70+ 文档)

⭐ = v0.4.0 新增或更新文档 | ✨ = v1.5.0 新增功能/文档 | 🆕 = 最新文档

---

## 联系方式

基于 QUANTAXIS 项目构建

**作者**: @yutiansut @quantaxis
