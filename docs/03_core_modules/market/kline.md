# K线聚合系统

> **模块作者**: @yutiansut @quantaxis
> **最后更新**: 2025-10-07

## 概述

K线（Candlestick）聚合系统是 QAExchange 市场数据模块的核心组件，负责从 tick 级别的成交数据实时聚合生成多周期 K 线数据。系统采用 **独立 Actor 架构**，通过订阅市场数据广播器实现高性能、低延迟的 K 线生成，并提供完整的持久化和恢复能力。

## 核心特性

- ✅ **分级采样**: 单个 tick 事件同时生成 7 个周期的 K 线（3s/1min/5min/15min/30min/60min/Day）
- ✅ **Actor 隔离**: 独立 Actix Actor，不阻塞交易流程
- ✅ **WAL 持久化**: 每个完成的 K 线自动写入 WAL，支持崩溃恢复
- ✅ **OLAP 存储**: K 线数据存储到 Arrow2 列式存储，支持高性能分析查询
- ✅ **双协议支持**: HTTP REST API + WebSocket DIFF 协议
- ✅ **实时推送**: 完成的 K 线立即广播到所有订阅者
- ✅ **历史查询**: 支持查询历史 K 线和当前未完成的 K 线

## 系统架构

### 架构图

```
┌────────────────────────────────────────────────────────────┐
│                    MatchingEngine                          │
│                    (撮合引擎)                              │
└────────────────────────────────────────────────────────────┘
                            │
                            ▼ publish tick
┌────────────────────────────────────────────────────────────┐
│              MarketDataBroadcaster                         │
│              (市场数据广播器)                              │
│                                                            │
│  - tick 事件: { instrument_id, price, volume, timestamp } │
└────────────────────────────────────────────────────────────┘
                            │
                            │ subscribe("tick")
                            ▼
┌────────────────────────────────────────────────────────────┐
│                   KLineActor                               │
│                   (K线聚合Actor)                           │
│                                                            │
│  ┌──────────────────────────────────────────────────┐    │
│  │  on_tick(price, volume, timestamp)               │    │
│  │                                                   │    │
│  │  for each period (3s/1min/5min/.../Day):        │    │
│  │    1. align_timestamp(timestamp, period)         │    │
│  │    2. if new period:                             │    │
│  │         - finish old kline                       │    │
│  │         - broadcast KLineFinished event          │    │
│  │         - persist to WAL                         │    │
│  │         - add to history (max 1000)              │    │
│  │         - create new kline                       │    │
│  │    3. update current kline (OHLCV + OI)          │    │
│  └──────────────────────────────────────────────────┘    │
│                                                            │
│  ┌──────────────────────────────────────────────────┐    │
│  │  GetKLines(instrument, period, count)            │    │
│  │  → return history klines                         │    │
│  └──────────────────────────────────────────────────┘    │
└────────────────────────────────────────────────────────────┘
         │                           │
         ▼ KLineFinished event       ▼ WAL append
┌─────────────────────┐     ┌──────────────────────────┐
│ MarketDataBroadcaster│     │   WalManager             │
│                     │     │                          │
│ → WebSocket clients │     │ → klines/wal_*.log       │
│ → DIFF sessions     │     │ → OLAP MemTable          │
└─────────────────────┘     └──────────────────────────┘
```

### 数据流详解

1. **Tick 事件生成**:
   - 撮合引擎每次成交后发布 tick 事件
   - MarketDataBroadcaster 广播给所有订阅者

2. **K 线聚合**:
   - KLineActor 订阅 tick 频道
   - 每个 tick 更新 7 个周期的当前 K 线
   - 周期切换时完成旧 K 线

3. **K 线完成处理**:
   - 广播 `KLineFinished` 事件（给 WebSocket 客户端）
   - 持久化到 WAL（崩溃恢复）
   - 写入 OLAP MemTable（分析查询）
   - 加入历史队列（限制 1000 根）

