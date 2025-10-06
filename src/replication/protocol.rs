//! 复制协议定义

use serde::{Serialize, Deserialize};
use rkyv::Deserialize as RkyvDeserialize;
use crate::storage::wal::WalRecord;

/// 复制消息类型（可序列化版本，用于网络传输）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ReplicationMessage {
    /// 日志复制请求
    LogReplication(SerializableReplicationRequest),

    /// 日志复制响应
    LogReplicationResponse(ReplicationResponse),

    /// 心跳请求
    Heartbeat(HeartbeatRequest),

    /// 心跳响应
    HeartbeatResponse(HeartbeatResponse),

    /// 快照传输
    Snapshot(SnapshotRequest),

    /// 快照响应
    SnapshotResponse(SnapshotResponse),
}

/// 日志条目（用于内存存储，不直接序列化）
#[derive(Debug, Clone)]
pub struct LogEntry {
    /// 日志序列号
    pub sequence: u64,

    /// 日志term（选举轮次）
    pub term: u64,

    /// WAL记录
    pub record: WalRecord,

    /// 时间戳
    pub timestamp: i64,
}

/// 可序列化的日志条目（用于网络传输）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableLogEntry {
    /// 日志序列号
    pub sequence: u64,

    /// 日志term（选举轮次）
    pub term: u64,

    /// WAL记录（rkyv序列化后的字节）
    pub record_bytes: Vec<u8>,

    /// 时间戳
    pub timestamp: i64,
}

impl LogEntry {
    /// 转换为可序列化格式
    pub fn to_serializable(&self) -> Result<SerializableLogEntry, String> {
        let record_bytes = rkyv::to_bytes::<_, 2048>(&self.record)
            .map_err(|e| format!("Serialize record failed: {}", e))?
            .to_vec();

        Ok(SerializableLogEntry {
            sequence: self.sequence,
            term: self.term,
            record_bytes,
            timestamp: self.timestamp,
        })
    }

    /// 从可序列化格式创建
    pub fn from_serializable(se: SerializableLogEntry) -> Result<Self, String> {
        let archived = rkyv::check_archived_root::<WalRecord>(&se.record_bytes)
            .map_err(|e| format!("Deserialize record failed: {}", e))?;

        let record: WalRecord = RkyvDeserialize::deserialize(archived, &mut rkyv::Infallible)
            .map_err(|e| format!("Deserialize record failed: {:?}", e))?;

        Ok(LogEntry {
            sequence: se.sequence,
            term: se.term,
            record,
            timestamp: se.timestamp,
        })
    }
}

/// 日志复制请求（内存版本）
#[derive(Debug, Clone)]
pub struct ReplicationRequest {
    /// Master term
    pub term: u64,

    /// Master ID
    pub leader_id: String,

    /// 前一个日志条目的序列号
    pub prev_log_sequence: u64,

    /// 前一个日志条目的term
    pub prev_log_term: u64,

    /// 要复制的日志条目
    pub entries: Vec<LogEntry>,

    /// Master的commit序列号
    pub leader_commit: u64,
}

/// 可序列化的日志复制请求（网络传输）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableReplicationRequest {
    pub term: u64,
    pub leader_id: String,
    pub prev_log_sequence: u64,
    pub prev_log_term: u64,
    pub entries: Vec<SerializableLogEntry>,
    pub leader_commit: u64,
}

impl ReplicationRequest {
    pub fn to_serializable(&self) -> Result<SerializableReplicationRequest, String> {
        let entries = self
            .entries
            .iter()
            .map(|e| e.to_serializable())
            .collect::<Result<Vec<_>, _>>()?;

        Ok(SerializableReplicationRequest {
            term: self.term,
            leader_id: self.leader_id.clone(),
            prev_log_sequence: self.prev_log_sequence,
            prev_log_term: self.prev_log_term,
            entries,
            leader_commit: self.leader_commit,
        })
    }

    pub fn from_serializable(sr: SerializableReplicationRequest) -> Result<Self, String> {
        let entries = sr
            .entries
            .into_iter()
            .map(LogEntry::from_serializable)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(ReplicationRequest {
            term: sr.term,
            leader_id: sr.leader_id,
            prev_log_sequence: sr.prev_log_sequence,
            prev_log_term: sr.prev_log_term,
            entries,
            leader_commit: sr.leader_commit,
        })
    }
}

/// 日志复制响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationResponse {
    /// Slave term
    pub term: u64,

    /// 是否成功
    pub success: bool,

    /// 当前匹配的序列号
    pub match_sequence: u64,

    /// 错误信息（失败时）
    pub error: Option<String>,
}

/// 心跳请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatRequest {
    /// Master term
    pub term: u64,

    /// Master ID
    pub leader_id: String,

    /// Master commit序列号
    pub leader_commit: u64,

    /// 时间戳
    pub timestamp: i64,
}

/// 心跳响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatResponse {
    /// Slave term
    pub term: u64,

    /// Slave ID
    pub node_id: String,

    /// 当前日志序列号
    pub last_log_sequence: u64,

    /// 是否健康
    pub healthy: bool,
}

/// 快照请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotRequest {
    /// Master term
    pub term: u64,

    /// 快照包含的最后序列号
    pub last_included_sequence: u64,

    /// 快照包含的最后term
    pub last_included_term: u64,

    /// 快照数据（可能分片传输）
    pub data: Vec<u8>,

    /// 是否是最后一片
    pub is_last_chunk: bool,
}

/// 快照响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotResponse {
    /// Slave term
    pub term: u64,

    /// 是否成功
    pub success: bool,

    /// 错误信息
    pub error: Option<String>,
}
