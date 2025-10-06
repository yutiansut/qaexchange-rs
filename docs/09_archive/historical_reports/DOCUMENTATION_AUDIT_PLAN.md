# QAExchange 文档审计与更新计划

## 执行时间
2025-10-05

---

## 📋 一、现状分析

### 1.1 已实现功能统计

#### 前端页面（17个）
```
用户端 (9个):
├── login.vue                    # 登录
├── register.vue                 # 注册
├── dashboard/index.vue          # 仪表盘
├── trade/index.vue              # 交易下单
├── orders/index.vue             # 订单管理
├── positions/index.vue          # 持仓管理
├── trades/index.vue             # 成交记录
├── accounts/index.vue           # 账户信息
└── user/account-curve.vue       # 账户曲线

管理端 (6个):
├── admin/instruments.vue        # 合约管理
├── admin/settlement.vue         # 结算管理
├── admin/risk.vue               # 风控监控
├── admin/accounts.vue           # 账户管理
├── admin/transactions.vue       # 交易管理
└── monitoring/index.vue         # 系统监控

其他 (2个):
├── chart/index.vue              # 行情图表
└── trade/components/*           # 交易组件
```

#### 后端API（42个）
```
账户管理 (6个):
├── open_account                 # 开户
├── query_account                # 查询账户
├── deposit                      # 入金
├── withdraw                     # 出金
├── list_all_accounts            # 账户列表
└── get_account_detail           # 账户详情

订单管理 (5个):
├── submit_order                 # 提交订单
├── cancel_order                 # 撤单
├── query_order                  # 查询订单
├── query_user_orders            # 用户订单列表
└── query_user_trades            # 用户成交列表

持仓管理 (1个):
└── query_position               # 查询持仓

合约管理 (6个):
├── get_all_instruments          # 获取所有合约
├── get_instruments              # 获取合约（筛选）
├── create_instrument            # 创建合约
├── update_instrument            # 更新合约
├── suspend_instrument           # 暂停合约
├── resume_instrument            # 恢复合约
└── delist_instrument            # 下市合约

结算管理 (5个):
├── set_settlement_price         # 设置结算价
├── batch_set_settlement_prices  # 批量设置结算价
├── execute_settlement           # 执行结算
├── get_settlement_history       # 结算历史
└── get_settlement_detail        # 结算详情

风控管理 (3个):
├── get_risk_accounts            # 风险账户列表
├── get_margin_summary           # 保证金汇总
└── get_liquidation_records      # 强平记录

市场数据 (5个):
├── get_tick                     # 获取行情tick
├── get_orderbook                # 获取订单簿
├── get_recent_trades            # 最近成交
├── get_market_order_stats       # 市场订单统计
└── get_transactions             # 交易记录

系统监控 (5个):
├── get_system_monitoring        # 系统监控
├── get_storage_monitoring       # 存储监控
├── get_accounts_monitoring      # 账户监控
├── get_orders_monitoring        # 订单监控
├── get_trades_monitoring        # 成交监控
└── generate_report              # 生成报告

认证管理 (3个):
├── register                     # 注册
├── login                        # 登录
└── get_current_user             # 获取当前用户

系统 (2个):
├── health_check                 # 健康检查
└── run                          # 启动服务
```

### 1.2 现有文档清单（60个文件）

#### 核心文档 (6个)
```
├── README.md                    # 项目主文档
├── CLAUDE.md                    # @yutiansut 指引
├── CHANGELOG.md                 # 变更日志
├── BUILD_CHECKLIST.md           # 构建清单
├── INTEGRATION_COMPLETE.md      # 集成完成报告
└── README_QUICKSTART.md         # 快速开始
```