4. **查询服务**:
   - HTTP API: `GET /api/klines/{instrument}/{period}?count=100`
   - WebSocket DIFF: `set_chart` 指令
   - Actor 消息: `GetKLines` / `GetCurrentKLine`

## K线数据结构

### KLine 结构体

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KLine {
    /// K线起始时间戳（毫秒）
    pub timestamp: i64,

    /// 开盘价
    pub open: f64,

    /// 最高价
    pub high: f64,

    /// 最低价
    pub low: f64,

    /// 收盘价
    pub close: f64,

    /// 成交量
    pub volume: i64,

    /// 成交额
    pub amount: f64,

    /// 起始持仓量（DIFF协议要求）
    pub open_oi: i64,

    /// 结束持仓量（DIFF协议要求）
    pub close_oi: i64,

    /// 是否已完成
    pub is_finished: bool,
}
```

### K线周期定义

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KLinePeriod {
    Sec3 = 3,        // 3秒线
    Min1 = 60,       // 1分钟线
    Min5 = 300,      // 5分钟线
    Min15 = 900,     // 15分钟线
    Min30 = 1800,    // 30分钟线
    Min60 = 3600,    // 60分钟线 (1小时)
    Day = 86400,     // 日线
}
```

### 周期对齐算法

```rust
impl KLinePeriod {
    /// 计算K线周期的起始时间戳
    pub fn align_timestamp(&self, timestamp_ms: i64) -> i64 {
        let ts_sec = timestamp_ms / 1000;
        let period_sec = self.seconds();

        match self {
            KLinePeriod::Day => {
                // 日线：按UTC 0点对齐
                (ts_sec / 86400) * 86400 * 1000
            }
            _ => {
                // 分钟线/秒线：按周期对齐
                (ts_sec / period_sec) * period_sec * 1000
            }
        }
    }
}
```

**对齐示例**:

```
timestamp_ms = 1696684405123  (2023-10-07 12:40:05.123 UTC)

Min1:  align → 1696684380000  (2023-10-07 12:40:00.000)
Min5:  align → 1696684200000  (2023-10-07 12:35:00.000)
Min15: align → 1696683900000  (2023-10-07 12:30:00.000)
Day:   align → 1696636800000  (2023-10-07 00:00:00.000)
```

## KLineActor 实现

### Actor 定义

```rust
pub struct KLineActor {
    /// 各合约的K线聚合器
    aggregators: Arc<RwLock<HashMap<String, KLineAggregator>>>,

    /// 市场数据广播器（用于订阅tick和推送K线完成事件）
    broadcaster: Arc<MarketDataBroadcaster>,

    /// 订阅的合约列表（空表示订阅所有合约）
    subscribed_instruments: Vec<String>,

    /// WAL管理器（用于K线持久化和恢复）
    wal_manager: Arc<WalManager>,
}
```

### 启动流程

