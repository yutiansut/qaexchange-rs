# DIFF 协议实施路线图

**项目**: QAExchange DIFF 协议完整实现
**方案**: 后端先行 → 前端跟进 → 联调测试
**文档语言**: 中文
**创建日期**: 2025-10-05

---

## 实施原则

1. ✅ **后端先行**: 先完成并测试后端功能
2. ✅ **增量交付**: 每完成一项立即更新文档和 CHANGELOG
3. ✅ **测试驱动**: 先写测试，再写实现
4. ✅ **中文优先**: 所有文档、注释、CHANGELOG 使用中文

---

## 总体进度

| 阶段 | 状态 | 完成度 | 预计时间 |
|------|------|--------|----------|
| 🔧 后端实现 | ⏳ 待开始 | 0% | 3 天 |
| 🧪 后端测试 | ⏳ 待开始 | 0% | 1 天 |
| 🎨 前端实现 | ⏳ 待开始 | 0% | 2 天 |
| 🧪 前端测试 | ⏳ 待开始 | 0% | 0.5 天 |
| 🔗 前后端联调 | ⏳ 待开始 | 0% | 0.5 天 |

---

## 第一阶段：后端基础设施（Day 1）

### 任务 1.1: 创建 JSON Merge Patch 实现

**状态**: ⏳ 待开始
**预计时间**: 2 小时
**负责模块**: `src/protocol/diff/merge.rs`

#### 实施步骤

- [ ] **1.1.1 创建模块文件**
  - 创建 `src/protocol/diff/merge.rs`
  - 添加模块导出到 `src/protocol/diff/mod.rs`

- [ ] **1.1.2 实现核心函数**
  ```rust
  /// JSON Merge Patch (RFC 7386)
  pub fn merge_patch(target: &mut Value, patch: &Value)

  /// 批量应用 Patch
  pub fn apply_patches(snapshot: &mut Value, patches: Vec<Value>)
  ```

- [ ] **1.1.3 编写单元测试**
  - 测试简单字段更新
  - 测试嵌套对象合并
  - 测试 null 值删除
  - 测试数组替换
  - RFC 7386 标准示例测试

#### 验收标准

- [ ] 测试覆盖率 ≥ 90%
- [ ] 通过 RFC 7386 所有标准测试用例
- [ ] Cargo test 全部通过

#### 文档更新

- [ ] **创建**: `docs/zh/json_merge_patch.md`
  - JSON Merge Patch 原理说明
  - 使用示例
  - 性能特性

- [ ] **更新**: `CHANGELOG.md`
  ```markdown
  ## [Unreleased]

  ### 新增 - JSON Merge Patch 实现
  - 实现 RFC 7386 JSON Merge Patch 标准
  - 支持嵌套对象合并和 null 值删除
  - 单元测试覆盖率 90%+
  - 文档: `docs/zh/json_merge_patch.md`
  ```

---

### 任务 1.2: 实现业务截面管理器

**状态**: ⏳ 待开始
**预计时间**: 4 小时
**负责模块**: `src/protocol/diff/snapshot.rs`

#### 实施步骤

- [ ] **1.2.1 定义数据结构**
  ```rust
  /// 业务截面管理器
  pub struct SnapshotManager {
      snapshot: Arc<RwLock<Value>>,
      pending_updates: Arc<RwLock<Vec<Value>>>,
      subscriptions: Arc<RwLock<SubscriptionState>>,
  }

  /// 订阅状态
  pub struct SubscriptionState {
      quote_subscriptions: HashMap<String, HashSet<String>>,
  }
  ```

- [ ] **1.2.2 实现核心方法**
  - `new()` - 创建管理器
  - `update(&self, patch: Value)` - 添加更新到队列
  - `peek(&self) -> Vec<Value>` - 获取待发送更新（阻塞）
  - `get_snapshot(&self) -> Value` - 获取完整截面
  - `subscribe(&self, user_id: String, instruments: Vec<String>)`

- [ ] **1.2.3 编写单元测试**
  - 测试并发更新
  - 测试 peek 阻塞等待
  - 测试订阅管理

#### 验收标准

- [ ] 支持多线程并发访问
- [ ] peek 正确阻塞直到有更新
- [ ] 测试覆盖率 ≥ 85%

#### 文档更新

- [ ] **创建**: `docs/zh/snapshot_manager.md`
  - 业务截面概念说明
  - SnapshotManager 使用指南
  - 线程安全保证

- [ ] **更新**: `CHANGELOG.md`
  ```markdown
  ### 新增 - 业务截面管理器
  - 实现 SnapshotManager 核心功能
  - 支持多线程并发安全访问
  - 实现 peek_message 阻塞等待机制
  - 文档: `docs/zh/snapshot_manager.md`
  ```

---

### 任务 1.3: 定义 DIFF 扩展数据类型

**状态**: ⏳ 待开始
**预计时间**: 2 小时
**负责模块**: `src/protocol/diff/*.rs`

