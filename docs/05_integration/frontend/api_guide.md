# QAExchange 前端 API 集成指南

## 📋 User-Account 架构说明

### 核心概念

```
User (用户) 1 ──────→ N Account (账户)
  │                      │
  ├─ user_id (UUID)      ├─ account_id (ACC_xxx)
  ├─ username            ├─ account_name
  ├─ email               ├─ balance
  └─ password            └─ portfolio_cookie = user_id
```

**关键理解**:
- 1 个 **User** 可以有 **多个 Account**
- User 用于登录认证
- Account 用于交易操作
- 通过 `portfolio_cookie` 字段关联 User ↔ Account

---

## 🔐 1. 用户认证流程

### 1.1 注册

**接口**: `POST /api/auth/register`

**请求**:
```json
{
  "username": "zhangsan",
  "email": "zhangsan@example.com",
  "password": "password123",
  "phone": "13800138000"
}
```

**响应**:
```json
{
  "success": true,
  "data": {
    "user_id": "8d482456-9fab-4d1b-9c2c-bf80cb3ff509",
    "username": "zhangsan",
    "email": "zhangsan@example.com",
    "message": "Registration successful"
  },
  "error": null
}
```

**前端处理**:
```javascript
async function register(username, email, password, phone) {
  const response = await fetch('http://192.168.2.115:8097/api/auth/register', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ username, email, password, phone })
  });

  const result = await response.json();
  if (result.success) {
    // 保存 user_id 到 localStorage
    localStorage.setItem('user_id', result.data.user_id);
    localStorage.setItem('username', result.data.username);
    return result.data;
  } else {
    throw new Error(result.error);
  }
}
```

---

### 1.2 登录

**接口**: `POST /api/auth/login`

**请求**:
```json
{
  "username": "zhangsan",
  "password": "password123"
}
```

**响应**:
```json
{
  "success": true,
  "data": {
    "user_id": "8d482456-9fab-4d1b-9c2c-bf80cb3ff509",
    "username": "zhangsan",
    "email": "zhangsan@example.com",
    "phone": "13800138000",
    "token": "mock_token_xxx",
    "message": "Login successful"
  },
  "error": null
}
```

**前端处理**:
```javascript
async function login(username, password) {
  const response = await fetch('http://192.168.2.115:8097/api/auth/login', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ username, password })
  });

  const result = await response.json();
  if (result.success) {
    // 保存登录态
    localStorage.setItem('user_id', result.data.user_id);
    localStorage.setItem('username', result.data.username);
    localStorage.setItem('token', result.data.token);
    return result.data;
  } else {
    throw new Error(result.error);
  }
}
```

---

### 1.3 获取当前用户信息

**接口**: `GET /api/auth/user/{user_id}`

**示例**:
```bash
curl http://192.168.2.115:8097/api/auth/user/8d482456-9fab-4d1b-9c2c-bf80cb3ff509
```

**响应**:
```json
{
  "success": true,
  "data": {
    "user_id": "8d482456-9fab-4d1b-9c2c-bf80cb3ff509",
    "username": "zhangsan",
    "email": "zhangsan@example.com",
    "phone": "13800138000",
    "is_admin": false,
    "created_at": "2025-10-05 12:00:00"
  },
  "error": null
}
```

---

## 💰 2. 账户管理 (核心功能)

### 2.1 查询用户的所有账户 ⭐ **重点**

**接口**: `GET /api/user/{user_id}/accounts`

**前端页面**: `http://192.168.2.115:8097/#/accounts`