```rust
impl Actor for KLineActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        log::info!("📊 [KLineActor] Starting K-line aggregator...");

        // 1. 从WAL恢复历史数据
        self.recover_from_wal();

        // 2. 订阅市场数据的tick频道
        let subscriber_id = uuid::Uuid::new_v4().to_string();
        let receiver = self.broadcaster.subscribe(
            subscriber_id.clone(),
            self.subscribed_instruments.clone(),  // 空=订阅所有
            vec!["tick".to_string()],            // 只订阅tick
        );

        // 3. 启动异步任务持续接收tick事件
        let aggregators = self.aggregators.clone();
        let broadcaster = self.broadcaster.clone();
        let wal_manager = self.wal_manager.clone();

        let fut = async move {
            loop {
                match tokio::task::spawn_blocking(move || receiver.recv()).await {
                    Ok(Ok(event)) => {
                        if let MarketDataEvent::Tick {
                            instrument_id, price, volume, timestamp, ..
                        } = event {
                            // 聚合K线
                            let mut agg_map = aggregators.write();
                            let aggregator = agg_map
                                .entry(instrument_id.clone())
                                .or_insert_with(|| KLineAggregator::new(instrument_id.clone()));

                            let finished_klines = aggregator.on_tick(price, volume, timestamp);

                            // 处理完成的K线
                            for (period, kline) in finished_klines {
                                // 广播K线完成事件
                                broadcaster.broadcast(MarketDataEvent::KLineFinished {
                                    instrument_id: instrument_id.clone(),
                                    period: period.to_int(),
                                    kline: kline.clone(),
                                    timestamp,
                                });

                                // 持久化到WAL
                                let wal_record = WalRecord::KLineFinished {
                                    instrument_id: WalRecord::to_fixed_array_16(&instrument_id),
                                    period: period.to_int(),
                                    kline_timestamp: kline.timestamp,
                                    open: kline.open,
                                    high: kline.high,
                                    low: kline.low,
                                    close: kline.close,
                                    volume: kline.volume,
                                    amount: kline.amount,
                                    open_oi: kline.open_oi,
                                    close_oi: kline.close_oi,
                                    timestamp: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
                                };

                                wal_manager.append(wal_record)?;
                            }
                        }
                    }
                    Ok(Err(_)) => {
                        log::warn!("📊 [KLineActor] Market data channel disconnected");
                        break;
                    }
                    Err(e) => {
                        log::error!("📊 [KLineActor] spawn_blocking error: {}", e);
                        break;
                    }
                }
            }
        };

        ctx.spawn(actix::fut::wrap_future(fut));
    }
}
```

### WAL 恢复

```rust
fn recover_from_wal(&self) {
    log::info!("📊 [KLineActor] Recovering K-line data from WAL...");

    let mut recovered_count = 0;

    let result = self.wal_manager.replay(|entry| {
        if let WalRecord::KLineFinished {
            instrument_id,
            period,
            kline_timestamp,
            open, high, low, close,
            volume, amount,
            open_oi, close_oi,
            ..
        } = &entry.record {
            let instrument_id_str = WalRecord::from_fixed_array(instrument_id);

            if let Some(kline_period) = KLinePeriod::from_int(*period) {
                let kline = KLine {
                    timestamp: *kline_timestamp,
                    open: *open,
                    high: *high,
                    low: *low,
                    close: *close,
                    volume: *volume,
                    amount: *amount,
                    open_oi: *open_oi,
                    close_oi: *close_oi,
                    is_finished: true,
                };

                // 添加到aggregators的历史K线
                let mut agg_map = self.aggregators.write();
                let aggregator = agg_map
                    .entry(instrument_id_str.clone())
                    .or_insert_with(|| KLineAggregator::new(instrument_id_str.clone()));

                let history = aggregator.history_klines
                    .entry(kline_period)
                    .or_insert_with(Vec::new);

                history.push(kline);

                // 限制历史数量
                if history.len() > aggregator.max_history {
                    history.remove(0);
                }

                recovered_count += 1;
            }
        }
        Ok(())
    });

    log::info!(
        "📊 [KLineActor] WAL recovery completed: {} K-lines recovered",
        recovered_count
    );
}
```

### Actor 消息处理

#### GetKLines - 查询历史K线

```rust
#[derive(Message)]
#[rtype(result = "Vec<KLine>")]
pub struct GetKLines {
    pub instrument_id: String,
    pub period: KLinePeriod,
    pub count: usize,
}

impl Handler<GetKLines> for KLineActor {
    type Result = Vec<KLine>;

    fn handle(&mut self, msg: GetKLines, _ctx: &mut Context<Self>) -> Self::Result {
        let aggregators = self.aggregators.read();

        if let Some(aggregator) = aggregators.get(&msg.instrument_id) {
            aggregator.get_recent_klines(msg.period, msg.count)
        } else {
            Vec::new()
        }
    }
}
```

#### GetCurrentKLine - 查询当前K线

