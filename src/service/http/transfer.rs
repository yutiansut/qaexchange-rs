//! 银期转账 API 处理器
//! @yutiansut @quantaxis
//!
//! 提供银期转账相关的 REST API 接口：
//! - 获取签约银行列表
//! - 执行银期转账（入金/出金）
//! - 查询转账记录

use actix_web::{web, HttpResponse, Result};
use chrono::Utc;
use dashmap::DashMap;
use log;
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

use super::handlers::AppState;
use super::models::*;

/// 转账记录存储
/// 使用 DashMap 实现线程安全的存储
pub struct TransferStore {
    /// 转账记录：account_id -> Vec<TransferRecord>
    records: DashMap<String, Vec<TransferRecord>>,
    /// 签约银行：account_id -> Vec<BankInfo>
    banks: DashMap<String, Vec<BankInfo>>,
}

impl TransferStore {
    pub fn new() -> Self {
        Self {
            records: DashMap::new(),
            banks: DashMap::new(),
        }
    }

    /// 添加签约银行
    pub fn add_bank(&self, account_id: &str, bank: BankInfo) {
        self.banks
            .entry(account_id.to_string())
            .or_insert_with(Vec::new)
            .push(bank);
    }

    /// 获取签约银行列表
    pub fn get_banks(&self, account_id: &str) -> Vec<BankInfo> {
        self.banks
            .get(account_id)
            .map(|v| v.clone())
            .unwrap_or_default()
    }

    /// 添加转账记录
    pub fn add_record(&self, account_id: &str, record: TransferRecord) {
        self.records
            .entry(account_id.to_string())
            .or_insert_with(Vec::new)
            .push(record);
    }

    /// 获取转账记录
    pub fn get_records(
        &self,
        account_id: &str,
        start_date: Option<&str>,
        end_date: Option<&str>,
        page: Option<u32>,
        page_size: Option<u32>,
    ) -> (Vec<TransferRecord>, usize) {
        let records = self
            .records
            .get(account_id)
            .map(|v| v.clone())
            .unwrap_or_default();

        // 过滤日期
        let filtered: Vec<TransferRecord> = records
            .into_iter()
            .filter(|r| {
                let date_str = chrono::DateTime::from_timestamp_millis(r.datetime)
                    .map(|dt| dt.format("%Y-%m-%d").to_string())
                    .unwrap_or_default();

                let after_start = start_date
                    .map(|s| date_str.as_str() >= s)
                    .unwrap_or(true);
                let before_end = end_date.map(|e| date_str.as_str() <= e).unwrap_or(true);

                after_start && before_end
            })
            .collect();

        let total = filtered.len();

        // 分页
        let page = page.unwrap_or(1).max(1) as usize;
        let page_size = page_size.unwrap_or(20).min(100) as usize;
        let start = (page - 1) * page_size;

        let paged: Vec<TransferRecord> = filtered
            .into_iter()
            .skip(start)
            .take(page_size)
            .collect();

        (paged, total)
    }

    /// 初始化默认银行（用于演示）
    pub fn init_default_banks(&self, account_id: &str) {
        if self.banks.contains_key(account_id) {
            return;
        }

        let default_banks = vec![
            BankInfo {
                id: "ICBC".to_string(),
                name: "中国工商银行".to_string(),
            },
            BankInfo {
                id: "CCB".to_string(),
                name: "中国建设银行".to_string(),
            },
            BankInfo {
                id: "ABC".to_string(),
                name: "中国农业银行".to_string(),
            },
            BankInfo {
                id: "BOC".to_string(),
                name: "中国银行".to_string(),
            },
            BankInfo {
                id: "BOCOM".to_string(),
                name: "交通银行".to_string(),
            },
        ];

        for bank in default_banks {
            self.add_bank(account_id, bank);
        }
    }
}

impl Default for TransferStore {
    fn default() -> Self {
        Self::new()
    }
}

// 全局转账存储
lazy_static::lazy_static! {
    pub static ref TRANSFER_STORE: TransferStore = TransferStore::new();
}

/// 获取签约银行列表
/// GET /api/account/{account_id}/banks
pub async fn get_banks(
    account_id: web::Path<String>,
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse> {
    let account_id = account_id.into_inner();

    // 验证账户存在
    if state.account_mgr.get_account(&account_id).is_err() {
        return Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error(
            404,
            format!("账户不存在: {}", account_id),
        )));
    }

    // 初始化默认银行（如果需要）
    TRANSFER_STORE.init_default_banks(&account_id);

    let banks = TRANSFER_STORE.get_banks(&account_id);

    log::info!("获取账户 {} 的签约银行，共 {} 家", account_id, banks.len());

    Ok(HttpResponse::Ok().json(ApiResponse::success(json!({
        "banks": banks,
        "total": banks.len()
    }))))
}