#### 实施步骤

- [ ] **1.3.1 行情数据类型** (`src/protocol/diff/quotes.rs`)
  ```rust
  /// 行情数据（DIFF 扩展）
  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct Quote {
      pub instrument_id: String,
      pub datetime: String,
      pub last_price: f64,
      pub bid_price1: f64,
      pub ask_price1: f64,
      pub bid_volume1: i64,
      pub ask_volume1: i64,
      // ... 完整字段
  }
  ```

- [ ] **1.3.2 通知数据类型** (`src/protocol/diff/notify.rs`)
  ```rust
  /// 通知数据（DIFF 扩展）
  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct Notify {
      pub r#type: String,   // MESSAGE/TEXT/HTML
      pub level: String,    // INFO/WARNING/ERROR
      pub code: i32,
      pub content: String,
  }
  ```

- [ ] **1.3.3 成交记录类型** (`src/protocol/diff/trades.rs`)
  ```rust
  /// 成交记录（使用 QIFI 字段扩展）
  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct Trade {
      pub user_id: String,
      pub trade_id: String,
      pub order_id: String,
      // ... 参考 QIFI
  }
  ```

- [ ] **1.3.4 类型复用声明** (`src/protocol/diff/mod.rs`)
  ```rust
  use qars::qaprotocol::qifi::{Account, Position, Order};

  pub type DiffAccount = Account;
  pub type DiffPosition = Position;
  pub type DiffOrder = Order;
  ```

#### 验收标准

- [ ] 所有类型可序列化/反序列化
- [ ] 与 QIFI 类型对齐验证
- [ ] 编译通过无警告

#### 文档更新

- [ ] **更新**: `docs/DIFF_INTEGRATION.md`
  - 添加实际代码示例
  - 补充类型映射表

- [ ] **更新**: `CHANGELOG.md`
  ```markdown
  ### 新增 - DIFF 协议数据类型
  - 定义 Quote、Notify、Trade 扩展类型
  - 复用 QIFI Account/Position/Order 类型
  - 完整的 Serde 序列化支持
  ```

---

## 第二阶段：后端 WebSocket 集成（Day 2）

### 任务 2.1: 修改 WebSocket 消息类型

**状态**: ⏳ 待开始
**预计时间**: 2 小时
**负责模块**: `src/service/websocket/messages.rs`

#### 实施步骤

- [ ] **2.1.1 定义 DIFF 客户端消息**
  ```rust
  #[derive(Debug, Clone, Serialize, Deserialize)]
  #[serde(tag = "aid", rename_all = "snake_case")]
  pub enum DiffClientMessage {
      PeekMessage,
      #[serde(rename = "req_login")]
      ReqLogin { /* ... */ },
      #[serde(rename = "subscribe_quote")]
      SubscribeQuote { ins_list: String },
      #[serde(rename = "insert_order")]
      InsertOrder { /* ... */ },
      #[serde(rename = "cancel_order")]
      CancelOrder { /* ... */ },
  }
  ```

- [ ] **2.1.2 定义 DIFF 服务端消息**
  ```rust
  #[derive(Debug, Clone, Serialize, Deserialize)]
  #[serde(tag = "aid", rename_all = "snake_case")]
  pub enum DiffServerMessage {
      #[serde(rename = "rtn_data")]
      RtnData { data: Vec<Value> },
  }
  ```

- [ ] **2.1.3 消息转换函数**
  - TIFI → DIFF 转换
  - DIFF → TIFI 转换（向后兼容）

#### 验收标准

- [ ] 消息正确序列化/反序列化
- [ ] aid 字段匹配 DIFF 协议
- [ ] 单元测试通过

#### 文档更新

- [ ] **创建**: `docs/zh/websocket_protocol.md`
  - WebSocket 消息格式说明
  - 客户端/服务端消息列表
  - 使用示例

- [ ] **更新**: `CHANGELOG.md`
  ```markdown
  ### 新增 - DIFF 协议 WebSocket 消息
  - 定义 DiffClientMessage 和 DiffServerMessage
  - 支持 peek_message、subscribe_quote、insert_order 等
  - 文档: `docs/zh/websocket_protocol.md`
  ```

---

### 任务 2.2: 实现 peek_message 机制

**状态**: ⏳ 待开始
**预计时间**: 3 小时
**负责模块**: `src/service/websocket/session.rs`

#### 实施步骤

- [ ] **2.2.1 修改 WsSession 结构**
  ```rust
  pub struct WsSession {
      // ... 现有字段
      snapshot_manager: Option<Arc<SnapshotManager>>,
      peek_in_progress: bool,
  }
  ```

- [ ] **2.2.2 实现 peek_message 处理**
  ```rust
  async fn handle_peek_message(&mut self, ctx: &mut ws::WebsocketContext<Self>) {
      if let Some(manager) = &self.snapshot_manager {
          let patches = manager.peek().await;
          let msg = DiffServerMessage::RtnData { data: patches };
          ctx.text(serde_json::to_string(&msg).unwrap());
      }
  }
  ```