```rust
#[derive(Message)]
#[rtype(result = "Option<KLine>")]
pub struct GetCurrentKLine {
    pub instrument_id: String,
    pub period: KLinePeriod,
}

impl Handler<GetCurrentKLine> for KLineActor {
    type Result = Option<KLine>;

    fn handle(&mut self, msg: GetCurrentKLine, _ctx: &mut Context<Self>) -> Self::Result {
        let aggregators = self.aggregators.read();

        aggregators.get(&msg.instrument_id)
            .and_then(|agg| agg.get_current_kline(msg.period))
            .cloned()
    }
}
```

## K线聚合器

### KLineAggregator 结构

```rust
pub struct KLineAggregator {
    /// 合约代码
    instrument_id: String,

    /// 各周期的当前K线
    current_klines: HashMap<KLinePeriod, KLine>,

    /// 各周期的历史K线（最多保留1000根）
    history_klines: HashMap<KLinePeriod, Vec<KLine>>,

    /// 最大历史K线数量
    max_history: usize,
}
```

### 聚合算法

```rust
pub fn on_tick(&mut self, price: f64, volume: i64, timestamp_ms: i64) -> Vec<(KLinePeriod, KLine)> {
    let mut finished_klines = Vec::new();

    // 所有周期（分级采样）
    let periods = vec![
        KLinePeriod::Sec3,
        KLinePeriod::Min1,
        KLinePeriod::Min5,
        KLinePeriod::Min15,
        KLinePeriod::Min30,
        KLinePeriod::Min60,
        KLinePeriod::Day,
    ];

    for period in periods {
        let period_start = period.align_timestamp(timestamp_ms);

        // 检查是否需要开始新K线
        let need_new_kline = if let Some(current) = self.current_klines.get(&period) {
            current.timestamp != period_start  // 时间戳不同，开始新K线
        } else {
            true  // 第一次，创建K线
        };

        if need_new_kline {
            // 完成旧K线
            if let Some(mut old_kline) = self.current_klines.remove(&period) {
                old_kline.finish();  // 标记is_finished = true
                finished_klines.push((period, old_kline.clone()));

                // 加入历史
                let history = self.history_klines.entry(period).or_insert_with(Vec::new);
                history.push(old_kline);

                // 限制历史数量
                if history.len() > self.max_history {
                    history.remove(0);
                }
            }

            // 创建新K线
            self.current_klines.insert(period, KLine::new(period_start, price));
        }

        // 更新当前K线
        if let Some(kline) = self.current_klines.get_mut(&period) {
            kline.update(price, volume);  // 更新OHLCV
        }
    }

    finished_klines
}
```

### K线更新逻辑

```rust
impl KLine {
    pub fn new(timestamp: i64, price: f64) -> Self {
        Self {
            timestamp,
            open: price,
            high: price,
            low: price,
            close: price,
            volume: 0,
            amount: 0.0,
            open_oi: 0,
            close_oi: 0,
            is_finished: false,
        }
    }

    pub fn update(&mut self, price: f64, volume: i64) {
        // 更新HLCV
        if price > self.high {
            self.high = price;
        }
        if price < self.low {
            self.low = price;
        }
        self.close = price;
        self.volume += volume;
        self.amount += price * volume as f64;
    }

    pub fn update_open_interest(&mut self, open_interest: i64) {
        if self.open_oi == 0 {
            self.open_oi = open_interest;  // 第一次设置起始持仓
        }
        self.close_oi = open_interest;     // 每次更新结束持仓
    }

    pub fn finish(&mut self) {
        self.is_finished = true;
    }
}
```

## 协议支持

### HQChart 周期格式

QAExchange 支持 HQChart 标准周期格式：

| HQChart ID | 周期 | QAExchange 枚举 |
|-----------|------|----------------|
| 0 | 日线 | `KLinePeriod::Day` |
| 3 | 3秒线 | `KLinePeriod::Sec3` |
| 4 | 1分钟线 | `KLinePeriod::Min1` |
| 5 | 5分钟线 | `KLinePeriod::Min5` |
| 6 | 15分钟线 | `KLinePeriod::Min15` |
| 7 | 30分钟线 | `KLinePeriod::Min30` |
| 8 | 60分钟线 | `KLinePeriod::Min60` |