/// 执行银期转账
/// POST /api/account/transfer
pub async fn do_transfer(
    req: web::Json<TransferRequest>,
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse> {
    let account_id = &req.account_id;
    let amount = req.amount;

    // 验证账户存在
    let account = match state.account_mgr.get_account(account_id) {
        Ok(acc) => acc,
        Err(e) => {
            return Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error(
                404,
                format!("账户不存在: {:?}", e),
            )));
        }
    };

    // 验证银行存在
    TRANSFER_STORE.init_default_banks(account_id);
    let banks = TRANSFER_STORE.get_banks(account_id);
    let bank = banks.iter().find(|b| b.id == req.bank_id);
    if bank.is_none() {
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(
            4001,
            format!("未签约银行: {}", req.bank_id),
        )));
    }
    let bank = bank.unwrap();

    // 验证金额
    if amount.abs() < 0.01 {
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(
            4002,
            "转账金额不能小于0.01".to_string(),
        )));
    }

    // 验证密码（简化处理，实际需要调用银行接口）
    // 这里只做基本验证
    if req.bank_password.is_empty() || req.future_password.is_empty() {
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(
            4003,
            "银行密码或期货密码不能为空".to_string(),
        )));
    }

    // 执行转账
    let mut acc = account.write();
    let transfer_id = Uuid::new_v4().to_string();
    let now = Utc::now().timestamp_millis();

    let (error_id, error_msg) = if amount > 0.0 {
        // 入金
        acc.deposit(amount);
        log::info!(
            "银期转账: 账户 {} 入金 {} 成功",
            account_id,
            amount
        );
        (0, "转账成功".to_string())
    } else {
        // 出金 - 检查可用余额
        let withdraw_amount = amount.abs();
        if acc.money < withdraw_amount {
            log::warn!(
                "银期转账: 账户 {} 出金 {} 失败，可用余额不足 ({})",
                account_id,
                withdraw_amount,
                acc.money
            );
            (-1, "可用余额不足".to_string())
        } else {
            acc.withdraw(withdraw_amount);
            log::info!(
                "银期转账: 账户 {} 出金 {} 成功",
                account_id,
                withdraw_amount
            );
            (0, "转账成功".to_string())
        }
    };

    // 记录转账
    let record = TransferRecord {
        id: transfer_id.clone(),
        datetime: now,
        currency: "CNY".to_string(),
        amount,
        error_id,
        error_msg: error_msg.clone(),
        bank_id: req.bank_id.clone(),
        bank_name: bank.name.clone(),
    };
    TRANSFER_STORE.add_record(account_id, record.clone());

    if error_id != 0 {
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(
            4004,
            error_msg,
        )));
    }

    Ok(HttpResponse::Ok().json(ApiResponse::success(json!({
        "transfer_id": transfer_id,
        "balance": acc.get_balance(),
        "available": acc.money,
        "message": error_msg
    }))))
}

/// 查询转账记录
/// GET /api/account/{account_id}/transfers
pub async fn get_transfers(
    account_id: web::Path<String>,
    query: web::Query<TransferQueryRequest>,
    state: web::Data<Arc<AppState>>,
) -> Result<HttpResponse> {
    let account_id = account_id.into_inner();

    // 验证账户存在
    if state.account_mgr.get_account(&account_id).is_err() {
        return Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error(
            404,
            format!("账户不存在: {}", account_id),
        )));
    }

    let (records, total) = TRANSFER_STORE.get_records(
        &account_id,
        query.start_date.as_deref(),
        query.end_date.as_deref(),
        query.page,
        query.page_size,
    );

    log::info!(
        "查询账户 {} 的转账记录，共 {} 条 (总计 {})",
        account_id,
        records.len(),
        total
    );

    Ok(HttpResponse::Ok().json(ApiResponse::success(json!({
        "records": records,
        "total": total,
        "page": query.page.unwrap_or(1),
        "page_size": query.page_size.unwrap_or(20)
    }))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transfer_store() {
        let store = TransferStore::new();
        let account_id = "test_account";

        // 初始化银行
        store.init_default_banks(account_id);
        let banks = store.get_banks(account_id);
        assert!(!banks.is_empty());

        // 添加转账记录
        let record = TransferRecord {
            id: "test_transfer".to_string(),
            datetime: Utc::now().timestamp_millis(),
            currency: "CNY".to_string(),
            amount: 10000.0,
            error_id: 0,
            error_msg: "成功".to_string(),
            bank_id: "ICBC".to_string(),
            bank_name: "中国工商银行".to_string(),
        };
        store.add_record(account_id, record);

        let (records, total) = store.get_records(account_id, None, None, None, None);
        assert_eq!(total, 1);
        assert_eq!(records.len(), 1);
    }
}
