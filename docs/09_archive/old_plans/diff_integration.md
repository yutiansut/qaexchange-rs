# DIFF 协议集成实施计划

**项目**: QAExchange DIFF 协议完整实现
**目标**: 在不破坏 QIFI/TIFI 协议基础上，完整集成 DIFF 协议
**优先级**: 高
**预计工期**: 7 天
**负责人**: 开发团队
**创建日期**: 2025-10-05

---

## 项目概述

本计划详细描述如何将 DIFF (Differential Information Flow for Finance) 协议集成到 QAExchange 系统中，通过复用现有的 QIFI/TIFI 协议实现高效的差分数据同步。

### 核心原则

1. ✅ **向后兼容**: 不修改任何 QIFI/TIFI 现有代码
2. ✅ **类型复用**: 直接使用 qars 的 Account/Position/Order
3. ✅ **渐进式实现**: 支持分阶段交付
4. ✅ **标准合规**: 严格遵循 RFC 7386 (JSON Merge Patch)

### 技术文档

详细的技术分析和融合方案请参考：
- 📄 `/home/quantaxis/qaexchange-rs/docs/DIFF_INTEGRATION.md`
- 📄 `/home/quantaxis/qaexchange-rs/CLAUDE.md` (行 275-917)

---

## 阶段 1: 基础设施搭建（第 1-2 天）

**状态**: ⏳ 待开始
**预计工时**: 16 小时

### 任务清单

#### 1.1 创建 DIFF 协议模块结构

- [ ] **创建目录结构**
  - `src/protocol/diff/mod.rs` - 模块入口
  - `src/protocol/diff/snapshot.rs` - 业务截面管理器
  - `src/protocol/diff/merge.rs` - JSON Merge Patch 实现
  - `src/protocol/diff/quotes.rs` - 行情数据扩展
  - `src/protocol/diff/klines.rs` - K线数据扩展
  - `src/protocol/diff/notify.rs` - 通知系统扩展
  - `src/protocol/diff/trades.rs` - 成交记录扩展

- [ ] **类型定义复用**
  ```rust
  // src/protocol/diff/mod.rs
  use qars::qaprotocol::qifi::{Account, Position, Order, BankDetail};

  pub type DiffAccount = Account;
  pub type DiffPosition = Position;
  pub type DiffOrder = Order;
  pub type DiffBank = BankDetail;
  ```

#### 1.2 实现 JSON Merge Patch

- [ ] **核心函数实现** (`src/protocol/diff/merge.rs`)
  - `merge_patch(target: &mut Value, patch: &Value)` - 单个 Patch 合并
  - `apply_patches(snapshot: &mut Value, patches: Vec<Value>)` - 批量应用
  - 处理 null 值删除语义
  - 处理嵌套对象递归合并

- [ ] **单元测试**
  - 测试简单字段更新
  - 测试嵌套对象合并
  - 测试 null 值删除
  - 测试数组替换（非合并）
  - 边界条件测试

**验收标准**:
- [ ] 单元测试覆盖率 ≥ 90%
- [ ] 符合 RFC 7386 标准
- [ ] 通过 JSON Merge Patch 官方示例测试

#### 1.3 实现业务截面管理器

- [ ] **SnapshotManager 核心功能** (`src/protocol/diff/snapshot.rs`)
  ```rust
  pub struct SnapshotManager {
      snapshot: Arc<RwLock<Value>>,
      pending_updates: Arc<RwLock<Vec<Value>>>,
      subscriptions: Arc<RwLock<SubscriptionState>>,
  }
  ```

- [ ] **关键方法**
  - `update(&self, patch: Value)` - 添加更新到队列
  - `peek(&self) -> Vec<Value>` - 获取待发送的更新（阻塞）
  - `get_snapshot(&self) -> Value` - 获取当前完整截面
  - `subscribe(&self, user_id: String, channels: Vec<String>)` - 管理订阅

- [ ] **订阅管理**
  ```rust
  pub struct SubscriptionState {
      quote_subscriptions: HashMap<String, HashSet<String>>,  // user_id -> instruments
      chart_subscriptions: HashMap<String, ChartSubscription>,
  }
  ```

**验收标准**:
- [ ] 支持多用户并发访问
- [ ] peek_message 正确阻塞等待更新
- [ ] 订阅状态准确管理

#### 1.4 定义扩展数据类型

- [ ] **行情数据** (`src/protocol/diff/quotes.rs`)
  ```rust
  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct Quote {
      pub instrument_id: String,
      pub datetime: String,
      pub last_price: f64,
      pub bid_price1: f64,
      pub ask_price1: f64,
      // ... 完整字段参考 CLAUDE.md
  }
  ```

