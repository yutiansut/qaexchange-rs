# Account ID 重构计划

## 问题诊断

**当前问题**：
- 前端未明确传递 `account_id`，仅传递 `user_id`
- 后端使用 `get_default_account(user_id)` 来猜测账户
- 交易系统核心混用 `user_id` 和 `account_id`
- 缺少账户所有权验证

**正确架构**：
```
前端 → 明确传递 (user_id + account_id)
  ↓
服务层 → 验证 account_id 是否属于 user_id
  ↓
交易层 → 只关心 account_id
```

---

## 重构计划

### Phase 1: 协议定义（后端）

#### 1.1 HTTP API Models

**文件**: `src/service/http/models.rs`

**修改**:
```rust
// 订单提交请求（外部接口）
pub struct SubmitOrderRequest {
    pub user_id: String,          // 用户身份（用于验证）
    pub account_id: String,        // 交易账户（必填）✨ NEW
    pub instrument_id: String,
    pub direction: String,
    pub offset: String,
    pub volume: f64,
    pub price: f64,
    pub order_type: String,
}

// 撤单请求（外部接口）
pub struct CancelOrderRequest {
    pub user_id: String,           // 用户身份（用于验证）
    pub account_id: String,        // 交易账户（必填）✨ NEW
    pub order_id: String,
}
```

#### 1.2 WebSocket Messages

**文件**: `src/service/websocket/messages.rs`

**修改**:
```rust
pub enum ClientMessage {
    // ...existing variants...

    SubmitOrder {
        account_id: String,        // ✨ NEW: 明确账户ID
        instrument_id: String,
        direction: String,
        offset: String,
        volume: f64,
        price: f64,
        order_type: String,
    },

    CancelOrder {
        account_id: String,        // ✨ NEW: 明确账户ID
        order_id: String,
    },
}
```

#### 1.3 DIFF 协议扩展

**文件**: `src/service/websocket/diff_handler.rs`

**DIFF insert_order 协议扩展**:
```json
{
  "aid": "insert_order",
  "user_id": "user123",           // 用户身份
  "account_id": "ACC_xxx",        // 账户ID（新增）✨
  "order_id": "order_001",
  "exchange_id": "SHFE",
  "instrument_id": "cu2501",
  "direction": "BUY",
  "offset": "OPEN",
  "volume": 1,
  "price_type": "LIMIT",
  "limit_price": 50000
}
```

**DIFF cancel_order 协议扩展**:
```json
{
  "aid": "cancel_order",
  "user_id": "user123",           // 用户身份
  "account_id": "ACC_xxx",        // 账户ID（新增）✨
  "order_id": "order_001"
}
```

---

### Phase 2: 后端验证层

#### 2.1 账户所有权验证

**文件**: `src/exchange/account_mgr.rs`

**新增方法**:
```rust
impl AccountManager {
    /// 验证账户是否属于指定用户
    ///
    /// # 参数
    /// - `account_id`: 账户ID
    /// - `user_id`: 用户ID
    ///
    /// # 返回
    /// - Ok(()) - 验证通过
    /// - Err(ExchangeError) - 验证失败
    pub fn verify_account_ownership(
        &self,
        account_id: &str,
        user_id: &str
    ) -> Result<(), ExchangeError> {
        // 1. 检查账户是否存在
        let metadata = self.metadata.get(account_id)
            .ok_or_else(|| ExchangeError::AccountError(
                format!("Account not found: {}", account_id)
            ))?;

        // 2. 检查账户所有权
        if metadata.user_id != user_id {
            return Err(ExchangeError::PermissionDenied(
                format!(
                    "Account {} does not belong to user {}",
                    account_id, user_id
                )
            ));
        }

        Ok(())
    }
}
```

#### 2.2 HTTP Handlers 验证

**文件**: `src/service/http/handlers.rs`

