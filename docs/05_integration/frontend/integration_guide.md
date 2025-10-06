# 前端对接指南

**版本**: v0.1.0
**更新日期**: 2025-10-03
**适用对象**: 前端开发者

---

## 📋 目录

1. [快速开始](#快速开始)
2. [环境配置](#环境配置)
3. [HTTP API 集成](#http-api-集成)
4. [WebSocket 集成](#websocket-集成)
5. [状态管理](#状态管理)
6. [错误处理](#错误处理)
7. [完整示例](#完整示例)
8. [常见问题](#常见问题)

---

## 快速开始

### 5 分钟快速集成

```javascript
// 1. 安装依赖
npm install axios

// 2. 创建 API 客户端
import axios from 'axios';

const api = axios.create({
  baseURL: 'http://localhost:8080/api',
  timeout: 5000,
  headers: {
    'Content-Type': 'application/json'
  }
});

// 3. 开户
const account = await api.post('/account/open', {
  user_id: 'user001',
  user_name: '张三',
  init_cash: 1000000,
  account_type: 'individual',
  password: 'password123'
});

// 4. 提交订单
const order = await api.post('/order/submit', {
  user_id: 'user001',
  instrument_id: 'IX2301',
  direction: 'BUY',
  offset: 'OPEN',
  volume: 10,
  price: 120.0,
  order_type: 'LIMIT'
});

// 5. 连接 WebSocket 接收实时推送
const ws = new WebSocket('ws://localhost:8081/ws?user_id=user001');
ws.onmessage = (event) => {
  const msg = JSON.parse(event.data);
  console.log('收到消息:', msg);
};
```

---

## 环境配置

### 服务器地址

| 服务 | 开发环境 | 生产环境 |
|------|---------|---------|
| HTTP API | http://localhost:8080 | https://api.yourdomain.com |
| WebSocket | ws://localhost:8081 | wss://ws.yourdomain.com |

### 跨域配置

开发环境已启用 CORS，允许所有来源访问。生产环境需要配置允许的域名白名单。

**Vite 开发服务器代理配置**:

```javascript
// vite.config.js
export default {
  server: {
    proxy: {
      '/api': {
        target: 'http://localhost:8080',
        changeOrigin: true
      },
      '/ws': {
        target: 'ws://localhost:8081',
        ws: true
      }
    }
  }
}
```

**Webpack 代理配置**:

```javascript
// webpack.config.js
module.exports = {
  devServer: {
    proxy: {
      '/api': {
        target: 'http://localhost:8080',
        changeOrigin: true
      },
      '/ws': {
        target: 'ws://localhost:8081',
        ws: true,
        changeOrigin: true
      }
    }
  }
}
```

---

## HTTP API 集成

### 创建 API 客户端

**TypeScript 版本**:

```typescript
// src/api/client.ts
import axios, { AxiosInstance, AxiosError } from 'axios';

export interface ApiResponse<T = any> {
  success: boolean;
  data?: T;
  error?: {
    code: number;
    message: string;
  };
}

class ApiClient {
  private client: AxiosInstance;

  constructor(baseURL: string) {
    this.client = axios.create({
      baseURL,
      timeout: 10000,
      headers: {
        'Content-Type': 'application/json'
      }
    });

    // 响应拦截器
    this.client.interceptors.response.use(
      (response) => response.data,
      (error: AxiosError<ApiResponse>) => {
        if (error.response?.data?.error) {
          throw new Error(error.response.data.error.message);
        }
        throw error;
      }
    );
  }

  // 账户管理
  async openAccount(params: {
    user_id: string;
    user_name: string;
    init_cash: number;
    account_type: string;
    password: string;
  }): Promise<ApiResponse> {
    return this.client.post('/account/open', params);
  }

  async getAccount(userId: string): Promise<ApiResponse> {
    return this.client.get(`/account/${userId}`);
  }

  async deposit(userId: string, amount: number): Promise<ApiResponse> {
    return this.client.post('/account/deposit', { user_id: userId, amount });
  }

  async withdraw(userId: string, amount: number): Promise<ApiResponse> {
    return this.client.post('/account/withdraw', { user_id: userId, amount });
  }

  // 订单管理
  async submitOrder(params: {
    user_id: string;
    instrument_id: string;
    direction: 'BUY' | 'SELL';
    offset: 'OPEN' | 'CLOSE' | 'CLOSETODAY' | 'CLOSEYESTERDAY';
    volume: number;
    price: number;
    order_type: 'LIMIT' | 'MARKET';
  }): Promise<ApiResponse> {
    return this.client.post('/order/submit', params);
  }

  async cancelOrder(orderId: string): Promise<ApiResponse> {
    return this.client.post('/order/cancel', { order_id: orderId });
  }

  async getOrder(orderId: string): Promise<ApiResponse> {
    return this.client.get(`/order/${orderId}`);
  }

  async getUserOrders(userId: string): Promise<ApiResponse> {
    return this.client.get(`/order/user/${userId}`);
  }

  // 持仓查询
  async getPosition(userId: string): Promise<ApiResponse> {
    return this.client.get(`/position/${userId}`);
  }

  // 健康检查
  async healthCheck(): Promise<ApiResponse> {
    return this.client.get('/health');
  }
}

export const apiClient = new ApiClient('http://localhost:8080/api');
```

### React Hooks 封装

```typescript
// src/hooks/useApi.ts
import { useState, useCallback } from 'react';
import { apiClient, ApiResponse } from '../api/client';

export function useApi<T = any>() {
  const [data, setData] = useState<T | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const execute = useCallback(async (
    apiCall: () => Promise<ApiResponse<T>>
  ) => {
    try {
      setLoading(true);
      setError(null);
      const response = await apiCall();

      if (response.success && response.data) {
        setData(response.data);
        return response.data;
      } else if (response.error) {
        setError(response.error.message);
        throw new Error(response.error.message);
      }
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Unknown error';
      setError(errorMessage);
      throw err;
    } finally {
      setLoading(false);
    }
  }, []);

  return { data, loading, error, execute };
}

// 使用示例
function AccountComponent({ userId }: { userId: string }) {
  const { data: account, loading, error, execute } = useApi();

  useEffect(() => {
    execute(() => apiClient.getAccount(userId));
  }, [userId, execute]);

  if (loading) return <div>加载中...</div>;
  if (error) return <div>错误: {error}</div>;
  if (!account) return null;

  return (
    <div>
      <h3>账户信息</h3>
      <p>余额: {account.balance}</p>
      <p>可用: {account.available}</p>
      <p>保证金: {account.margin}</p>
    </div>
  );
}
```

---

## WebSocket 集成

### React WebSocket Hook

```typescript
// src/hooks/useWebSocket.ts
import { useEffect, useRef, useState, useCallback } from 'react';

interface WebSocketMessage {
  type: string;
  [key: string]: any;
}

interface UseWebSocketOptions {
  url: string;
  userId: string;
  onMessage?: (msg: WebSocketMessage) => void;
  onError?: (error: Event) => void;
  reconnectInterval?: number;
  maxReconnectAttempts?: number;
}

export function useWebSocket({
  url,
  userId,
  onMessage,
  onError,
  reconnectInterval = 3000,
  maxReconnectAttempts = 5
}: UseWebSocketOptions) {
  const wsRef = useRef<WebSocket | null>(null);
  const reconnectAttemptsRef = useRef(0);
  const [isConnected, setIsConnected] = useState(false);
  const [lastMessage, setLastMessage] = useState<WebSocketMessage | null>(null);

  const connect = useCallback(() => {
    try {
      const ws = new WebSocket(`${url}?user_id=${userId}`);

      ws.onopen = () => {
        console.log('WebSocket 连接成功');
        setIsConnected(true);
        reconnectAttemptsRef.current = 0;

        // 发送认证消息
        ws.send(JSON.stringify({
          type: 'auth',
          user_id: userId,
          token: 'your_token_here'
        }));
      };

      ws.onmessage = (event) => {
        try {
          const msg = JSON.parse(event.data) as WebSocketMessage;
          setLastMessage(msg);
          onMessage?.(msg);
        } catch (err) {
          console.error('解析消息失败:', err);
        }
      };

      ws.onerror = (error) => {
        console.error('WebSocket 错误:', error);
        onError?.(error);
      };

      ws.onclose = () => {
        console.log('WebSocket 连接关闭');
        setIsConnected(false);

        // 自动重连
        if (reconnectAttemptsRef.current < maxReconnectAttempts) {
          reconnectAttemptsRef.current++;
          console.log(`尝试重连 (${reconnectAttemptsRef.current}/${maxReconnectAttempts})...`);
          setTimeout(connect, reconnectInterval);
        }
      };

      wsRef.current = ws;
    } catch (err) {
      console.error('WebSocket 连接失败:', err);
    }
  }, [url, userId, onMessage, onError, reconnectInterval, maxReconnectAttempts]);

  useEffect(() => {
    connect();

    return () => {
      if (wsRef.current) {
        wsRef.current.close();
      }
    };
  }, [connect]);

  const send = useCallback((message: WebSocketMessage) => {
    if (wsRef.current?.readyState === WebSocket.OPEN) {
      wsRef.current.send(JSON.stringify(message));
    } else {
      console.warn('WebSocket 未连接');
    }
  }, []);

  const subscribe = useCallback((channels: string[], instruments: string[]) => {
    send({
      type: 'subscribe',
      channels,
      instruments
    });
  }, [send]);

  const submitOrder = useCallback((order: {
    instrument_id: string;
    direction: string;
    offset: string;
    volume: number;
    price: number;
    order_type: string;
  }) => {
    send({
      type: 'submit_order',
      ...order
    });
  }, [send]);

  return {
    isConnected,
    lastMessage,
    send,
    subscribe,
    submitOrder
  };
}
```

### Vue 3 Composition API

```typescript
// src/composables/useWebSocket.ts
import { ref, onMounted, onUnmounted } from 'vue';

export function useWebSocket(url: string, userId: string) {
  const ws = ref<WebSocket | null>(null);
  const isConnected = ref(false);
  const messages = ref<any[]>([]);

  const connect = () => {
    ws.value = new WebSocket(`${url}?user_id=${userId}`);

    ws.value.onopen = () => {
      isConnected.value = true;

      // 认证
      ws.value?.send(JSON.stringify({
        type: 'auth',
        user_id: userId,
        token: 'your_token'
      }));
    };

    ws.value.onmessage = (event) => {
      const msg = JSON.parse(event.data);
      messages.value.push(msg);
    };

    ws.value.onclose = () => {
      isConnected.value = false;
      // 3秒后重连
      setTimeout(connect, 3000);
    };
  };

  const send = (message: any) => {
    if (ws.value?.readyState === WebSocket.OPEN) {
      ws.value.send(JSON.stringify(message));
    }
  };

  onMounted(connect);
  onUnmounted(() => ws.value?.close());

  return { isConnected, messages, send };
}
```

---

## 状态管理

### Redux Toolkit 集成

```typescript
// src/store/accountSlice.ts
import { createSlice, createAsyncThunk, PayloadAction } from '@reduxjs/toolkit';
import { apiClient } from '../api/client';

interface AccountState {
  data: any | null;
  loading: boolean;
  error: string | null;
}

const initialState: AccountState = {
  data: null,
  loading: false,
  error: null
};

export const fetchAccount = createAsyncThunk(
  'account/fetch',
  async (userId: string) => {
    const response = await apiClient.getAccount(userId);
    return response.data;
  }
);

export const depositFunds = createAsyncThunk(
  'account/deposit',
  async ({ userId, amount }: { userId: string; amount: number }) => {
    const response = await apiClient.deposit(userId, amount);
    return response.data;
  }
);

const accountSlice = createSlice({
  name: 'account',
  initialState,
  reducers: {
    updateAccount: (state, action: PayloadAction<any>) => {
      state.data = action.payload;
    }
  },
  extraReducers: (builder) => {
    builder
      .addCase(fetchAccount.pending, (state) => {
        state.loading = true;
        state.error = null;
      })
      .addCase(fetchAccount.fulfilled, (state, action) => {
        state.loading = false;
        state.data = action.payload;
      })
      .addCase(fetchAccount.rejected, (state, action) => {
        state.loading = false;
        state.error = action.error.message || 'Failed to fetch account';
      });
  }
});

export const { updateAccount } = accountSlice.actions;
export default accountSlice.reducer;
```

### Zustand 状态管理

```typescript
// src/store/useStore.ts
import create from 'zustand';
import { apiClient } from '../api/client';

interface Account {
  user_id: string;
  balance: number;
  available: number;
  margin: number;
  // ...
}

interface Order {
  order_id: string;
  status: string;
  // ...
}

interface Store {
  // 账户
  account: Account | null;
  fetchAccount: (userId: string) => Promise<void>;

  // 订单
  orders: Order[];
  submitOrder: (params: any) => Promise<void>;

  // WebSocket
  isWsConnected: boolean;
  setWsConnected: (connected: boolean) => void;
}

export const useStore = create<Store>((set) => ({
  account: null,
  fetchAccount: async (userId) => {
    const response = await apiClient.getAccount(userId);
    if (response.success) {
      set({ account: response.data });
    }
  },

  orders: [],
  submitOrder: async (params) => {
    const response = await apiClient.submitOrder(params);
    if (response.success) {
      set((state) => ({
        orders: [...state.orders, response.data]
      }));
    }
  },

  isWsConnected: false,
  setWsConnected: (connected) => set({ isWsConnected: connected })
}));
```

---

## 错误处理

### 统一错误处理

```typescript
// src/utils/errorHandler.ts
import { message } from 'antd'; // 或其他 UI 库

export class ApiError extends Error {
  code: number;

  constructor(code: number, message: string) {
    super(message);
    this.code = code;
    this.name = 'ApiError';
  }
}

export function handleApiError(error: any) {
  if (error.response?.data?.error) {
    const { code, message: msg } = error.response.data.error;

    // 根据错误码显示不同提示
    switch (code) {
      case 1001:
        message.error('账户不存在');
        break;
      case 2001:
        message.error('订单不存在');
        break;
      case 3001:
        message.error('资金不足');
        break;
      case 3002:
        message.error('持仓不足');
        break;
      case 3003:
        message.error('超过持仓限制');
        break;
      case 3004:
        message.error('风险度过高');
        break;
      default:
        message.error(msg || '操作失败');
    }

    throw new ApiError(code, msg);
  } else {
    message.error('网络错误，请稍后重试');
    throw error;
  }
}
```

### 错误边界组件

```typescript
// src/components/ErrorBoundary.tsx
import React, { Component, ReactNode } from 'react';

interface Props {
  children: ReactNode;
  fallback?: ReactNode;
}

interface State {
  hasError: boolean;
  error: Error | null;
}

export class ErrorBoundary extends Component<Props, State> {
  constructor(props: Props) {
    super(props);
    this.state = { hasError: false, error: null };
  }

  static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, errorInfo: React.ErrorInfo) {
    console.error('错误边界捕获:', error, errorInfo);
  }

  render() {
    if (this.state.hasError) {
      return this.props.fallback || (
        <div>
          <h2>出错了</h2>
          <p>{this.state.error?.message}</p>
        </div>
      );
    }

    return this.props.children;
  }
}
```

---

## 完整示例

### React 完整交易组件

```typescript
// src/components/TradingPanel.tsx
import React, { useState, useEffect } from 'react';
import { useWebSocket } from '../hooks/useWebSocket';
import { apiClient } from '../api/client';

export function TradingPanel({ userId }: { userId: string }) {
  const [account, setAccount] = useState<any>(null);
  const [orders, setOrders] = useState<any[]>([]);

  // WebSocket 连接
  const { isConnected, lastMessage, submitOrder, subscribe } = useWebSocket({
    url: 'ws://localhost:8081/ws',
    userId,
    onMessage: (msg) => {
      switch (msg.type) {
        case 'trade':
          console.log('成交:', msg);
          // 刷新账户
          loadAccount();
          break;

        case 'account_update':
          setAccount(msg);
          break;

        case 'order_status':
          setOrders(prev => {
            const index = prev.findIndex(o => o.order_id === msg.order_id);
            if (index >= 0) {
              const newOrders = [...prev];
              newOrders[index] = { ...newOrders[index], ...msg };
              return newOrders;
            }
            return prev;
          });
          break;
      }
    }
  });

  // 加载账户
  const loadAccount = async () => {
    const response = await apiClient.getAccount(userId);
    if (response.success) {
      setAccount(response.data);
    }
  };

  // 加载订单
  const loadOrders = async () => {
    const response = await apiClient.getUserOrders(userId);
    if (response.success) {
      setOrders(response.data);
    }
  };

  useEffect(() => {
    loadAccount();
    loadOrders();

    // 订阅行情
    if (isConnected) {
      subscribe(['trade', 'account_update'], ['IX2301', 'IF2301']);
    }
  }, [isConnected]);

  // 提交订单
  const handleSubmitOrder = async (orderParams: any) => {
    try {
      const response = await apiClient.submitOrder({
        user_id: userId,
        ...orderParams
      });

      if (response.success) {
        console.log('订单提交成功:', response.data);
        loadOrders();
      }
    } catch (error) {
      console.error('订单提交失败:', error);
    }
  };

  return (
    <div className="trading-panel">
      {/* 连接状态 */}
      <div className="status">
        WebSocket: {isConnected ? '已连接' : '未连接'}
      </div>

      {/* 账户信息 */}
      {account && (
        <div className="account-info">
          <h3>账户信息</h3>
          <p>余额: {account.balance.toFixed(2)}</p>
          <p>可用: {account.available.toFixed(2)}</p>
          <p>保证金: {account.margin.toFixed(2)}</p>
          <p>风险度: {(account.risk_ratio * 100).toFixed(2)}%</p>
        </div>
      )}

      {/* 下单表单 */}
      <OrderForm onSubmit={handleSubmitOrder} />

      {/* 订单列表 */}
      <OrderList orders={orders} />
    </div>
  );
}
```

---

## 常见问题

### Q1: WebSocket 连接失败怎么办？

**A**: 检查以下几点:
1. WebSocket 服务器是否启动 (端口 8081)
2. URL 格式是否正确 (`ws://localhost:8081/ws?user_id=xxx`)
3. 浏览器控制台是否有 CORS 错误
4. 防火墙是否阻止连接

### Q2: 如何处理 WebSocket 断线重连？

**A**: 使用 `useWebSocket` hook 已内置自动重连机制，默认最多重连 5 次，间隔 3 秒。可通过参数配置:

```typescript
const { isConnected } = useWebSocket({
  url: 'ws://localhost:8081/ws',
  userId: 'user001',
  reconnectInterval: 5000,  // 5秒
  maxReconnectAttempts: 10  // 最多10次
});
```

### Q3: API 请求超时怎么办？

**A**: 调整 axios 超时时间或实现重试机制:

```typescript
const client = axios.create({
  timeout: 30000  // 30秒
});

// 或实现重试
async function retryRequest(fn: () => Promise<any>, retries = 3) {
  for (let i = 0; i < retries; i++) {
    try {
      return await fn();
    } catch (error) {
      if (i === retries - 1) throw error;
      await new Promise(resolve => setTimeout(resolve, 1000 * (i + 1)));
    }
  }
}
```

### Q4: 如何测试 API 集成？

**A**: 使用 Mock Service Worker (MSW):

```typescript
// src/mocks/handlers.ts
import { rest } from 'msw';

export const handlers = [
  rest.post('/api/order/submit', (req, res, ctx) => {
    return res(
      ctx.json({
        success: true,
        data: {
          order_id: 'O12345',
          status: 'submitted'
        }
      })
    );
  })
];
```

### Q5: 生产环境部署注意事项？

**A**:
1. 使用 HTTPS/WSS 加密连接
2. 配置 CORS 白名单，不要使用 `allow_any_origin()`
3. 实现 Token 认证机制
4. 添加请求限流
5. 启用日志监控
6. 实现心跳保活机制

---

## 下一步

- 阅读 [API_REFERENCE.md](API_REFERENCE.md) 了解详细 API 文档
- 阅读 [WEBSOCKET_PROTOCOL.md](WEBSOCKET_PROTOCOL.md) 了解 WebSocket 协议
- 阅读 [ERROR_CODES.md](ERROR_CODES.md) 了解所有错误码
- 参考示例项目: `examples/frontend-demo/`

---

**文档更新**: 2025-10-03
**维护者**: @yutiansut