- [ ] **2.2.3 修改消息路由**
  - 识别 DIFF 消息
  - 路由到正确的处理函数

#### 验收标准

- [ ] peek_message 正确阻塞等待
- [ ] rtn_data 正确发送更新
- [ ] 多个客户端互不干扰

#### 文档更新

- [ ] **创建**: `docs/zh/peek_message_mechanism.md`
  - peek_message 工作原理
  - 时序图
  - 性能特性

- [ ] **更新**: `CHANGELOG.md`
  ```markdown
  ### 新增 - peek_message 推送机制
  - 实现 WebSocket peek_message 处理
  - 支持阻塞等待更新
  - rtn_data 自动推送
  - 文档: `docs/zh/peek_message_mechanism.md`
  ```

---

### 任务 2.3: 集成到业务逻辑

**状态**: ⏳ 待开始
**预计时间**: 3 小时
**负责模块**: `src/exchange/*.rs`

#### 实施步骤

- [ ] **2.3.1 AccountManager 集成**
  ```rust
  // src/exchange/account_mgr.rs
  impl AccountManager {
      pub fn set_snapshot_manager(&mut self, manager: Arc<SnapshotManager>) {
          self.snapshot_manager = Some(manager);
      }

      fn notify_snapshot_change(&self, user_id: &str, account: &Account) {
          if let Some(manager) = &self.snapshot_manager {
              let patch = json!({
                  "trade": {
                      user_id: {
                          "accounts": {
                              account.currency.clone(): account
                          }
                      }
                  }
              });
              manager.update(patch);
          }
      }
  }
  ```

- [ ] **2.3.2 OrderRouter 集成**
  - 订单状态变化推送

- [ ] **2.3.3 TradeGateway 集成**
  - 成交记录推送

#### 验收标准

- [ ] 账户变化实时推送
- [ ] 订单状态实时推送
- [ ] 成交记录实时推送

#### 文档更新

- [ ] **更新**: `docs/zh/snapshot_manager.md`
  - 添加业务集成示例

- [ ] **更新**: `CHANGELOG.md`
  ```markdown
  ### 新增 - 业务逻辑与截面同步集成
  - AccountManager 自动推送账户变化
  - OrderRouter 推送订单状态更新
  - TradeGateway 推送成交记录
  ```

---

## 第三阶段：后端测试（Day 3）

### 任务 3.1: 单元测试

**状态**: ⏳ 待开始
**预计时间**: 2 小时

#### 测试清单

- [ ] **JSON Merge Patch 测试**
  - `tests/protocol/diff/merge_test.rs`
  - RFC 7386 标准测试用例

- [ ] **SnapshotManager 测试**
  - `tests/protocol/diff/snapshot_test.rs`
  - 并发安全测试
  - peek 阻塞测试

- [ ] **消息序列化测试**
  - `tests/service/websocket/messages_test.rs`
  - DIFF 消息序列化/反序列化

#### 执行命令

```bash
cargo test --lib protocol::diff
cargo test --lib service::websocket
```

#### 验收标准

- [ ] 所有测试通过
- [ ] 覆盖率 ≥ 85%

#### 文档更新

- [ ] **创建**: `docs/zh/testing_guide.md`
  - 测试架构说明
  - 如何运行测试
  - 如何编写新测试

- [ ] **更新**: `CHANGELOG.md`
  ```markdown
  ### 测试 - 后端单元测试
  - JSON Merge Patch 单元测试（覆盖率 90%）
  - SnapshotManager 单元测试（覆盖率 85%）
  - WebSocket 消息测试
  - 文档: `docs/zh/testing_guide.md`
  ```

---

### 任务 3.2: 集成测试

**状态**: ⏳ 待开始
**预计时间**: 3 小时

#### 测试方案

- [ ] **3.2.1 WebSocket 服务器启动测试**
  ```rust
  // tests/integration/websocket_server_test.rs
  #[tokio::test]
  async fn test_websocket_server_starts() {
      // 启动服务器
      // 验证端口监听
  }
  ```

- [ ] **3.2.2 peek_message 循环测试**
  ```rust
  #[tokio::test]
  async fn test_peek_message_loop() {
      // 连接 WebSocket
      // 发送 peek_message
      // 验证收到 rtn_data
      // 再次发送 peek_message
  }
  ```

- [ ] **3.2.3 账户更新推送测试**
  ```rust
  #[tokio::test]
  async fn test_account_update_push() {
      // 连接 WebSocket
      // 发送 peek_message
      // 修改账户
      // 验证收到 rtn_data
      // 验证 Merge Patch 内容
  }
  ```

#### 工具准备

- [ ] 安装 `websocat`
  ```bash
  cargo install websocat
  ```

- [ ] 编写测试脚本
  ```bash
  # test_websocket.sh
  websocat ws://127.0.0.1:8081/ws?user_id=test_user
  ```