**转换方法**:

```rust
impl KLinePeriod {
    pub fn to_int(&self) -> i32 {
        match self {
            KLinePeriod::Day => 0,
            KLinePeriod::Sec3 => 3,
            KLinePeriod::Min1 => 4,
            KLinePeriod::Min5 => 5,
            KLinePeriod::Min15 => 6,
            KLinePeriod::Min30 => 7,
            KLinePeriod::Min60 => 8,
        }
    }

    pub fn from_int(val: i32) -> Option<Self> {
        match val {
            0 => Some(KLinePeriod::Day),
            3 => Some(KLinePeriod::Sec3),
            4 => Some(KLinePeriod::Min1),
            5 => Some(KLinePeriod::Min5),
            6 => Some(KLinePeriod::Min15),
            7 => Some(KLinePeriod::Min30),
            8 => Some(KLinePeriod::Min60),
            _ => None,
        }
    }
}
```

### DIFF 协议周期格式

DIFF 协议使用**纳秒时长**表示周期：

| 周期 | 纳秒时长 | 计算公式 |
|------|---------|---------|
| 3秒 | `3_000_000_000` | 3 × 10^9 |
| 1分钟 | `60_000_000_000` | 60 × 10^9 |
| 5分钟 | `300_000_000_000` | 300 × 10^9 |
| 15分钟 | `900_000_000_000` | 900 × 10^9 |
| 30分钟 | `1_800_000_000_000` | 1800 × 10^9 |
| 60分钟 | `3_600_000_000_000` | 3600 × 10^9 |
| 日线 | `86_400_000_000_000` | 86400 × 10^9 |

**转换方法**:

```rust
pub fn to_duration_ns(&self) -> i64 {
    match self {
        KLinePeriod::Sec3 => 3_000_000_000,
        KLinePeriod::Min1 => 60_000_000_000,
        KLinePeriod::Min5 => 300_000_000_000,
        KLinePeriod::Min15 => 900_000_000_000,
        KLinePeriod::Min30 => 1_800_000_000_000,
        KLinePeriod::Min60 => 3_600_000_000_000,
        KLinePeriod::Day => 86_400_000_000_000,
    }
}

pub fn from_duration_ns(duration_ns: i64) -> Option<Self> {
    match duration_ns {
        3_000_000_000 => Some(KLinePeriod::Sec3),
        60_000_000_000 => Some(KLinePeriod::Min1),
        300_000_000_000 => Some(KLinePeriod::Min5),
        900_000_000_000 => Some(KLinePeriod::Min15),
        1_800_000_000_000 => Some(KLinePeriod::Min30),
        3_600_000_000_000 => Some(KLinePeriod::Min60),
        86_400_000_000_000 => Some(KLinePeriod::Day),
        _ => None,
    }
}
```

### DIFF K线 ID 计算

DIFF 协议使用 K 线 ID 标识每根 K 线：

```rust
// K线ID = (timestamp_ms × 1_000_000) / duration_ns
let kline_id = (kline.timestamp * 1_000_000) / duration_ns;
```

**示例**:

```
timestamp_ms = 1696684800000  (2023-10-07 13:00:00.000 UTC)
duration_ns  = 60_000_000_000  (1分钟)

kline_id = (1696684800000 × 1_000_000) / 60_000_000_000
         = 1696684800000000000 / 60_000_000_000
         = 28278080
```

## API 使用

### HTTP API

#### 查询历史K线

```http
GET /api/klines/{instrument_id}/{period}?count=100

响应:
{
  "success": true,
  "data": [
    {
      "timestamp": 1696684800000,
      "open": 36500.0,
      "high": 36600.0,
      "low": 36480.0,
      "close": 36580.0,
      "volume": 1234,
      "amount": 45123456.0,
      "open_oi": 23000,
      "close_oi": 23100,
      "is_finished": true
    }
  ],
  "error": null
}
```