**修改**:
```rust
pub async fn submit_order(
    req: web::Json<SubmitOrderRequest>,
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse> {
    // 1. 验证账户所有权
    if let Err(e) = state.account_mgr.verify_account_ownership(
        &req.account_id,
        &req.user_id
    ) {
        return Ok(HttpResponse::Forbidden().json(
            ApiResponse::<()>::error(403, format!("Permission denied: {}", e))
        ));
    }

    // 2. 构造交易层请求（只传 account_id）
    let core_req = CoreSubmitOrderRequest {
        account_id: req.account_id.clone(),
        instrument_id: req.instrument_id.clone(),
        direction: req.direction.clone(),
        offset: req.offset.clone(),
        volume: req.volume,
        price: req.price,
        order_type: req.order_type.clone(),
    };

    // 3. 提交到交易层
    let response = state.order_router.submit_order(core_req);
    // ...
}
```

#### 2.3 WebSocket Handlers 验证

**文件**: `src/service/websocket/handler.rs`

**修改**:
```rust
ClientMessage::SubmitOrder {
    account_id,
    instrument_id,
    direction,
    // ...
} => {
    // 1. 验证账户所有权
    if let Err(e) = self.account_mgr.verify_account_ownership(
        &account_id,
        &user_id
    ) {
        let server_msg = ServerMessage::OrderResponse {
            success: false,
            order_id: None,
            error_code: Some(403),
            error_message: Some(format!("Permission denied: {}", e)),
        };
        session_addr.do_send(SendMessage(server_msg));
        continue;
    }

    // 2. 构造交易层请求
    let req = SubmitOrderRequest {
        account_id,  // 直接使用前端传来的 account_id
        instrument_id,
        // ...
    };
    // ...
}
```

---

### Phase 3: 前端适配

#### 3.1 Vuex Store - 账户管理

**文件**: `web/src/store/modules/websocket.js`

**修改**:
```javascript
const state = {
  // 当前选中的账户ID
  currentAccountId: null,  // ✨ NEW

  // 用户的所有账户列表
  userAccounts: [],        // ✨ NEW

  // ...existing state...
}

const mutations = {
  SET_CURRENT_ACCOUNT(state, accountId) {
    state.currentAccountId = accountId
  },

  SET_USER_ACCOUNTS(state, accounts) {
    state.userAccounts = accounts
  },
  // ...
}

const actions = {
  // 初始化时获取用户账户列表
  async initWebSocket({ commit, rootState, dispatch }) {
    // ...existing code...

    // 获取用户账户列表
    const accounts = await this.fetchUserAccounts(userId)
    commit('SET_USER_ACCOUNTS', accounts)

    // 设置默认账户
    if (accounts.length > 0) {
      commit('SET_CURRENT_ACCOUNT', accounts[0].account_id)
    }
  },

  // 下单时传递 account_id
  insertOrder({ state, rootState }, order) {
    const orderWithAccount = {
      user_id: rootState.currentUser,
      account_id: state.currentAccountId,  // ✨ 明确传递账户ID
      ...order
    }

    state.ws.insertOrder(orderWithAccount)
  },
}
```

#### 3.2 订单表单 - 账户选择器

**文件**: `web/src/views/WebSocketTest.vue`

**新增组件**:
```vue
<template>
  <div class="order-form">
    <!-- 账户选择器 ✨ NEW -->
    <el-form-item label="交易账户">
      <el-select v-model="selectedAccountId" placeholder="选择账户">
        <el-option
          v-for="account in userAccounts"
          :key="account.account_id"
          :label="`${account.account_name} (${account.account_id})`"
          :value="account.account_id"
        />
      </el-select>
    </el-form-item>

    <!-- 其他表单项 -->
    <el-form-item label="合约">
      <el-input v-model="orderForm.instrument_id" />
    </el-form-item>
    <!-- ... -->
  </div>
</template>

<script>
export default {
  computed: {
    ...mapState('websocket', ['userAccounts', 'currentAccountId']),

    selectedAccountId: {
      get() {
        return this.currentAccountId
      },
      set(value) {
        this.$store.commit('websocket/SET_CURRENT_ACCOUNT', value)
      }
    }
  },

  methods: {
    submitOrder() {
      this.$store.dispatch('websocket/insertOrder', {
        account_id: this.selectedAccountId,  // ✨ 明确传递
        instrument_id: this.orderForm.instrument_id,
        // ...
      })
    }
  }
}
</script>
```