**正确调用方式**:
```javascript
// ✅ 正确: 使用 user_id 查询该用户的所有账户
async function getUserAccounts() {
  const user_id = localStorage.getItem('user_id');  // 从登录态获取

  const response = await fetch(
    `http://192.168.2.115:8097/api/user/${user_id}/accounts`
  );

  const result = await response.json();
  if (result.success) {
    return result.data.accounts;  // 返回账户列表
  }
}
```

**错误调用方式**:
```javascript
// ❌ 错误: 直接用 user_id 查单个账户（这不存在）
fetch(`http://192.168.2.115:8097/api/account/${user_id}`)  // 404 错误！
```

**响应示例**:
```json
{
  "success": true,
  "data": {
    "accounts": [
      {
        "account_id": "ACC_9bc0b5268d4741cb8e03d766565f3fc8",
        "account_name": "tea1",
        "account_type": "Individual",
        "balance": 10000000.0,
        "available": 10000000.0,
        "margin": 0.0,
        "risk_ratio": 0.0,
        "created_at": 1759680011
      },
      {
        "account_id": "ACC_a1b2c3d4e5f6...",
        "account_name": "tea2",
        "account_type": "Corporate",
        "balance": 5000000.0,
        "available": 4800000.0,
        "margin": 200000.0,
        "risk_ratio": 0.04,
        "created_at": 1759680999
      }
    ],
    "total": 2
  },
  "error": null
}
```

**前端展示**:
```vue
<!-- Vue 组件示例 -->
<template>
  <div class="accounts-page">
    <h2>我的账户</h2>
    <div v-for="account in accounts" :key="account.account_id" class="account-card">
      <h3>{{ account.account_name }}</h3>
      <p>账户ID: {{ account.account_id }}</p>
      <p>类型: {{ account.account_type }}</p>
      <p>总权益: ¥{{ account.balance.toLocaleString() }}</p>
      <p>可用资金: ¥{{ account.available.toLocaleString() }}</p>
      <p>保证金: ¥{{ account.margin.toLocaleString() }}</p>
      <p>风险度: {{ (account.risk_ratio * 100).toFixed(2) }}%</p>
      <button @click="selectAccount(account.account_id)">选择此账户</button>
    </div>
  </div>
</template>

<script>
export default {
  data() {
    return {
      accounts: [],
      currentUserId: ''
    }
  },

  async mounted() {
    this.currentUserId = localStorage.getItem('user_id');
    await this.loadAccounts();
  },

  methods: {
    async loadAccounts() {
      try {
        const response = await fetch(
          `http://192.168.2.115:8097/api/user/${this.currentUserId}/accounts`
        );
        const result = await response.json();

        if (result.success) {
          this.accounts = result.data.accounts;
        } else {
          console.error('加载账户失败:', result.error);
        }
      } catch (error) {
        console.error('请求失败:', error);
      }
    },

    selectAccount(accountId) {
      // 保存当前选中的账户ID，用于后续交易
      localStorage.setItem('current_account_id', accountId);
      this.$router.push('/trading');
    }
  }
}
</script>
```

---

### 2.2 查询单个账户详情

**接口**: `GET /api/account/{account_id}`

**使用场景**: 点击某个账户后，查看该账户的详细信息

**示例**:
```javascript
async function getAccountDetail(accountId) {
  const response = await fetch(
    `http://192.168.2.115:8097/api/account/${accountId}`
  );

  const result = await response.json();
  if (result.success) {
    return result.data;
  }
}