**参数说明**:
- `instrument_id`: 合约代码（如 `IF2501`）
- `period`: 周期（`3s` / `1min` / `5min` / `15min` / `30min` / `60min` / `day`）
- `count`: 查询数量（默认 100，最大 1000）

### WebSocket DIFF 协议

#### set_chart - 订阅K线图表

```json
// 客户端请求
{
  "aid": "set_chart",
  "chart_id": "chart1",
  "ins_list": "SHFE.cu1701",
  "duration": 60000000000,    // 1分钟（纳秒）
  "view_width": 500           // 最新500根K线
}
```

**参数说明**:
- `chart_id`: 图表 ID（同一 ID 后续请求会覆盖）
- `ins_list`: 合约列表（逗号分隔，第一个为主合约）
- `duration`: 周期（纳秒）
- `view_width`: 查询数量

#### 服务端响应 - 历史K线

```json
{
  "aid": "rtn_data",
  "data": [{
    "klines": {
      "SHFE.cu1701": {
        "60000000000": {
          "last_id": 28278080,
          "data": {
            "28278080": {
              "datetime": 1696684800000000000,  // UnixNano
              "open": 36500.0,
              "high": 36600.0,
              "low": 36480.0,
              "close": 36580.0,
              "volume": 1234,
              "open_oi": 23000,
              "close_oi": 23100
            }
          }
        }
      }
    }
  }]
}
```

#### 服务端推送 - 实时K线完成

```json
{
  "aid": "rtn_data",
  "data": [{
    "klines": {
      "SHFE.cu1701": {
        "60000000000": {
          "data": {
            "28278081": {
              "datetime": 1696684860000000000,
              "open": 36580.0,
              "high": 36650.0,
              "low": 36570.0,
              "close": 36620.0,
              "volume": 890,
              "open_oi": 23100,
              "close_oi": 23200
            }
          }
        }
      }
    }
  }]
}
```

### 代码示例

#### HTTP 查询

```rust
use reqwest;

let url = "http://localhost:8080/api/klines/IF2501/1min?count=100";
let response: serde_json::Value = reqwest::get(url).await?.json().await?;

let klines = response["data"].as_array().unwrap();
for kline in klines {
    println!(
        "Time: {}, OHLC: {}/{}/{}/{}, Volume: {}",
        kline["timestamp"],
        kline["open"],
        kline["high"],
        kline["low"],
        kline["close"],
        kline["volume"]
    );
}
```

#### WebSocket 订阅

```rust
use actix_web_actors::ws;

// 1. 连接WebSocket
let (tx, rx) = ws::Client::new("ws://localhost:8080/ws/diff")
    .connect()
    .await?;

// 2. 订阅K线图表
let set_chart = json!({
    "aid": "set_chart",
    "chart_id": "chart1",
    "ins_list": "IF2501",
    "duration": 60_000_000_000,  // 1分钟
    "view_width": 100
});
tx.send(Message::Text(set_chart.to_string())).await?;

// 3. 接收K线数据
while let Some(msg) = rx.next().await {
    match msg? {
        Message::Text(text) => {
            let data: serde_json::Value = serde_json::from_str(&text)?;
            if data["aid"] == "rtn_data" {
                // 处理K线数据
                println!("Received klines: {:?}", data["data"][0]["klines"]);
            }
        }
        _ => {}
    }
}
```

## 持久化和恢复

### WAL 记录结构

```rust
WalRecord::KLineFinished {
    instrument_id: [u8; 16],     // 合约ID
    period: i32,                 // 周期（HQChart格式）
    kline_timestamp: i64,        // K线起始时间戳（毫秒）
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: i64,
    amount: f64,
    open_oi: i64,                // 起始持仓量
    close_oi: i64,               // 结束持仓量
    timestamp: i64,              // 记录写入时间戳（纳秒）
}
```

### OLAP 列式存储

K 线数据写入 Arrow2 列式存储，支持高性能分析查询：