#### 3.3 DIFF 协议客户端

**文件**: `web/src/websocket/DiffProtocol.js`

**修改**:
```javascript
/**
 * 创建下单消息
 * @param {Object} order - 订单对象
 * @param {string} order.user_id - 用户ID
 * @param {string} order.account_id - 账户ID ✨ NEW (必填)
 * @param {string} order.instrument_id - 合约ID
 * // ...
 */
createInsertOrder(order) {
  if (!order.account_id) {
    throw new Error('account_id is required')
  }

  return JSON.stringify({
    aid: 'insert_order',
    user_id: order.user_id,
    account_id: order.account_id,  // ✨ NEW
    order_id: order.order_id,
    exchange_id: order.exchange_id,
    instrument_id: order.instrument_id,
    // ...
  })
}

/**
 * 创建撤单消息
 * @param {string} userId - 用户ID
 * @param {string} accountId - 账户ID ✨ NEW (必填)
 * @param {string} orderId - 订单ID
 */
createCancelOrder(userId, accountId, orderId) {
  if (!accountId) {
    throw new Error('accountId is required')
  }

  return JSON.stringify({
    aid: 'cancel_order',
    user_id: userId,
    account_id: accountId,  // ✨ NEW
    order_id: orderId
  })
}
```

---

### Phase 4: 文档更新

#### 4.1 API 文档

**文件**: `docs/API.md`

#### 4.2 WebSocket 协议文档

**文件**: `docs/WEBSOCKET_PROTOCOL.md`

#### 4.3 迁移指南

**文件**: `docs/MIGRATION_ACCOUNT_ID.md`

---

## 实施顺序

### Step 1: 后端基础设施（不破坏现有功能）
1. ✅ 添加 `verify_account_ownership` 方法
2. ✅ HTTP/WebSocket models 添加 `account_id` 字段（可选）
3. ✅ 保持向后兼容：`account_id` 为空时使用 `get_default_account`

### Step 2: 前端适配
1. ✅ Vuex store 添加账户管理
2. ✅ 订单表单添加账户选择器
3. ✅ DIFF 协议客户端传递 `account_id`

### Step 3: 强制验证（切换到新架构）
1. ✅ HTTP/WebSocket handlers 强制要求 `account_id`
2. ✅ 添加账户所有权验证
3. ✅ 移除 `get_default_account` 的使用

### Step 4: 文档和测试
1. ✅ 更新所有 API 文档
2. ✅ 编写迁移指南
3. ✅ 集成测试验证

---

## 验证清单

- [ ] HTTP POST /api/order/submit 需要 `account_id`
- [ ] HTTP POST /api/order/cancel 需要 `account_id`
- [ ] WebSocket submit_order 需要 `account_id`
- [ ] WebSocket cancel_order 需要 `account_id`
- [ ] DIFF insert_order 需要 `account_id`
- [ ] DIFF cancel_order 需要 `account_id`
- [ ] 所有接口验证账户所有权
- [ ] 前端订单表单有账户选择器
- [ ] 前端正确传递 `account_id`
- [ ] 交易层只使用 `account_id`（不使用 `user_id`）

---

## 风险评估

**兼容性风险**: ⚠️ 高
- 旧客户端未传递 `account_id` 会失败
- 需要明确的迁移期和废弃通知

**缓解措施**:
1. 第一阶段保持向后兼容（`account_id` 可选）
2. 提供详细的迁移文档和示例
3. 设置 deprecation warning
4. 分阶段强制要求（先 warning，后 error）

---

## 预期收益

1. **安全性提升**: 明确验证账户所有权，防止跨账户操作
2. **架构清晰**: 前端明确传递，后端只验证，交易层专注业务
3. **多账户支持**: 用户可以轻松切换多个交易账户
4. **可维护性**: 消除隐式的 `get_default_account` 猜测逻辑