// 使用示例
const detail = await getAccountDetail('ACC_9bc0b5268d4741cb8e03d766565f3fc8');
console.log(detail);
```

**响应**:
```json
{
  "success": true,
  "data": {
    "user_id": "ACC_9bc0b5268d4741cb8e03d766565f3fc8",
    "user_name": "tea1",
    "balance": 10000000.0,
    "available": 10000000.0,
    "frozen": 0.0,
    "margin": 0.0,
    "profit": 0.0,
    "risk_ratio": 0.0,
    "account_type": "individual",
    "created_at": 1759680011
  },
  "error": null
}
```

---

### 2.3 创建新账户

**接口**: `POST /api/user/{user_id}/account/create`

**请求**:
```json
{
  "account_id": "ACC_custom_123",  // 可选，不填则自动生成
  "account_name": "My Trading Account",
  "account_type": "Individual",
  "init_cash": 1000000.0
}
```

**前端示例**:
```javascript
async function createAccount(accountName, accountType, initCash) {
  const user_id = localStorage.getItem('user_id');

  const response = await fetch(
    `http://192.168.2.115:8097/api/user/${user_id}/account/create`,
    {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        account_name: accountName,
        account_type: accountType,
        init_cash: initCash  // ✨ 统一使用 init_cash @yutiansut @quantaxis
      })
    }
  );

  const result = await response.json();
  if (result.success) {
    console.log('账户创建成功:', result.data.account_id);
    return result.data;
  }
}
```

**响应**:
```json
{
  "success": true,
  "data": {
    "account_id": "ACC_9bc0b5268d4741cb8e03d766565f3fc8",
    "message": "Account created successfully"
  },
  "error": null
}
```

---

## 📊 3. 交易流程

### 3.1 下单

**接口**: `POST /api/order/submit`

**前提**: 用户已选择当前交易账户

**请求**:
```json
{
  "user_id": "ACC_9bc0b5268d4741cb8e03d766565f3fc8",  // 注意: 这里是 account_id
  "instrument_id": "SHFE.cu2501",
  "direction": "Buy",
  "offset": "Open",
  "volume": 1,
  "price": 75000.0,
  "order_type": "Limit"
}
```

**前端示例**:
```javascript
async function submitOrder(instrumentId, direction, volume, price) {
  // 使用当前选中的 account_id
  const account_id = localStorage.getItem('current_account_id');

  const response = await fetch(
    'http://192.168.2.115:8097/api/order/submit',
    {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        user_id: account_id,  // 后端参数名叫 user_id，但实际是 account_id
        instrument_id: instrumentId,
        direction: direction,
        offset: 'Open',
        volume: volume,
        price: price,
        order_type: 'Limit'
      })
    }
  );

  const result = await response.json();
  return result;
}
```

---

### 3.2 查询订单

**按用户查询**: `GET /api/order/user/{account_id}`

```javascript
async function getUserOrders() {
  const account_id = localStorage.getItem('current_account_id');

  const response = await fetch(
    `http://192.168.2.115:8097/api/order/user/${account_id}`
  );

  const result = await response.json();
  return result.data;
}
```

---

### 3.3 查询持仓

**接口**: `GET /api/position/{account_id}`

```javascript
async function getPositions() {
  const account_id = localStorage.getItem('current_account_id');

  const response = await fetch(
    `http://192.168.2.115:8097/api/position/${account_id}`
  );

  const result = await response.json();
  return result.data.positions;
}
```

---

### 3.4 查询成交记录

**接口**: `GET /api/trades/user/{account_id}`

```javascript
async function getTrades() {
  const account_id = localStorage.getItem('current_account_id');

  const response = await fetch(
    `http://192.168.2.115:8097/api/trades/user/${account_id}`
  );

  const result = await response.json();
  return result.data.trades;
}
```

---

## 📈 4. 市场数据

### 4.1 获取合约列表

**接口**: `GET /api/market/instruments`

```javascript
async function getInstruments() {
  const response = await fetch(
    'http://192.168.2.115:8097/api/market/instruments'
  );

  const result = await response.json();
  return result.data.instruments;
}
```

---

### 4.2 获取行情快照

**接口**: `GET /api/market/tick/{instrument_id}`

```javascript
async function getTick(instrumentId) {
  const response = await fetch(
    `http://192.168.2.115:8097/api/market/tick/${instrumentId}`
  );

  const result = await response.json();
  return result.data;
}

// 使用示例
const tick = await getTick('SHFE.cu2501');
console.log('最新价:', tick.last_price);
```

---

### 4.3 获取盘口数据

**接口**: `GET /api/market/orderbook/{instrument_id}`

```javascript
async function getOrderbook(instrumentId) {
  const response = await fetch(
    `http://192.168.2.115:8097/api/market/orderbook/${instrumentId}`
  );

  const result = await response.json();
  return result.data;
}

// 使用示例
const orderbook = await getOrderbook('SHFE.cu2501');
console.log('买一价:', orderbook.bids[0].price);
console.log('卖一价:', orderbook.asks[0].price);
```

---

## 🔌 5. WebSocket 实时推送 (DIFF 协议)

### 5.1 连接 WebSocket

```javascript
const ws = new WebSocket('ws://192.168.2.115:8097/ws');

ws.onopen = () => {
  console.log('WebSocket 连接成功');

  // 发送 peek_message 请求数据更新
  ws.send(JSON.stringify({ aid: 'peek_message' }));
};

ws.onmessage = (event) => {
  const message = JSON.parse(event.data);

  if (message.aid === 'rtn_data') {
    // 处理数据更新 (DIFF 协议)
    handleDataUpdate(message.data);
  }
};
```

---

### 5.2 订阅行情

```javascript
function subscribeQuotes(instruments) {
  const message = {
    aid: 'subscribe_quote',
    ins_list: instruments.join(',')  // 'SHFE.cu2501,CFFEX.IF2501'
  };

  ws.send(JSON.stringify(message));
}

// 使用示例
subscribeQuotes(['SHFE.cu2501', 'CFFEX.IF2501']);
```

---

### 5.3 处理 DIFF 数据更新

```javascript
let businessSnapshot = {
  accounts: {},
  orders: {},
  positions: {},
  quotes: {}
};

function handleDataUpdate(patches) {
  // 应用所有 JSON Merge Patch
  patches.forEach(patch => {
    applyMergePatch(businessSnapshot, patch);
  });

  // 更新 UI
  updateUI(businessSnapshot);

  // 发送下一个 peek_message
  ws.send(JSON.stringify({ aid: 'peek_message' }));
}