| 列名 | 数据类型 | 说明 |
|------|---------|------|
| `record_type` | `Int32` | 记录类型（13=KLineFinished） |
| `instrument_id` | `Binary` | 合约ID |
| `kline_period` | `Int32` | K线周期 |
| `kline_timestamp` | `Int64` | K线起始时间戳 |
| `kline_open` | `Float64` | 开盘价 |
| `kline_high` | `Float64` | 最高价 |
| `kline_low` | `Float64` | 最低价 |
| `kline_close` | `Float64` | 收盘价 |
| `kline_volume` | `Int64` | 成交量 |
| `kline_amount` | `Float64` | 成交额 |
| `kline_open_oi` | `Int64` | 起始持仓量 |
| `kline_close_oi` | `Int64` | 结束持仓量 |

### 查询示例（Polars）

```rust
use polars::prelude::*;

// 查询IF2501的1分钟K线，最近100根
let df = LazyFrame::scan_parquet("./data/olap/*.parquet", ScanArgsParquet::default())?
    .filter(
        col("record_type").eq(13)
            .and(col("instrument_id").eq(lit("IF2501")))
            .and(col("kline_period").eq(lit(4)))  // 4=1min
    )
    .sort("kline_timestamp", SortOptions::default().with_order_descending(true))
    .limit(100)
    .select(&[
        col("kline_timestamp"),
        col("kline_open"),
        col("kline_high"),
        col("kline_low"),
        col("kline_close"),
        col("kline_volume"),
    ])
    .collect()?;

println!("{:?}", df);
```

## 性能指标

| 指标 | 目标值 | 实测值 | 说明 |
|------|--------|--------|------|
| **聚合延迟** | < 100μs | ~50μs | tick → K线更新 |
| **WAL 写入延迟** | P99 < 50ms | ~20ms | K线完成 → WAL |
| **广播延迟** | < 1ms | ~500μs | K线完成 → WebSocket |
| **历史查询延迟** | < 10ms | ~5ms | HTTP API 查询100根K线 |
| **恢复速度** | < 5s | ~2s | WAL 恢复1万根K线 |
| **内存占用** | < 100MB | ~50MB | 100合约 × 7周期 × 1000历史 |

### 性能优化措施

1. **单Actor聚合**:
   - 所有合约的K线聚合在单个Actor中完成
   - 避免Actor间通信开销

2. **分级采样**:
   - 单个tick同时更新7个周期
   - 无需多次遍历

3. **限制历史数量**:
   - 每个周期最多保留1000根K线
   - 超出部分自动删除

4. **批量WAL写入**:
   - K线完成时立即追加WAL
   - 使用rkyv零拷贝序列化

5. **OLAP列式存储**:
   - Arrow2列式格式，查询性能优异
   - 支持SIMD加速

## 测试

### 单元测试

```bash
# 运行K线模块测试
cargo test --lib kline -- --nocapture

# 运行特定测试
cargo test --lib test_kline_aggregator
cargo test --lib test_wal_recovery
```

### 测试覆盖

- ✅ `test_kline_period_align` - K线周期对齐
- ✅ `test_kline_aggregator` - K线聚合器
- ✅ `test_kline_manager` - K线管理器
- ✅ `test_kline_finish` - K线完成机制
- ✅ `test_multiple_periods` - 多周期K线生成
- ✅ `test_open_interest_update` - 持仓量更新
- ✅ `test_period_conversion` - 周期格式转换
- ✅ `test_history_limit` - 历史K线数量限制
- ✅ `test_kline_actor_creation` - Actor创建
- ✅ `test_kline_query` - K线查询
- ✅ `test_wal_recovery` - **WAL持久化和恢复**（集成测试）

### WAL恢复测试示例