#### 验收标准

- [ ] WebSocket 服务正常启动
- [ ] peek_message 循环正常工作
- [ ] 数据推送实时准确

#### 文档更新

- [ ] **创建**: `docs/zh/integration_testing.md`
  - 集成测试说明
  - 使用 websocat 测试 WebSocket
  - 测试用例说明

- [ ] **更新**: `CHANGELOG.md`
  ```markdown
  ### 测试 - 后端集成测试
  - WebSocket 服务器集成测试
  - peek_message 循环测试
  - 账户更新推送测试
  - 文档: `docs/zh/integration_testing.md`
  ```

---

### 任务 3.3: 性能测试

**状态**: ⏳ 待开始
**预计时间**: 3 小时

#### 测试项目

- [ ] **JSON Merge Patch 性能**
  ```rust
  // benches/merge_patch_bench.rs
  use criterion::{black_box, criterion_group, criterion_main, Criterion};

  fn bench_merge_patch(c: &mut Criterion) {
      c.bench_function("merge_patch", |b| {
          b.iter(|| merge_patch(black_box(&mut target), black_box(&patch)));
      });
  }
  ```

- [ ] **SnapshotManager 并发性能**
  - 1000 个并发更新
  - 100 个并发 peek

- [ ] **WebSocket 吞吐量**
  - 100 个并发连接
  - 每秒推送次数

#### 性能目标

| 指标 | 目标 | 实测 |
|------|------|------|
| Merge Patch 性能 | > 10K ops/s | - |
| peek 延迟 | < 10ms (P99) | - |
| WebSocket 并发 | > 100 连接 | - |

#### 验收标准

- [ ] 所有性能指标达标
- [ ] 无内存泄漏

#### 文档更新

- [ ] **创建**: `docs/zh/performance_report.md`
  - 性能测试结果
  - 性能优化建议

- [ ] **更新**: `CHANGELOG.md`
  ```markdown
  ### 测试 - 后端性能测试
  - JSON Merge Patch: 12K ops/s
  - peek 延迟: P99 < 8ms
  - WebSocket 并发: 150 连接稳定
  - 文档: `docs/zh/performance_report.md`
  ```

---

## 第四阶段：前端实现（Day 4-5）

### 任务 4.1: WebSocket 客户端实现

**状态**: ⏳ 待开始
**预计时间**: 3 小时
**负责模块**: `web/src/utils/websocket.js`

#### 实施步骤

- [ ] **4.1.1 创建 WebSocket 客户端类**
  ```javascript
  // web/src/utils/websocket.js
  class DiffWebSocket {
      constructor(url, userId) {
          this.url = url
          this.userId = userId
          this.ws = null
          this.snapshot = {}
          this.callbacks = []
          this.reconnectDelay = 1000
      }

      connect() {
          this.ws = new WebSocket(`${this.url}?user_id=${this.userId}`)
          this.ws.onopen = () => this.onOpen()
          this.ws.onmessage = (event) => this.onMessage(event)
          this.ws.onclose = () => this.onClose()
          this.ws.onerror = (error) => this.onError(error)
      }

      onOpen() {
          console.log('WebSocket 已连接')
          this.startPeekLoop()
      }

      startPeekLoop() {
          this.send({ aid: 'peek_message' })
      }

      onMessage(event) {
          const msg = JSON.parse(event.data)
          if (msg.aid === 'rtn_data') {
              this.handleRtnData(msg.data)
              this.startPeekLoop()  // 继续下一轮
          }
      }

      handleRtnData(patches) {
          for (const patch of patches) {
              mergePatch(this.snapshot, patch)
          }
          this.notifyCallbacks()
      }
  }
  ```

- [ ] **4.1.2 实现断线重连**
  ```javascript
  onClose() {
      console.log('WebSocket 断开，尝试重连...')
      setTimeout(() => this.connect(), this.reconnectDelay)
      this.reconnectDelay = Math.min(this.reconnectDelay * 2, 30000)
  }
  ```

- [ ] **4.1.3 实现心跳保活**
  ```javascript
  startHeartbeat() {
      this.heartbeatTimer = setInterval(() => {
          if (this.ws.readyState === WebSocket.OPEN) {
              this.send({ aid: 'ping' })
          }
      }, 30000)  // 30 秒
  }
  ```

#### 验收标准

- [ ] WebSocket 连接正常
- [ ] 断线自动重连
- [ ] 心跳正常工作

#### 文档更新

- [ ] **创建**: `web/docs/zh/websocket_client.md`
  - WebSocket 客户端使用指南
  - API 文档
  - 示例代码

- [ ] **更新**: `CHANGELOG.md`
  ```markdown
  ### 新增 - 前端 WebSocket 客户端
  - 实现 DiffWebSocket 类
  - 支持自动重连和心跳保活
  - 文档: `web/docs/zh/websocket_client.md`
  ```

---

### 任务 4.2: JSON Merge Patch 前端实现