// JSON Merge Patch 算法 (RFC 7386)
function applyMergePatch(target, patch) {
  for (const key in patch) {
    if (patch[key] === null) {
      delete target[key];
    } else if (typeof patch[key] === 'object' && !Array.isArray(patch[key])) {
      if (!target[key]) target[key] = {};
      applyMergePatch(target[key], patch[key]);
    } else {
      target[key] = patch[key];
    }
  }
}
```

---

## 📝 6. 完整示例：账户管理页面

```vue
<template>
  <div class="accounts-management">
    <!-- 用户信息 -->
    <div class="user-info">
      <h2>欢迎, {{ username }}</h2>
      <p>用户ID: {{ userId }}</p>
    </div>

    <!-- 账户列表 -->
    <div class="accounts-section">
      <h3>我的账户 ({{ accounts.length }})</h3>

      <button @click="showCreateDialog = true">+ 创建新账户</button>

      <div class="accounts-grid">
        <div
          v-for="account in accounts"
          :key="account.account_id"
          class="account-card"
          :class="{ active: account.account_id === currentAccountId }"
          @click="selectAccount(account.account_id)"
        >
          <h4>{{ account.account_name }}</h4>
          <div class="account-id">{{ account.account_id }}</div>
          <div class="account-stats">
            <div class="stat">
              <span>总权益</span>
              <strong>¥{{ account.balance.toLocaleString() }}</strong>
            </div>
            <div class="stat">
              <span>可用资金</span>
              <strong>¥{{ account.available.toLocaleString() }}</strong>
            </div>
            <div class="stat">
              <span>保证金</span>
              <strong>¥{{ account.margin.toLocaleString() }}</strong>
            </div>
            <div class="stat">
              <span>风险度</span>
              <strong :class="getRiskClass(account.risk_ratio)">
                {{ (account.risk_ratio * 100).toFixed(2) }}%
              </strong>
            </div>
          </div>
          <div class="account-actions">
            <button @click.stop="viewDetail(account.account_id)">详情</button>
            <button @click.stop="deposit(account.account_id)">入金</button>
          </div>
        </div>
      </div>
    </div>

    <!-- 创建账户对话框 -->
    <div v-if="showCreateDialog" class="dialog">
      <h3>创建新账户</h3>
      <form @submit.prevent="createAccount">
        <input v-model="newAccount.name" placeholder="账户名称" required />
        <select v-model="newAccount.type" required>
          <option value="Individual">个人账户</option>
          <option value="Corporate">企业账户</option>
        </select>
        <input
          v-model.number="newAccount.balance"
          type="number"
          placeholder="初始资金"
          required
        />
        <button type="submit">创建</button>
        <button type="button" @click="showCreateDialog = false">取消</button>
      </form>
    </div>
  </div>
</template>

<script>
export default {
  data() {
    return {
      userId: '',
      username: '',
      accounts: [],
      currentAccountId: '',
      showCreateDialog: false,
      newAccount: {
        name: '',
        type: 'Individual',
        balance: 1000000
      }
    }
  },

  async mounted() {
    // 从 localStorage 获取登录态
    this.userId = localStorage.getItem('user_id');
    this.username = localStorage.getItem('username');
    this.currentAccountId = localStorage.getItem('current_account_id') || '';

    if (!this.userId) {
      this.$router.push('/login');
      return;
    }

    await this.loadAccounts();
  },

  methods: {
    async loadAccounts() {
      try {
        const response = await fetch(
          `http://192.168.2.115:8097/api/user/${this.userId}/accounts`
        );
        const result = await response.json();

        if (result.success) {
          this.accounts = result.data.accounts;
        } else {
          this.$message.error('加载账户失败: ' + result.error);
        }
      } catch (error) {
        console.error('请求失败:', error);
        this.$message.error('网络错误');
      }
    },

    async createAccount() {
      try {
        const response = await fetch(
          `http://192.168.2.115:8097/api/user/${this.userId}/account/create`,
          {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
              account_name: this.newAccount.name,
              account_type: this.newAccount.type,
              init_cash: this.newAccount.init_cash  // ✨ 统一使用 init_cash @yutiansut @quantaxis
            })
          }
        );

        const result = await response.json();

        if (result.success) {
          this.$message.success('账户创建成功');
          this.showCreateDialog = false;
          await this.loadAccounts();
        } else {
          this.$message.error('创建失败: ' + result.error);
        }
      } catch (error) {
        console.error('创建失败:', error);
        this.$message.error('网络错误');
      }
    },

    selectAccount(accountId) {
      this.currentAccountId = accountId;
      localStorage.setItem('current_account_id', accountId);
      this.$message.success('已切换到账户: ' + accountId);
    },

    async viewDetail(accountId) {
      this.$router.push(`/account/${accountId}`);
    },

    async deposit(accountId) {
      // 跳转到入金页面
      this.$router.push(`/deposit?account=${accountId}`);
    },

    getRiskClass(ratio) {
      if (ratio > 0.8) return 'danger';
      if (ratio > 0.5) return 'warning';
      return 'safe';
    }
  }
}
</script>