```rust
#[test]
fn test_wal_recovery() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let wal_path = tmp_dir.path().to_str().unwrap();

    // 第一步：创建WAL并写入K线数据
    {
        let wal_manager = crate::storage::wal::WalManager::new(wal_path);

        // 写入3根K线
        for i in 0..3 {
            let record = WalRecord::KLineFinished {
                instrument_id: WalRecord::to_fixed_array_16("IF2501"),
                period: 4, // Min1
                kline_timestamp: 1000000 + i * 60000, // 每分钟一根
                open: 3800.0 + i as f64,
                high: 3850.0 + i as f64,
                low: 3750.0 + i as f64,
                close: 3820.0 + i as f64,
                volume: 100 + i,
                amount: (3800.0 + i as f64) * (100 + i) as f64,
                open_oi: 1000,
                close_oi: 1010 + i,
                timestamp: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
            };
            wal_manager.append(record).unwrap();
        }
    }

    // 第二步：创建新的Actor并恢复
    {
        let broadcaster = Arc::new(MarketDataBroadcaster::new());
        let wal_manager = Arc::new(crate::storage::wal::WalManager::new(wal_path));
        let actor = KLineActor::new(broadcaster, wal_manager);

        // 触发恢复
        actor.recover_from_wal();

        // 验证恢复的数据
        let agg_map = actor.aggregators.read();
        let aggregator = agg_map.get("IF2501").expect("Should have IF2501 aggregator");

        let history = aggregator.history_klines.get(&KLinePeriod::Min1).expect("Should have Min1 history");
        assert_eq!(history.len(), 3, "Should have recovered 3 K-lines");

        // 验证第一根K线
        assert_eq!(history[0].open, 3800.0);
        assert_eq!(history[0].close, 3820.0);
        assert_eq!(history[0].volume, 100);
    }
}
```

## 故障排查

### 常见问题

**Q1: K线数据丢失**

检查项：
1. WAL 文件是否完整：`ls -lh ./data/wal/klines/`
2. Actor 是否启动：日志中搜索 `[KLineActor] Started successfully`
3. tick 订阅是否成功：日志中搜索 `Subscribed to tick events`

**Q2: K线更新延迟**

检查项：
1. tick 事件是否及时发布：`broadcaster.tick.throughput` 指标
2. Actor 队列积压：`actor.kline.pending_events` 指标
3. WAL 写入延迟：`wal.append_latency` 指标

**Q3: WebSocket 收不到K线**

检查项：
1. 是否订阅图表：`set_chart` 指令是否发送成功
2. 合约代码是否正确：需带交易所前缀（如 `SHFE.cu1612`）
3. 周期格式是否正确：duration 单位为纳秒

### 日志分析

**启动日志**:

```
[INFO] 📊 [KLineActor] Starting K-line aggregator...
[INFO] 📊 [KLineActor] Recovering K-line data from WAL...
[INFO] 📊 [KLineActor] WAL recovery completed: 1234 K-lines recovered
[INFO] 📊 [KLineActor] Subscribed to tick events (subscriber_id=xxx)
[INFO] 📊 [KLineActor] Started successfully
```

**K线完成日志**:

```
[DEBUG] 📊 [KLineActor] Finished IF2501 Min1 K-line: O=3800.00 H=3850.00 L=3750.00 C=3820.00 V=1234
[TRACE] 📊 [KLineActor] K-line persisted to WAL: IF2501 Min1
```

## 未来优化

1. **多级缓存**:
   - L1: Actor 内存（当前实现）
   - L2: Redis 缓存（计划中）
   - L3: OLAP 存储（已实现）

2. **压缩算法**:
   - 历史K线使用差分编码（Delta encoding）
   - 减少存储空间和网络传输

3. **分布式聚合**:
   - 多个 KLineActor 分担不同交易所的合约
   - 提升并发处理能力

4. **智能预加载**:
   - 根据用户订阅频率预加载热门合约K线
   - 减少查询延迟

## 相关文档

- [Actix Actor 架构](../../02_architecture/actor_architecture.md)
- [市场数据模块](README.md)
- [DIFF 协议](../../04_api/websocket/diff_protocol.md)
- [WAL 设计](../storage/wal.md)
- [OLAP 存储](../storage/memtable.md)

---

**模块作者**: @yutiansut @quantaxis