- [ ] **K线数据** (`src/protocol/diff/klines.rs`)
- [ ] **Tick数据** (`src/protocol/diff/klines.rs`)
- [ ] **通知数据** (`src/protocol/diff/notify.rs`)
- [ ] **成交记录** (`src/protocol/diff/trades.rs`)

**产出**:
- ✅ 可编译的 `protocol::diff` 模块
- ✅ 完整的单元测试套件
- ✅ 基准测试（JSON Merge Patch 性能）

---

## 阶段 2: WebSocket 服务集成（第 3-4 天）

**状态**: ⏳ 待开始
**预计工时**: 16 小时
**依赖**: 阶段 1 完成

### 任务清单

#### 2.1 更新 WebSocket 消息类型

- [ ] **修改 `src/service/websocket/messages.rs`**
  - 保留原有 `ClientMessage` 和 `ServerMessage`（兼容性）
  - 新增 `DiffClientMessage` 和 `DiffServerMessage`
  - 实现双向转换（TIFI ↔ DIFF）

- [ ] **消息路由**
  ```rust
  enum WsMessage {
      Legacy(ClientMessage),   // 旧版协议
      Diff(DiffClientMessage), // DIFF 协议
  }
  ```

#### 2.2 实现 peek_message 机制

- [ ] **修改 `src/service/websocket/session.rs`**
  - 添加 `snapshot_manager: Arc<SnapshotManager>` 字段
  - 实现 peek_message 处理逻辑
  - 启动 rtn_data 发送循环

- [ ] **peek_message 处理流程**
  ```rust
  async fn handle_peek_message(&mut self, ctx: &mut ws::WebsocketContext<Self>) {
      let patches = self.snapshot_manager.peek().await;
      let msg = DiffServerMessage::RtnData { data: patches };
      ctx.text(serde_json::to_string(&msg).unwrap());
  }
  ```

- [ ] **rtn_data 自动推送**
  - 客户端发送 peek_message
  - 服务端阻塞等待更新
  - 有更新时立即发送 rtn_data
  - 循环等待下一个 peek_message

#### 2.3 集成到业务逻辑

- [ ] **AccountManager 集成**
  ```rust
  // src/exchange/account_mgr.rs
  impl AccountManager {
      fn notify_snapshot_change(&self, user_id: &str, account: &Account) {
          let patch = json!({
              "trade": {
                  user_id: {
                      "accounts": {
                          account.currency.clone(): account
                      }
                  }
              }
          });
          self.snapshot_manager.update(patch);
      }
  }
  ```

- [ ] **OrderRouter 集成** - 订单状态变化推送
- [ ] **TradeGateway 集成** - 成交记录推送
- [ ] **MarketDataBroadcaster 集成** - 行情数据推送

#### 2.4 实现行情订阅

- [ ] **subscribe_quote 处理**
  ```rust
  fn handle_subscribe_quote(&self, user_id: &str, ins_list: String) {
      let instruments: Vec<_> = ins_list.split(',').collect();
      self.snapshot_manager.subscribe(user_id, instruments);

      // 立即推送当前行情快照
      for instrument in instruments {
          if let Some(quote) = self.market_data.get_quote(instrument) {
              self.snapshot_manager.update(json!({
                  "quotes": {
                      instrument: quote
                  }
              }));
          }
      }
  }
  ```

**验收标准**:
- [ ] WebSocket 服务支持 DIFF 协议
- [ ] peek_message + rtn_data 循环正常工作
- [ ] 账户/订单/行情变化实时推送
- [ ] 多用户并发正常

**产出**:
- ✅ WebSocket 服务完整支持 DIFF 协议
- ✅ 集成测试通过（使用 websocat 工具）

---

## 阶段 3: 前端实现（第 5-6 天）

**状态**: ⏳ 待开始
**预计工时**: 16 小时
**依赖**: 阶段 2 完成

### 任务清单

#### 3.1 创建 WebSocket 客户端

- [ ] **WebSocket 连接管理类** (`web/src/utils/websocket.js`)
  ```javascript
  class DiffWebSocket {
      constructor(url, userId) {
          this.url = url
          this.userId = userId
          this.ws = null
          this.snapshot = {}
          this.callbacks = []
      }

      connect() {
          this.ws = new WebSocket(`${this.url}?user_id=${this.userId}`)
          this.ws.onopen = () => this.startPeekLoop()
          this.ws.onmessage = (event) => this.handleMessage(event)
      }

      startPeekLoop() {
          this.send({ aid: 'peek_message' })
      }

      handleMessage(event) {
          const msg = JSON.parse(event.data)
          if (msg.aid === 'rtn_data') {
              for (const patch of msg.data) {
                  mergePatch(this.snapshot, patch)
              }
              this.notifyCallbacks()
              this.startPeekLoop()  // 继续下一轮
          }
      }
  }
  ```

