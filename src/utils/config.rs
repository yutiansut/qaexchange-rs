//! 配置管理模块

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeConfig {
    pub server: ServerConfig,
    pub http: HttpConfig,
    pub websocket: WebSocketConfig,
    pub storage: StorageConfig,
    pub instruments: Vec<InstrumentConfig>,
    /// `[settlement]` 日终结算(缺省 = 每自然日北京 00:00 自动结算)
    #[serde(default)]
    pub settlement: SettlementSettings,
}

/// `[settlement]` —— 日终结算调度 @yutiansut @quantaxis
///
/// 用户拍板:**每个自然日做日终结算**(仿真 7×24 运行,不按交易日历)。
/// 流程见 `SettlementEngine::run_end_of_day`:闭市 → 撤活单 → 归档 → 结算 → 开市。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettlementSettings {
    /// 是否按 `time` 每天自动结算
    #[serde(default = "default_settlement_auto_daily")]
    pub auto_daily: bool,
    /// 北京时间 `HH:MM`。结算的是这一时刻之前的那个自然日(00:00 → 刚结束的前一天)
    #[serde(default = "default_settlement_time")]
    pub time: String,
}

impl Default for SettlementSettings {
    fn default() -> Self {
        Self {
            auto_daily: default_settlement_auto_daily(),
            time: default_settlement_time(),
        }
    }
}

fn default_settlement_auto_daily() -> bool {
    true
}

fn default_settlement_time() -> String {
    "00:00".to_string()
}

/// 性能优化配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
pub struct PerformanceConfig {
    #[serde(default)]
    pub batch_write: BatchWriteConfig,
    #[serde(default)]
    pub websocket: WebSocketPerfConfig,
    #[serde(default)]
    pub priority_queue: PriorityQueueConfig,
    #[serde(default)]
    pub memtable: MemTableConfig,
    #[serde(default)]
    pub iceoryx: IceoryxConfig,
}