**状态**: ⏳ 待开始
**预计时间**: 2 小时
**负责模块**: `web/src/utils/merge-patch.js`

#### 实施步骤

- [ ] **4.2.1 实现 Merge Patch 函数**
  ```javascript
  // web/src/utils/merge-patch.js

  /**
   * JSON Merge Patch (RFC 7386)
   * @param {Object} target - 目标对象
   * @param {Object} patch - 补丁对象
   * @returns {Object} 合并后的对象
   */
  export function mergePatch(target, patch) {
      // 如果 patch 不是对象，直接替换
      if (typeof patch !== 'object' || patch === null || Array.isArray(patch)) {
          return patch
      }

      // 如果 target 不是对象，初始化为空对象
      if (typeof target !== 'object' || target === null || Array.isArray(target)) {
          target = {}
      }

      // 遍历 patch 的所有字段
      for (const [key, value] of Object.entries(patch)) {
          if (value === null) {
              // null 表示删除字段
              delete target[key]
          } else if (typeof value === 'object' && !Array.isArray(value)) {
              // 对象类型，递归合并
              target[key] = mergePatch(target[key] || {}, value)
          } else {
              // 其他类型，直接替换
              target[key] = value
          }
      }

      return target
  }

  /**
   * 批量应用 Merge Patch
   * @param {Object} target - 目标对象
   * @param {Array} patches - 补丁数组
   * @returns {Object} 合并后的对象
   */
  export function applyPatches(target, patches) {
      for (const patch of patches) {
          mergePatch(target, patch)
      }
      return target
  }
  ```

- [ ] **4.2.2 编写单元测试**
  ```javascript
  // web/tests/unit/merge-patch.spec.js
  import { mergePatch } from '@/utils/merge-patch'

  describe('mergePatch', () => {
      it('应该合并简单字段', () => {
          const target = { a: 1, b: 2 }
          const patch = { b: 3, c: 4 }
          mergePatch(target, patch)
          expect(target).toEqual({ a: 1, b: 3, c: 4 })
      })

      it('应该删除 null 字段', () => {
          const target = { a: 1, b: 2 }
          const patch = { b: null }
          mergePatch(target, patch)
          expect(target).toEqual({ a: 1 })
      })

      it('应该递归合并嵌套对象', () => {
          const target = { a: { b: 1, c: 2 } }
          const patch = { a: { c: 3, d: 4 } }
          mergePatch(target, patch)
          expect(target).toEqual({ a: { b: 1, c: 3, d: 4 } })
      })
  })
  ```

#### 验收标准

- [ ] 单元测试全部通过
- [ ] 符合 RFC 7386 标准

#### 文档更新

- [ ] **创建**: `web/docs/zh/merge_patch.md`
  - JSON Merge Patch 原理
  - 使用示例
  - 性能说明

- [ ] **更新**: `CHANGELOG.md`
  ```markdown
  ### 新增 - 前端 JSON Merge Patch
  - 实现 RFC 7386 标准
  - 支持嵌套对象合并
  - 单元测试覆盖率 100%
  - 文档: `web/docs/zh/merge_patch.md`
  ```

---

### 任务 4.3: Vuex Snapshot Store

**状态**: ⏳ 待开始
**预计时间**: 3 小时
**负责模块**: `web/src/store/modules/snapshot.js`

#### 实施步骤

- [ ] **4.3.1 创建 Snapshot Store**
  ```javascript
  // web/src/store/modules/snapshot.js
  import { mergePatch } from '@/utils/merge-patch'
  import DiffWebSocket from '@/utils/websocket'

  const state = {
      snapshot: {},      // 业务截面镜像
      connected: false,  // 连接状态
      ws: null           // WebSocket 实例
  }

  const mutations = {
      UPDATE_SNAPSHOT(state, patches) {
          for (const patch of patches) {
              mergePatch(state.snapshot, patch)
          }
      },

      SET_CONNECTED(state, connected) {
          state.connected = connected
      },

      SET_WS(state, ws) {
          state.ws = ws
      }
  }

  const actions = {
      connect({ commit, rootState }) {
          const ws = new DiffWebSocket(
              process.env.VUE_APP_WS_URL,
              rootState.user.currentUser
          )

          ws.onUpdate((patches) => {
              commit('UPDATE_SNAPSHOT', patches)
          })

          ws.onConnected(() => {
              commit('SET_CONNECTED', true)
          })

          ws.onDisconnected(() => {
              commit('SET_CONNECTED', false)
          })

          ws.connect()
          commit('SET_WS', ws)
      },

      disconnect({ state, commit }) {
          if (state.ws) {
              state.ws.close()
              commit('SET_WS', null)
              commit('SET_CONNECTED', false)
          }
      },

      subscribeQuote({ state }, instruments) {
          if (state.ws) {
              state.ws.send({
                  aid: 'subscribe_quote',
                  ins_list: instruments.join(',')
              })
          }
      }
  }

  const getters = {
      // 交易数据
      tradeData: (state) => (userId) => {
          return state.snapshot.trade?.[userId] || {}
      },

      // 账户
      accounts: (state) => (userId) => {
          return state.snapshot.trade?.[userId]?.accounts || {}
      },

      // 持仓
      positions: (state) => (userId) => {
          return state.snapshot.trade?.[userId]?.positions || {}
      },

      // 订单
      orders: (state) => (userId) => {
          return state.snapshot.trade?.[userId]?.orders || {}
      },

      // 行情
      quotes: (state) => {
          return state.snapshot.quotes || {}
      },

      // 特定合约行情
      quote: (state) => (instrumentId) => {
          return state.snapshot.quotes?.[instrumentId] || {}
      }
  }

  export default {
      namespaced: true,
      state,
      mutations,
      actions,
      getters
  }
  ```