- [ ] **断线重连机制**
- [ ] **心跳保活**
- [ ] **错误处理**

#### 3.2 实现 JSON Merge Patch

- [ ] **Merge Patch 函数** (`web/src/utils/merge-patch.js`)
  ```javascript
  export function mergePatch(target, patch) {
      if (typeof patch !== 'object' || patch === null || Array.isArray(patch)) {
          return patch
      }

      if (typeof target !== 'object' || target === null || Array.isArray(target)) {
          target = {}
      }

      for (const [key, value] of Object.entries(patch)) {
          if (value === null) {
              delete target[key]
          } else if (typeof value === 'object' && !Array.isArray(value)) {
              target[key] = mergePatch(target[key] || {}, value)
          } else {
              target[key] = value
          }
      }

      return target
  }
  ```

- [ ] **单元测试** (Jest)

#### 3.3 创建 Vuex 业务截面 Store

- [ ] **Snapshot Store** (`web/src/store/modules/snapshot.js`)
  ```javascript
  const state = {
      snapshot: {},
      connected: false,
      subscriptions: {
          quotes: [],
          charts: []
      }
  }

  const mutations = {
      UPDATE_SNAPSHOT(state, patches) {
          for (const patch of patches) {
              mergePatch(state.snapshot, patch)
          }
      },

      SET_CONNECTED(state, connected) {
          state.connected = connected
      }
  }

  const actions = {
      connect({ commit, rootState }) {
          const ws = new DiffWebSocket(WS_URL, rootState.user.userId)
          ws.onUpdate((snapshot) => {
              commit('UPDATE_SNAPSHOT', snapshot)
          })
          ws.connect()
      },

      subscribeQuote({ state }, instruments) {
          ws.send({
              aid: 'subscribe_quote',
              ins_list: instruments.join(',')
          })
      }
  }

  const getters = {
      accounts: (state) => state.snapshot.trade?.[userId]?.accounts || {},
      positions: (state) => state.snapshot.trade?.[userId]?.positions || {},
      orders: (state) => state.snapshot.trade?.[userId]?.orders || {},
      quotes: (state) => state.snapshot.quotes || {}
  }
  ```

- [ ] **集成到主 Store** (`web/src/store/index.js`)

#### 3.4 修改交易页面集成

- [ ] **修改 `web/src/views/trade/index.vue`**
  - 移除 HTTP 轮询逻辑 (`setInterval`)
  - 使用 Vuex snapshot getters
  - 添加 WebSocket 连接状态显示

- [ ] **行情订阅**
  ```vue
  <script>
  export default {
      computed: {
          ...mapGetters('snapshot', ['quotes', 'accounts', 'orders']),

          currentQuote() {
              return this.quotes[this.selectedInstrument] || {}
          }
      },

      methods: {
          ...mapActions('snapshot', ['subscribeQuote']),

          handleInstrumentChange(instrumentId) {
              this.subscribeQuote([instrumentId])
          }
      },

      mounted() {
          // WebSocket 连接在 App.vue 中统一建立
          this.subscribeQuote([this.selectedInstrument])
      }
  }
  </script>
  ```

#### 3.5 实现实时更新 UI

- [ ] **响应式数据绑定**
  - 账户余额实时更新
  - 订单状态实时更新
  - 持仓盈亏实时更新
  - 行情数据实时刷新（Tick）

- [ ] **性能优化**
  - 使用 `Object.freeze()` 冻结大对象
  - 计算属性缓存
  - 虚拟滚动（订单/成交列表）

**验收标准**:
- [ ] WebSocket 连接稳定
- [ ] 业务截面实时同步
- [ ] UI 响应流畅（无闪烁）
- [ ] 断线自动重连
- [ ] 移除所有 HTTP 轮询

**产出**:
- ✅ 完整的前端 DIFF 协议客户端
- ✅ Vuex snapshot store
- ✅ 交易页面实时更新

---

## 阶段 4: 测试与优化（第 7 天）

**状态**: ⏳ 待开始
**预计工时**: 8 小时
**依赖**: 阶段 1-3 完成

### 任务清单

#### 4.1 端到端测试

- [ ] **功能测试**
  - [ ] 登录后自动建立 WebSocket 连接
  - [ ] peek_message + rtn_data 循环正常
  - [ ] 下单后订单状态实时推送
  - [ ] 成交后账户余额实时更新
  - [ ] 持仓盈亏实时计算
  - [ ] 行情订阅和推送

- [ ] **并发测试**
  - [ ] 100 个并发用户连接
  - [ ] 1000 笔订单/秒提交
  - [ ] WebSocket 连接稳定性