#### docs/ 架构文档 (46个)
```
根目录:
├── README.md                    # 文档索引
├── ARCHITECTURE.md              # 架构设计
├── API_REFERENCE.md             # API参考
├── DEPLOYMENT.md                # 部署指南
├── DEVELOPMENT.md               # 开发指南
├── PERFORMANCE.md               # 性能指标
├── TESTING.md                   # 测试指南
├── TRADING_MECHANISM.md         # 交易机制
├── WEBSOCKET_PROTOCOL.md        # WebSocket协议
├── ERROR_CODES.md               # 错误代码
├── SERIALIZATION_GUIDE.md       # 序列化指南
└── HIGH_PERFORMANCE_ARCHITECTURE.md

管理功能:
├── ADMIN_API_INTEGRATION.md
├── BACKEND_ADMIN_SUMMARY.md
├── FRONTEND_BACKEND_INTEGRATION_CHECKLIST.md
├── FRONTEND_INTEGRATION.md
├── MANAGEMENT_FEATURE_IMPLEMENTATION.md
├── COMPLETE_FEATURE_SCENARIOS.md
└── DECOUPLED_STORAGE_ARCHITECTURE.md

阶段实现:
├── P1_P2_IMPLEMENTATION_SUMMARY.md
├── P6_P7_实现总结.md
├── PHASE6_7_IMPLEMENTATION.md
└── PHASE8_QUERY_ENGINE.md

notification/ (11个):
├── README.md
├── CHANGELOG.md
├── ITERATIONS.md
├── DOCUMENTATION_OPTIMIZATION_SUMMARY.md
├── 01_DESIGN/
│   ├── SYSTEM_DESIGN.md
│   ├── IMPLEMENTATION_PLAN.md
│   └── RKYV_EVALUATION.md
├── 02_IMPLEMENTATION/
│   ├── API_REFERENCE.md
│   ├── INTEGRATION_GUIDE.md
│   └── FINAL_SUMMARY.md
├── 03_TESTING/
│   └── TESTING.md
└── 04_MAINTENANCE/
    ├── CONTRIBUTION.md
    └── TROUBLESHOOTING.md

storage/ (9个):
├── README.md
├── SUMMARY.md
├── 01_STORAGE_ARCHITECTURE.md
├── 02_DISTRIBUTION_ARCHITECTURE.md
├── 03_RECOVERY_DESIGN.md
├── 04_IMPLEMENTATION_PLAN.md
├── 05_ARROW2_QUERY_ENGINE.md
├── 06_INTEGRATED_IMPLEMENTATION_PLAN.md
└── 07_HYBRID_OLTP_OLAP_DESIGN.md
```

#### web/ 前端文档 (8个)
```
├── README.md                    # 前端说明
├── IMPLEMENTATION_PLAN.md       # 实现计划
├── IMPLEMENTATION_SUMMARY.md    # 实现总结
├── COMPLETION_SUMMARY.md        # 完成总结
├── ENHANCEMENT_PLAN.md          # 增强计划
├── FRONT_FIX_SUMMARY.md         # 修复总结
├── MARKET_API_DOCUMENTATION.md  # 市场API文档
└── TESTING_GUIDE.md             # 测试指南
```

### 1.3 文档问题诊断

#### ❌ 缺失的文档
1. **管理端API完整文档** - 合约/结算/风控API未完整记录
2. **前端路由文档** - 17个页面的路由配置和权限说明
3. **数据流文档** - 前端→后端→存储的完整数据流
4. **部署配置文档** - Nginx、环境变量、数据库配置
5. **监控API文档** - 5个监控API的详细说明
6. **WebSocket完整协议** - 现有文档不完整

#### ⚠️ 过时的文档
1. **API_REFERENCE.md** - 缺少管理端API（11个新增API）
2. **ARCHITECTURE.md** - 未包含最新的管理功能模块
3. **CHANGELOG.md** - 未记录最近的修复和功能添加
4. **DEPLOYMENT.md** - 未包含前端部署说明
5. **FRONTEND_INTEGRATION.md** - 未更新最新的API对接

#### 📝 需要补充的文档
1. **功能清单文档** - 完整的功能矩阵（前端页面 ↔ 后端API）
2. **数据模型文档** - 所有数据结构的TS/Rust定义
3. **权限设计文档** - 用户权限和管理员权限
4. **错误处理文档** - 前后端错误码映射
5. **配置参考文档** - 所有配置项的说明

---

## 📝 二、文档更新计划

### 阶段1：核心文档更新（优先级P0）