- [ ] **4.3.2 集成到主 Store**
  ```javascript
  // web/src/store/index.js
  import snapshot from './modules/snapshot'

  export default new Vuex.Store({
      modules: {
          user,
          snapshot  // 新增
      }
  })
  ```

#### 验收标准

- [ ] Vuex store 正常工作
- [ ] Getters 返回正确数据
- [ ] 响应式更新正常

#### 文档更新

- [ ] **创建**: `web/docs/zh/vuex_snapshot.md`
  - Snapshot Store 使用指南
  - Getters 文档
  - 最佳实践

- [ ] **更新**: `CHANGELOG.md`
  ```markdown
  ### 新增 - Vuex Snapshot Store
  - 实现业务截面 Vuex 状态管理
  - 支持账户、持仓、订单、行情数据访问
  - 响应式更新
  - 文档: `web/docs/zh/vuex_snapshot.md`
  ```

---

### 任务 4.4: 修改交易页面

**状态**: ⏳ 待开始
**预计时间**: 2 小时
**负责模块**: `web/src/views/trade/index.vue`

#### 实施步骤

- [ ] **4.4.1 移除 HTTP 轮询**
  ```javascript
  // 删除以下代码
  // this.refreshTimer = setInterval(() => {
  //   this.loadOrderBook()
  //   this.loadTick()
  // }, 1000)
  ```

- [ ] **4.4.2 使用 Vuex Getters**
  ```vue
  <script>
  import { mapGetters, mapActions } from 'vuex'

  export default {
      computed: {
          ...mapGetters('snapshot', [
              'quote',
              'accounts',
              'orders',
              'positions'
          ]),

          // 当前合约行情
          currentQuote() {
              return this.quote(this.selectedInstrument)
          },

          // 当前账户
          currentAccount() {
              const accounts = this.accounts(this.$store.getters.currentUser)
              return accounts.CNY || {}
          }
      },

      methods: {
          ...mapActions('snapshot', ['subscribeQuote']),

          handleInstrumentChange(instrumentId) {
              this.selectedInstrument = instrumentId
              this.subscribeQuote([instrumentId])
          }
      },

      mounted() {
          // 订阅默认合约
          this.subscribeQuote([this.selectedInstrument])
      }
  }
  </script>
  ```

- [ ] **4.4.3 更新模板绑定**
  ```vue
  <template>
      <div class="market-info">
          <div class="last-price">
              {{ currentQuote.last_price || '--' }}
          </div>
          <div class="bid-ask">
              <span>买: {{ currentQuote.bid_price1 || '--' }}</span>
              <span>卖: {{ currentQuote.ask_price1 || '--' }}</span>
          </div>
      </div>
  </template>
  ```

#### 验收标准

- [ ] 页面数据实时更新
- [ ] 无 HTTP 轮询
- [ ] UI 响应流畅

#### 文档更新

- [ ] **更新**: `web/docs/zh/trade_page.md`
  - 交易页面使用说明
  - WebSocket 实时数据绑定

- [ ] **更新**: `CHANGELOG.md`
  ```markdown
  ### 更新 - 交易页面 WebSocket 集成
  - 移除 HTTP 轮询逻辑
  - 使用 Vuex snapshot 实时数据
  - 行情数据实时更新
  - 文档: `web/docs/zh/trade_page.md`
  ```

---

## 第五阶段：前端测试（Day 5 下午）

### 任务 5.1: 前端单元测试

**状态**: ⏳ 待开始
**预计时间**: 2 小时

#### 测试清单

- [ ] **Merge Patch 测试**
  - `web/tests/unit/merge-patch.spec.js`

- [ ] **WebSocket 客户端测试**
  - `web/tests/unit/websocket.spec.js`
  - Mock WebSocket API

- [ ] **Vuex Store 测试**
  - `web/tests/unit/store/snapshot.spec.js`

#### 执行命令

```bash
cd web
npm run test:unit
```

#### 验收标准

- [ ] 所有测试通过
- [ ] 覆盖率 ≥ 80%

#### 文档更新