- [ ] **压力测试**
  - [ ] 10000 个并发 WebSocket 连接
  - [ ] 行情数据高频推送（100 次/秒）
  - [ ] 内存泄漏检测

#### 4.2 性能优化

- [ ] **后端优化**
  - [ ] JSON Merge Patch 批量生成
  - [ ] SnapshotManager 无锁优化
  - [ ] WebSocket 消息批量发送
  - [ ] 背压处理（队列满时丢弃旧数据）

- [ ] **前端优化**
  - [ ] Merge Patch 批量应用
  - [ ] Vuex mutation 批量提交
  - [ ] 防抖/节流优化

#### 4.3 监控与日志

- [ ] **添加 Prometheus 指标**
  - `diff_websocket_connections` - 当前连接数
  - `diff_rtn_data_sent` - rtn_data 发送次数
  - `diff_merge_patch_size` - Patch 大小分布
  - `diff_peek_wait_time` - peek_message 等待时间

- [ ] **日志增强**
  - 记录每次 snapshot 更新
  - 记录订阅变化
  - 记录 WebSocket 连接/断开

#### 4.4 文档完善

- [ ] **用户文档**
  - WebSocket 连接指南
  - DIFF 协议使用示例
  - 故障排查指南

- [ ] **开发文档**
  - 更新 CLAUDE.md
  - 更新 API 文档
  - 添加架构图

**验收标准**:
- [ ] 所有功能测试通过
- [ ] 性能满足目标（见下表）
- [ ] 文档完整

| 性能指标 | 目标 | 实际 |
|----------|------|------|
| WebSocket 延迟 | < 10ms (P99) | - |
| Merge Patch 性能 | > 10K ops/s | - |
| 并发连接数 | > 1000 | - |
| 内存占用 | < 100MB (1000连接) | - |

**产出**:
- ✅ 完整的测试报告
- ✅ 性能基准报告
- ✅ 生产就绪的系统

---

## 风险评估与应对

### 高风险项

| 风险 | 概率 | 影响 | 应对措施 |
|------|------|------|----------|
| QIFI/TIFI 协议不兼容 | 低 | 高 | ✅ 已验证 100% 兼容 |
| JSON Merge Patch 性能不足 | 中 | 中 | 批量优化 + 缓存 |
| WebSocket 连接不稳定 | 中 | 高 | 断线重连 + 心跳保活 |
| 前端内存泄漏 | 中 | 中 | 定期 GC + 对象池 |

### 低风险项

| 风险 | 概率 | 影响 | 应对措施 |
|------|------|------|----------|
| 浏览器兼容性 | 低 | 低 | WebSocket 已是标准 API |
| 序列化性能 | 低 | 低 | serde_json 已高度优化 |

---

## 依赖与资源

### 外部依赖

- ✅ `serde_json` - JSON 序列化
- ✅ `actix-web` - WebSocket 支持
- ✅ `parking_lot` - 高性能锁
- ✅ `crossbeam` - 无锁通道

### 团队资源

| 角色 | 工时 | 任务 |
|------|------|------|
| 后端开发 | 32h | 阶段 1-2 |
| 前端开发 | 16h | 阶段 3 |
| 测试工程师 | 8h | 阶段 4 |

---

## 交付物清单

### 代码

- [ ] `src/protocol/diff/` - DIFF 协议模块
- [ ] `src/service/websocket/` - WebSocket 服务更新
- [ ] `web/src/utils/websocket.js` - 前端 WebSocket 客户端
- [ ] `web/src/store/modules/snapshot.js` - Vuex snapshot store

### 文档

- [x] `docs/DIFF_INTEGRATION.md` - 技术文档
- [x] `todo/diff_integration.md` - 实施计划
- [ ] `docs/WEBSOCKET_GUIDE.md` - WebSocket 使用指南
- [ ] `CLAUDE.md` - 更新协议说明

### 测试

- [ ] 单元测试（后端）
- [ ] 单元测试（前端）
- [ ] 集成测试
- [ ] 性能测试报告

---

## 里程碑

| 日期 | 里程碑 | 状态 |
|------|--------|------|
| Day 2 | 阶段 1 完成 | ⏳ |
| Day 4 | 阶段 2 完成 | ⏳ |
| Day 6 | 阶段 3 完成 | ⏳ |
| Day 7 | 阶段 4 完成 | ⏳ |
| Day 7 | 🎉 项目交付 | ⏳ |

---

## 后续迭代计划

### Phase 2 (Week 2)

- [ ] 图表数据支持（K线/Tick）
- [ ] 通知系统集成
- [ ] 银期转账功能

### Phase 3 (Week 3)

- [ ] 移动端适配
- [ ] 性能优化（无锁化）
- [ ] 集群部署支持

---

**批准**: _________
**日期**: 2025-10-05
**版本**: 1.0