impl PerformanceConfig {
    /// 从文件加载性能配置
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let content = fs::read_to_string(path.as_ref())
            .map_err(|e| format!("Failed to read performance config file: {}", e))?;
        toml::from_str(&content)
            .map_err(|e| format!("Failed to parse performance config file: {}", e))
    }

    /// 加载默认性能配置文件
    pub fn load_default() -> Result<Self, String> {
        Self::load_from_file("config/performance.toml")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchWriteConfig {
    #[serde(default = "default_buffer_size")]
    pub tick_buffer_size: usize,
    #[serde(default = "default_flush_interval")]
    pub tick_flush_interval_ms: u64,
    #[serde(default = "default_batch_size")]
    pub max_batch_size: usize,
    #[serde(default = "default_snapshot_interval")]
    pub snapshot_interval_ms: u64,
}

impl Default for BatchWriteConfig {
    fn default() -> Self {
        Self {
            tick_buffer_size: 1000,
            tick_flush_interval_ms: 10,
            max_batch_size: 1000,
            snapshot_interval_ms: 1000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketPerfConfig {
    #[serde(default = "default_ws_batch_size")]
    pub batch_size: usize,
    #[serde(default = "default_flush_interval")]
    pub batch_timeout_ms: u64,
    #[serde(default = "default_queue_threshold")]
    pub queue_threshold: usize,
}

impl Default for WebSocketPerfConfig {
    fn default() -> Self {
        Self {
            batch_size: 100,
            batch_timeout_ms: 10,
            queue_threshold: 500,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriorityQueueConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_low_queue_limit")]
    pub low_queue_limit: usize,
    #[serde(default = "default_critical_threshold")]
    pub critical_amount_threshold: f64,
    #[serde(default)]
    pub vip_users: Vec<String>,
}

impl Default for PriorityQueueConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            low_queue_limit: 100,
            critical_amount_threshold: 1_000_000.0,
            vip_users: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemTableConfig {
    #[serde(default = "default_memtable_size")]
    pub max_size_mb: usize,
    #[serde(default = "default_entry_size")]
    pub estimated_entry_size: usize,
}

impl Default for MemTableConfig {
    fn default() -> Self {
        Self {
            max_size_mb: 64,
            estimated_entry_size: 256,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IceoryxConfig {
    /// 是否启用 iceoryx2 零拷贝 IPC
    #[serde(default = "default_false")]
    pub enabled: bool,

    /// 服务名称前缀
    #[serde(default = "default_service_prefix")]
    pub service_prefix: String,

    /// 最大订阅者数量
    #[serde(default = "default_max_subscribers")]
    pub max_subscribers: usize,

    /// 消息队列容量
    #[serde(default = "default_queue_capacity")]
    pub queue_capacity: usize,

    /// 最大消息大小（字节）
    #[serde(default = "default_max_message_size")]
    pub max_message_size: usize,
}

impl Default for IceoryxConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            service_prefix: "qaexchange".to_string(),
            max_subscribers: 1000,
            queue_capacity: 1024,
            max_message_size: 4096,
        }
    }
}

// 默认值函数
fn default_buffer_size() -> usize {
    1000
}
fn default_flush_interval() -> u64 {
    10
}
fn default_batch_size() -> usize {
    1000
}
fn default_snapshot_interval() -> u64 {
    1000
}
fn default_ws_batch_size() -> usize {
    100
}
fn default_queue_threshold() -> usize {
    500
}
fn default_true() -> bool {
    true
}
fn default_false() -> bool {
    false
}
fn default_low_queue_limit() -> usize {
    100
}
fn default_critical_threshold() -> f64 {
    1_000_000.0
}
fn default_memtable_size() -> usize {
    64
}
fn default_entry_size() -> usize {
    256
}
fn default_service_prefix() -> String {
    "qaexchange".to_string()
}
fn default_max_subscribers() -> usize {
    1000
}
fn default_queue_capacity() -> usize {
    1024
}
fn default_max_message_size() -> usize {
    4096
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub name: String,
    pub environment: String,
    pub log_level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpConfig {
    pub host: String,
    pub port: u16,
}

impl HttpConfig {
    pub fn bind_address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketConfig {
    pub host: String,
    pub port: u16,
}

impl WebSocketConfig {
    pub fn bind_address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub enabled: bool,
    pub base_path: String,
    pub subscriber: SubscriberConfig,
    /// `[storage.wal]` —— 此前整段被 serde **静默丢弃**(结构体里没有这个字段),
    /// 配置写着 `sync_mode = "async"`,实际热路径逐条 fsync。@yutiansut @quantaxis
    /// 缺省 = 同步(保持未配置时的旧行为)。同段里的 `directory` / `max_file_size`
    /// 仍未接线(WAL 目录与 1GB 滚动阈值由代码决定)。
    #[serde(default)]
    pub wal: WalSettings,
}

/// `[storage.wal]` 落盘模式
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalSettings {
    /// `"sync"`:每条记录 fsync;`"async"`:组提交,后台线程按 `sync_interval_ms` 刷盘
    #[serde(default = "default_wal_sync_mode")]
    pub sync_mode: String,
    /// async 模式的刷盘间隔(毫秒)= 崩溃时最多丢失的时间窗
    #[serde(default = "default_wal_sync_interval_ms")]
    pub sync_interval_ms: u64,
}

impl Default for WalSettings {
    fn default() -> Self {
        Self {
            sync_mode: default_wal_sync_mode(),
            sync_interval_ms: default_wal_sync_interval_ms(),
        }
    }
}

impl WalSettings {
    /// 是否异步(组提交)模式;非法取值按同步处理并由调用方告警
    pub fn is_async(&self) -> bool {
        self.sync_mode.eq_ignore_ascii_case("async")
    }
}

fn default_wal_sync_mode() -> String {
    "sync".to_string()
}

fn default_wal_sync_interval_ms() -> u64 {
    5
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriberConfig {
    pub batch_size: usize,
    pub batch_timeout_ms: u64,
    pub buffer_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstrumentConfig {
    pub instrument_id: String,
    pub name: String,
    pub exchange_id: String,
    pub product_type: String,
    pub init_price: f64,
    pub is_trading: bool,
    /// 合约乘数。`exchange.toml` 里一直写的是 `contract_multiplier`,而字段名是 `multiplier` ——
    /// 没有别名时 serde 静默丢弃、一律回落成 300(FG/SA 真实是 20)。两种写法都认。@yutiansut @quantaxis
    #[serde(default = "default_multiplier", alias = "contract_multiplier")]
    pub multiplier: f64,
    #[serde(default = "default_tick_size")]
    pub tick_size: f64,
    /// ✨ 按合约覆盖 qars MarketPreset 的费率(不改共享库;见 `core::account_ext::preset_overrides`)。
    /// 每手手续费(元;开仓 / 平仓 / 平今同价)。设了就不再按成交额收。@yutiansut @quantaxis
    #[serde(default)]
    pub commission_per_lot: Option<f64>,
    /// 按成交额的手续费率(开仓 / 平仓 / 平今同率)。设了就不再按手收。
    #[serde(default)]
    pub commission_rate: Option<f64>,
    /// 保证金率(多空同)。
    #[serde(default)]
    pub margin_rate: Option<f64>,
}

fn default_multiplier() -> f64 {
    300.0
}

fn default_tick_size() -> f64 {
    0.2
}

impl ExchangeConfig {
    /// 本结构体实际解析的顶层配置段
    ///
    /// 用于在启动时把「写了但从不被读」的配置段告警出来 —— 见
    /// `warn_unparsed_sections` 的说明。@yutiansut @quantaxis
    const KNOWN_SECTIONS: &'static [&'static str] =
        &["server", "http", "websocket", "storage", "instruments", "settlement"];

    /// 对配置文件里**存在但本程序从不解析**的顶层段发出告警
    ///
    /// ⚠️ `ExchangeConfig` 没有 `deny_unknown_fields`,serde 会**静默丢弃**
    /// 未知段。实测 `config/exchange.toml` 里有三整段从不被解析:
    ///
    /// | 段 | 典型内容 | 代码实际生效值 |
    /// |---|---|---|
    /// | `[risk]` | `max_order_amount = 1000.0` | **10_000_000**(差 1 万倍) |
    /// |          | `max_position_ratio = 80.0`(百分数) | **0.5**(比率,单位都不一致) |
    /// | `[matching]` | `max_order_volume = 1000.0` 等 | 未接线 |
    /// | `[account]` | `margin_rate = 0.15` | registry 0.12 / qars preset 0.05~0.12 |
    ///
    /// 运维在这里调风控限额得不到任何效果、也没有任何提示 —— 这是最危险的一种
    /// 「配置陷阱」。这里只**告警不阻断**:直接加 `deny_unknown_fields` 会让
    /// 现有配置文件启动失败;真正的修法是把这些段接线(需要先统一单位)。
    fn warn_unparsed_sections(content: &str) {
        let parsed: toml::Value = match toml::from_str(content) {
            Ok(v) => v,
            Err(_) => return, // 解析失败会由调用方报错,这里不重复
        };
        if let Some(table) = parsed.as_table() {
            let unknown: Vec<&str> = table
                .keys()
                .map(|k| k.as_str())
                .filter(|k| !Self::KNOWN_SECTIONS.contains(k))
                .collect();
            if !unknown.is_empty() {
                log::warn!(
                    "⚠️ 配置文件中有 {} 个段**从不被解析**,其中的设置全部无效: {:?}。\
                     本程序只读取 {:?}。若要让这些设置生效,需要在 ExchangeConfig 里接线。",
                    unknown.len(),
                    unknown,
                    Self::KNOWN_SECTIONS
                );
            }
            // ✨ 子表同样会被静默丢弃:`[storage.wal]` 就是这样埋了很久的死配置 @yutiansut @quantaxis
            if let Some(storage) = table.get("storage").and_then(|v| v.as_table()) {
                const KNOWN_STORAGE: &[&str] = &["enabled", "base_path", "subscriber", "wal"];
                let unknown: Vec<&str> = storage
                    .keys()
                    .map(|k| k.as_str())
                    .filter(|k| !KNOWN_STORAGE.contains(k))
                    .collect();
                if !unknown.is_empty() {
                    log::warn!(
                        "⚠️ [storage] 下有 {} 个子项**从不被解析**,其中的设置全部无效: {:?}(只读取 {:?})",
                        unknown.len(),
                        unknown,
                        KNOWN_STORAGE
                    );
                }
            }
        }
    }

    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let content = fs::read_to_string(path.as_ref())
            .map_err(|e| format!("Failed to read config file: {}", e))?;
        Self::warn_unparsed_sections(&content);
        toml::from_str(&content).map_err(|e| format!("Failed to parse config file: {}", e))
    }

    pub fn load_default() -> Result<Self, String> {
        Self::load_from_file("config/exchange.toml")
    }
}