<style scoped>
.accounts-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
  gap: 20px;
  margin-top: 20px;
}

.account-card {
  border: 2px solid #e0e0e0;
  border-radius: 8px;
  padding: 20px;
  cursor: pointer;
  transition: all 0.3s;
}

.account-card:hover {
  border-color: #1890ff;
  box-shadow: 0 4px 12px rgba(0,0,0,0.1);
}

.account-card.active {
  border-color: #52c41a;
  background-color: #f6ffed;
}

.account-id {
  font-size: 12px;
  color: #999;
  margin: 5px 0;
}

.account-stats {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 10px;
  margin: 15px 0;
}

.stat {
  display: flex;
  flex-direction: column;
}

.stat span {
  font-size: 12px;
  color: #666;
}

.stat strong {
  font-size: 16px;
  margin-top: 5px;
}

.danger { color: #f5222d; }
.warning { color: #faad14; }
.safe { color: #52c41a; }
</style>
```

---

## 🎯 7. API 路由总结

### 用户认证
- ✅ `POST /api/auth/register` - 注册
- ✅ `POST /api/auth/login` - 登录
- ✅ `GET /api/auth/user/{user_id}` - 获取用户信息

### 账户管理
- ⭐ `GET /api/user/{user_id}/accounts` - **查询用户的所有账户**
- ⭐ `POST /api/user/{user_id}/account/create` - **创建新账户**
- ✅ `GET /api/account/{account_id}` - 查询单个账户详情
- ✅ `POST /api/account/deposit` - 入金
- ✅ `POST /api/account/withdraw` - 出金

### 交易相关
- ✅ `POST /api/order/submit` - 下单
- ✅ `POST /api/order/cancel` - 撤单
- ✅ `GET /api/order/{order_id}` - 查询订单
- ✅ `GET /api/order/user/{account_id}` - 查询用户订单
- ✅ `GET /api/position/{account_id}` - 查询持仓
- ✅ `GET /api/trades/user/{account_id}` - 查询成交

### 市场数据
- ✅ `GET /api/market/instruments` - 合约列表
- ✅ `GET /api/market/tick/{instrument_id}` - 行情快照
- ✅ `GET /api/market/orderbook/{instrument_id}` - 盘口数据

---

## ⚠️ 常见错误

### 错误 1: 用 user_id 查账户详情
```javascript
// ❌ 错误
fetch(`/api/account/${user_id}`)  // 404: Account not found

// ✅ 正确
fetch(`/api/user/${user_id}/accounts`)  // 返回账户列表
```

### 错误 2: 下单时传错 ID
```javascript
// ❌ 错误: 传了 user_id
{
  "user_id": "8d482456-9fab-4d1b-9c2c-bf80cb3ff509"  // UUID
}

// ✅ 正确: 传 account_id
{
  "user_id": "ACC_9bc0b5268d4741cb8e03d766565f3fc8"  // account_id
}
```

### 错误 3: 混淆 User 和 Account
```javascript
// User (用户): 8d482456-9fab-4d1b-9c2c-bf80cb3ff509
// Account (账户): ACC_9bc0b5268d4741cb8e03d766565f3fc8

// 登录时保存 user_id
localStorage.setItem('user_id', user_id);

// 查询账户列表
GET /api/user/{user_id}/accounts

// 选择账户后保存 account_id
localStorage.setItem('current_account_id', account_id);

// 交易时使用 account_id
POST /api/order/submit { user_id: account_id }
```

---

## 📞 技术支持

如有问题，请查看：
- [DIFF 协议文档](./DIFF_BUSINESS_INTEGRATION.md)
- [后端 API 代码](../src/service/http/)
- [WebSocket 集成](../src/service/websocket/)

---

*最后更新: 2025-10-06*
*API 版本: 1.0*