#### 1.1 更新 API_REFERENCE.md
**目标**: 补充所有缺失的API文档（11个管理端API + 5个监控API）

**新增章节**:
```markdown
## 合约管理 API
- GET /admin/instruments
- POST /admin/instrument/create
- PUT /admin/instrument/{id}/update
- PUT /admin/instrument/{id}/suspend
- PUT /admin/instrument/{id}/resume
- DELETE /admin/instrument/{id}/delist

## 结算管理 API
- POST /admin/settlement/set-price
- POST /admin/settlement/batch-set-prices
- POST /admin/settlement/execute
- GET /admin/settlement/history
- GET /admin/settlement/detail/{date}

## 风控管理 API
- GET /admin/risk/accounts
- GET /admin/risk/margin-summary
- GET /admin/risk/liquidations

## 系统监控 API
- GET /monitoring/system
- GET /monitoring/storage
- GET /monitoring/accounts
- GET /monitoring/orders
- GET /monitoring/trades
- POST /monitoring/report
```

**预估工作量**: 2小时

---

#### 1.2 更新 ARCHITECTURE.md
**目标**: 补充管理端架构和最新模块

**新增章节**:
```markdown
## 管理端架构
### 合约管理 (InstrumentRegistry)
### 结算引擎 (SettlementEngine)
### 风控监控 (RiskMonitor)
### 系统监控 (SystemMonitor)

## 数据流架构图
User/Admin → HTTP/WebSocket → Router → Engine → Storage

## 模块依赖图
```

**预估工作量**: 1.5小时

---

#### 1.3 更新 CHANGELOG.md
**目标**: 记录所有最近的修复和功能添加

**新增条目**:
```markdown
## [Unreleased]

### Added (2025-10-05)
- 管理端合约管理功能（6个API）
- 管理端结算管理功能（5个API）
- 管理端风控监控功能（3个API）
- 系统监控API（6个）
- 前端管理页面（6个）

### Fixed (2025-10-05)
- 前端移除所有mock数据（~160行）
- 实现日终结算功能（settlement.rs）
- 实现下市合约安全检查（admin.rs）
- 修复持仓盈亏计算（handlers.rs）
- 修复存储监控统计（monitoring.rs）

### Changed
- 前端API全部对接后端
- 结算流程改为两步执行
```

**预估工作量**: 1小时

---

### 阶段2：新增专项文档（优先级P1）

#### 2.1 创建 FEATURE_MATRIX.md
**目标**: 前端页面 ↔ 后端API完整映射表

**内容结构**:
```markdown
# 功能矩阵

## 用户端功能
| 前端页面 | 路由 | 后端API | 状态 | 备注 |
|---------|------|---------|------|------|
| 登录 | /login | POST /auth/login | ✅ | |
| 注册 | /register | POST /auth/register | ✅ | |
| 仪表盘 | /dashboard | GET /account/detail | ✅ | |
| ... | ... | ... | ... | ... |

## 管理端功能
| 前端页面 | 路由 | 后端API | 状态 | 备注 |
|---------|------|---------|------|------|
| 合约管理 | /admin/instruments | GET/POST/PUT/DELETE /admin/instrument/* | ✅ | 已完成对接 |
| ... | ... | ... | ... | ... |
```

**预估工作量**: 1.5小时

---

#### 2.2 创建 DATA_MODELS.md
**目标**: 所有数据结构的完整定义

**内容结构**:
```markdown
# 数据模型文档

## 账户相关
### Account (QIFI)
### Position
### Trade

## 订单相关
### Order (QAOrder)
### OrderBook

## 管理相关
### InstrumentInfo
### SettlementResult
### RiskAccount
```

**预估工作量**: 2小时

---

#### 2.3 创建 DEPLOYMENT_GUIDE.md（重写）
**目标**: 完整的部署指南（后端+前端）

**内容结构**:
```markdown
# 部署指南

## 环境准备
### Rust 环境
### Node.js 环境
### Nginx 配置

## 后端部署
### 编译
### 配置文件
### 启动脚本
### 日志管理

## 前端部署
### 构建
### Nginx 配置
### 环境变量

## 数据库配置
### MongoDB
### ClickHouse（可选）

## 监控和运维
```