- [ ] **更新**: `CHANGELOG.md`
  ```markdown
  ### 测试 - 前端单元测试
  - Merge Patch 单元测试
  - WebSocket 客户端测试
  - Vuex Store 测试
  - 覆盖率: 85%
  ```

---

### 任务 5.2: 前端 E2E 测试

**状态**: ⏳ 待开始
**预计时间**: 2 小时

#### 测试方案

- [ ] **5.2.1 WebSocket 连接测试**
  ```javascript
  // web/tests/e2e/specs/websocket.spec.js
  describe('WebSocket 连接', () => {
      it('应该成功连接并订阅行情', () => {
          cy.visit('/trade')
          cy.get('.ws-status').should('contain', '已连接')
          cy.get('.last-price').should('not.be.empty')
      })
  })
  ```

- [ ] **5.2.2 实时数据更新测试**
  ```javascript
  it('应该实时更新行情数据', () => {
      // 监听 WebSocket 消息
      // 验证页面数据更新
  })
  ```

#### 执行命令

```bash
npm run test:e2e
```

#### 验收标准

- [ ] E2E 测试全部通过
- [ ] 覆盖核心用户流程

#### 文档更新

- [ ] **创建**: `web/docs/zh/e2e_testing.md`
  - E2E 测试指南
  - Cypress 使用说明

- [ ] **更新**: `CHANGELOG.md`
  ```markdown
  ### 测试 - 前端 E2E 测试
  - WebSocket 连接测试
  - 实时数据更新测试
  - 文档: `web/docs/zh/e2e_testing.md`
  ```

---

## 第六阶段：前后端联调（Day 6）

### 任务 6.1: 本地联调

**状态**: ⏳ 待开始
**预计时间**: 2 小时

#### 联调步骤

- [ ] **6.1.1 启动后端服务**
  ```bash
  cargo run --bin qaexchange-server
  ```

- [ ] **6.1.2 启动前端开发服务器**
  ```bash
  cd web
  npm run serve
  ```

- [ ] **6.1.3 验证功能**
  - [ ] 登录后 WebSocket 自动连接
  - [ ] peek_message 循环正常
  - [ ] 下单后订单状态实时更新
  - [ ] 账户余额实时更新
  - [ ] 行情数据实时刷新

#### 调试工具

- [ ] Chrome DevTools Network → WS
- [ ] 后端日志: `RUST_LOG=debug`
- [ ] 前端日志: `console.log`

#### 验收标准

- [ ] 所有功能正常
- [ ] 无错误日志
- [ ] 延迟 < 100ms

#### 文档更新

- [ ] **创建**: `docs/zh/local_development.md`
  - 本地开发环境搭建
  - 联调步骤
  - 常见问题排查

- [ ] **更新**: `CHANGELOG.md`
  ```markdown
  ### 联调 - 前后端本地联调完成
  - WebSocket 实时通信正常
  - 数据同步准确无误
  - 延迟: P99 < 50ms
  - 文档: `docs/zh/local_development.md`
  ```

---

### 任务 6.2: 压力测试

**状态**: ⏳ 待开始
**预计时间**: 2 小时

#### 测试场景

- [ ] **6.2.1 并发连接测试**
  - 100 个并发 WebSocket 连接
  - 验证服务稳定性

- [ ] **6.2.2 高频推送测试**
  - 每秒 100 次行情更新
  - 验证前端渲染性能

- [ ] **6.2.3 长时间运行测试**
  - 连续运行 1 小时
  - 验证无内存泄漏

#### 测试工具

- [ ] JMeter WebSocket 插件
- [ ] Chrome Performance Monitor
- [ ] 后端 Prometheus 监控

#### 验收标准

- [ ] 100 并发连接稳定
- [ ] 前端帧率 > 30 FPS
- [ ] 无内存泄漏

#### 文档更新

- [ ] **更新**: `docs/zh/performance_report.md`
  - 添加前后端联调性能数据

- [ ] **更新**: `CHANGELOG.md`
  ```markdown
  ### 测试 - 压力测试完成
  - 并发连接: 150 稳定运行
  - 推送频率: 150 次/秒
  - 内存占用: 稳定在 200MB
  - 无内存泄漏
  ```

---

### 任务 6.3: 文档最终完善

**状态**: ⏳ 待开始
**预计时间**: 2 小时

#### 文档清单

- [ ] **用户文档**
  - [ ] `docs/zh/USER_GUIDE.md` - 用户使用指南
  - [ ] `docs/zh/FAQ.md` - 常见问题

- [ ] **开发文档**
  - [ ] `docs/zh/DEVELOPER_GUIDE.md` - 开发者指南
  - [ ] `docs/zh/API_REFERENCE.md` - API 参考

- [ ] **部署文档**
  - [ ] `docs/zh/DEPLOYMENT.md` - 部署指南
  - [ ] `docs/zh/CONFIGURATION.md` - 配置说明

#### 验收标准

- [ ] 所有文档完整
- [ ] 代码示例可运行
- [ ] 无拼写错误

#### CHANGELOG 最终更新

- [ ] **更新**: `CHANGELOG.md`
  ```markdown
  ## [Unreleased]

  ### 新增 - DIFF 协议完整实现 (2025-10-05)

  #### 后端功能
  - ✅ JSON Merge Patch (RFC 7386)
  - ✅ 业务截面管理器 (SnapshotManager)
  - ✅ DIFF 协议数据类型 (Quote, Notify, Trade)
  - ✅ WebSocket peek_message 机制
  - ✅ 业务逻辑与截面集成

  #### 前端功能
  - ✅ WebSocket 客户端 (自动重连 + 心跳)
  - ✅ JSON Merge Patch 前端实现
  - ✅ Vuex Snapshot Store
  - ✅ 交易页面实时更新

  #### 测试
  - ✅ 后端单元测试 (覆盖率 88%)
  - ✅ 后端集成测试
  - ✅ 后端性能测试
  - ✅ 前端单元测试 (覆盖率 85%)
  - ✅ 前端 E2E 测试
  - ✅ 前后端联调测试
  - ✅ 压力测试 (150 并发连接)

  #### 文档
  - ✅ 技术方案: `docs/DIFF_INTEGRATION.md`
  - ✅ 实施计划: `todo/diff_integration.md`
  - ✅ 中文文档: `docs/zh/*.md` (15 篇)
  - ✅ 用户指南、开发指南、API 参考

  #### 性能指标
  - Merge Patch: 12K ops/s
  - peek 延迟: P99 < 8ms
  - WebSocket 并发: 150 连接
  - 推送频率: 150 次/秒
  - 前端帧率: > 30 FPS

  #### 兼容性
  - ✅ 100% 复用 QIFI/TIFI 协议
  - ✅ 零迁移成本
  - ✅ 向后兼容

  **协议体系**: QIFI (数据层) + TIFI (传输层) + DIFF (同步层)
  ```

---

## 里程碑时间线

```
Day 1    Day 2    Day 3    Day 4    Day 5    Day 6
 │        │        │        │        │        │
 ├─后端基础设施    ├─后端测试     ├─前端实现    │
 │  • Merge Patch │  • 单元测试   │  • WebSocket │
 │  • Snapshot    │  • 集成测试   │  • Vuex      │
 │  • DIFF 类型   │  • 性能测试   │  • 页面更新  │
 │                │               │             │
 ├─WebSocket集成                  ├─前端测试    ├─联调
 │  • peek机制                    │  • 单元     │  • 本地
 │  • 业务集成                    │  • E2E      │  • 压力
 │  • 消息处理                    │             │  • 文档
 │                                │             │
 └────────────────────────────────┴─────────────┴───✅
```

---

## 交付检查清单

### 代码

- [ ] `src/protocol/diff/` - DIFF 协议模块
- [ ] `src/service/websocket/` - WebSocket 更新
- [ ] `web/src/utils/websocket.js` - 前端 WebSocket
- [ ] `web/src/utils/merge-patch.js` - 前端 Merge Patch
- [ ] `web/src/store/modules/snapshot.js` - Vuex Store
- [ ] `web/src/views/trade/index.vue` - 交易页面更新

### 测试

- [ ] 后端单元测试 (覆盖率 ≥ 85%)
- [ ] 后端集成测试
- [ ] 后端性能测试
- [ ] 前端单元测试 (覆盖率 ≥ 80%)
- [ ] 前端 E2E 测试
- [ ] 联调测试

### 文档（全部中文）

- [ ] `docs/DIFF_INTEGRATION.md`
- [ ] `docs/zh/json_merge_patch.md`
- [ ] `docs/zh/snapshot_manager.md`
- [ ] `docs/zh/websocket_protocol.md`
- [ ] `docs/zh/peek_message_mechanism.md`
- [ ] `docs/zh/testing_guide.md`
- [ ] `docs/zh/integration_testing.md`
- [ ] `docs/zh/performance_report.md`
- [ ] `docs/zh/websocket_client.md`
- [ ] `docs/zh/merge_patch.md`
- [ ] `docs/zh/vuex_snapshot.md`
- [ ] `docs/zh/trade_page.md`
- [ ] `docs/zh/e2e_testing.md`
- [ ] `docs/zh/local_development.md`
- [ ] `docs/zh/USER_GUIDE.md`
- [ ] `docs/zh/DEVELOPER_GUIDE.md`

### CHANGELOG

- [ ] 每个任务完成后更新 CHANGELOG.md
- [ ] 最终版本完整 CHANGELOG

---

## 备注

1. **文档语言**: 所有文档必须使用中文
2. **增量交付**: 每完成一个任务立即更新文档和 CHANGELOG
3. **测试优先**: 先写测试，再写实现
4. **版本控制**: 每个阶段完成后提交 Git

---

**开始日期**: 2025-10-05
**预计完成**: 2025-10-11 (6 天)
**状态**: ⏳ 待开始