**预估工作量**: 2小时

---

#### 2.4 创建 FRONTEND_GUIDE.md
**目标**: 前端开发完整指南

**内容结构**:
```markdown
# 前端开发指南

## 项目结构
## 路由配置
## 状态管理
## API 调用规范
## 组件开发规范
## 样式规范
## 测试规范
```

**预估工作量**: 1.5小时

---

### 阶段3：文档整合优化（优先级P2）

#### 3.1 更新 README.md
**目标**: 重写主文档，反映最新功能

**新增章节**:
- 完整功能列表
- 快速开始（后端+前端）
- 架构概览图
- 文档导航

**预估工作量**: 1小时

---

#### 3.2 创建 docs/README.md（索引页）
**目标**: 文档导航和分类

**内容结构**:
```markdown
# 文档中心

## 快速开始
- [快速开始](../README_QUICKSTART.md)
- [部署指南](DEPLOYMENT_GUIDE.md)

## 核心概念
- [架构设计](ARCHITECTURE.md)
- [交易机制](TRADING_MECHANISM.md)

## API文档
- [REST API](API_REFERENCE.md)
- [WebSocket协议](WEBSOCKET_PROTOCOL.md)

## 开发指南
- [后端开发](DEVELOPMENT.md)
- [前端开发](FRONTEND_GUIDE.md)

## 运维文档
- [部署指南](DEPLOYMENT_GUIDE.md)
- [性能调优](PERFORMANCE.md)
- [监控运维](MONITORING.md)
```

**预估工作量**: 0.5小时

---

#### 3.3 更新 web/README.md
**目标**: 前端文档重组

**预估工作量**: 0.5小时

---

## 📊 三、执行时间表

### Week 1 (Day 1-2): 阶段1 - 核心文档更新
- [ ] Day 1 上午: 更新 API_REFERENCE.md（管理端API）
- [ ] Day 1 下午: 更新 ARCHITECTURE.md（管理模块）
- [ ] Day 2 上午: 更新 CHANGELOG.md（近期变更）
- [ ] Day 2 下午: Review 阶段1文档

### Week 1 (Day 3-4): 阶段2 - 新增专项文档
- [ ] Day 3 上午: 创建 FEATURE_MATRIX.md
- [ ] Day 3 下午: 创建 DATA_MODELS.md
- [ ] Day 4 上午: 创建 DEPLOYMENT_GUIDE.md
- [ ] Day 4 下午: 创建 FRONTEND_GUIDE.md

### Week 1 (Day 5): 阶段3 - 文档整合优化
- [ ] Day 5 上午: 更新 README.md
- [ ] Day 5 下午: 创建文档索引，整体Review

---

## 📋 四、TODO 清单

### 立即执行（P0）
- [ ] 1. 更新 API_REFERENCE.md - 补充11个管理端API
- [ ] 2. 更新 ARCHITECTURE.md - 补充管理端架构
- [ ] 3. 更新 CHANGELOG.md - 记录近期修复

### 本周完成（P1）
- [ ] 4. 创建 FEATURE_MATRIX.md - 功能映射表
- [ ] 5. 创建 DATA_MODELS.md - 数据模型文档
- [ ] 6. 创建 DEPLOYMENT_GUIDE.md - 部署指南
- [ ] 7. 创建 FRONTEND_GUIDE.md - 前端开发指南

### 下周完成（P2）
- [ ] 8. 更新 README.md - 主文档重写
- [ ] 9. 创建 docs/README.md - 文档索引
- [ ] 10. 更新 web/README.md - 前端文档重组

---

## 📈 五、成功指标

### 文档完整性
- ✅ 所有42个API都有完整文档
- ✅ 所有17个前端页面都有说明
- ✅ 前后端功能映射清晰

### 文档质量
- ✅ 代码示例完整可运行
- ✅ 错误处理说明清晰
- ✅ 部署流程可复现

### 文档可维护性
- ✅ 文档结构清晰
- ✅ 导航便捷
- ✅ 版本记录完整

---

**创建时间**: 2025-10-05
**预计完成时间**: 2025-10-10
**总工作量**: 约14小时
