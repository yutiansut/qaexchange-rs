//! 账户管理功能 HTTP API 处理器
//! Phase 12-13: 密码管理、手续费、保证金、账户冻结、审计日志、系统公告
//! @yutiansut @quantaxis

use actix_web::{web, HttpResponse};
use crate::core::account_ext::AccountExt;   // safe_risk_ratio @yutiansut @quantaxis
use dashmap::DashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;
use tokio::sync::OnceCell;

use super::models::*;
use crate::exchange::account_mgr::AccountManager;
use crate::protocol::diff::snapshot::SnapshotManager;

// ==================== 内存存储（生产环境应使用数据库） ====================

lazy_static::lazy_static! {
    // 账户密码存储 (account_id -> (trading_password, fund_password))
    static ref ACCOUNT_PASSWORDS: DashMap<String, (String, String)> = DashMap::new();

    // 手续费率存储 (product_id -> CommissionRate)
    static ref COMMISSION_RATES: DashMap<String, CommissionRate> = {
        let map = DashMap::new();
        init_default_commission_rates(&map);
        map
    };

    // 保证金率存储 (product_id -> MarginRate)
    static ref MARGIN_RATES: DashMap<String, MarginRate> = {
        let map = DashMap::new();
        init_default_margin_rates(&map);
        map
    };

    // 账户状态存储 (account_id -> AccountStatusInfo)
    static ref ACCOUNT_STATUS: DashMap<String, AccountStatusInfo> = DashMap::new();

    // 审计日志存储
    static ref AUDIT_LOGS: DashMap<String, AuditLogEntry> = DashMap::new();

    // 系统公告存储
    static ref ANNOUNCEMENTS: DashMap<String, Announcement> = DashMap::new();
}

// ==================== 全局 SnapshotManager（用于广播公告）====================
// @yutiansut @quantaxis

static GLOBAL_SNAPSHOT_MANAGER: OnceCell<Arc<SnapshotManager>> = OnceCell::const_new();

/// 设置全局 SnapshotManager（由 WebSocket 服务器调用）
pub fn set_global_snapshot_manager(mgr: Arc<SnapshotManager>) {
    let _ = GLOBAL_SNAPSHOT_MANAGER.set(mgr);
    log::info!("✅ Global SnapshotManager set for announcement broadcasting");
}

/// 获取全局 SnapshotManager
pub fn get_global_snapshot_manager() -> Option<Arc<SnapshotManager>> {
    GLOBAL_SNAPSHOT_MANAGER.get().cloned()
}

// 管理员令牌验证（从环境变量读取，生产环境应使用JWT等）
fn get_admin_token() -> String {
    std::env::var("QAEXCHANGE_ADMIN_TOKEN")
        .unwrap_or_else(|_| "qaexchange_admin_2024".to_string())
}

fn verify_admin_token(token: &str) -> bool {
    token == get_admin_token()
}

/// 校验账户的交易/资金密码 @yutiansut @quantaxis
///
/// 供 transfer.rs 等模块复用 —— 此前 do_transfer 只检查密码**非空**
/// (transfer.rs:226 `is_empty()`),从不与任何东西比对,密码字段纯装饰。
/// 实测:`bank_password:"x", future_password:"x"` 转入 50,000 直接成功。
///
/// ⚠️ 本函数只是把已有的存储接上,**没有解决存储本身的问题**:
///   · ACCOUNT_PASSWORDS 是明文 `DashMap<String,(String,String)>`(:20),
///     用 `!=` 直接比较 —— 而同一代码库的用户登录密码是
///     `bcrypt::hash(.., DEFAULT_COST)`(user_manager.rs:80)
///   · 默认密码硬编码为 "123456"(:232,:239,:272),交易与资金密码都是
///   · 该表**从不持久化**(全库只有 insert/get/entry),重启后所有改动丢失、
///     一律回到 "123456"
/// 这三条需要单独的设计决定(哈希算法、默认密码策略、落盘位置),不在此处解决。
pub(crate) fn verify_account_password(
    account_id: &str,
    password_type: PasswordType,
    password: &str,
) -> bool {
    match ACCOUNT_PASSWORDS.get(account_id) {
        Some(p) => {
            let expected = match password_type {
                PasswordType::Trading => &p.0,
                PasswordType::Fund => &p.1,
            };
            expected == password
        }
        // 未设置过密码的账户沿用默认值,与 change_password(:232) 的判定保持一致
        None => password == "123456",
    }
}

/// 查账户状态的小写字符串形式 @yutiansut @quantaxis
///
/// `ACCOUNT_STATUS`(:37)只在冻结/解冻时写入,从未冻结过的账户**没有条目** ——
/// 那是隐含的 Active,不是"未知"。管理端列表要按状态筛选就得走这里。
pub fn account_status_str(account_id: &str) -> &'static str {
    match ACCOUNT_STATUS.get(account_id).map(|s| s.status.clone()) {
        Some(AccountStatus::Frozen) => "frozen",
        Some(AccountStatus::Suspended) => "suspended",
        Some(AccountStatus::Closed) => "closed",
        Some(AccountStatus::Active) | None => "active",
    }
}

/// 开户时把密码写入校验表 @yutiansut @quantaxis
///
/// ⚠️ 此前**没有任何地方在开户时写这张表** —— `ACCOUNT_PASSWORDS`(:20)只被
/// `change_password`(:270)和 `reset_password`(:303)写入。于是所有新账户在
/// `verify_account_password` 里都落到 `None => password == "123456"` 分支:
///   · 用户拿开户时自己设的密码改密码 → 恒定 400「原密码错误」
///   · 银期转账的资金密码 → 只有 "123456" 被接受
/// `OpenAccountRequest.password`(models.rs:45)被 HTTP 层收下后**直接丢弃**,
/// 因为 `CoreOpenAccountRequest` 根本没有这个字段。
///
/// 交易密码与资金密码初始设为同一个值(开户请求只提供一个密码);
/// 用户之后可分别通过 `password_type=TRADING/FUND` 修改。
///
/// 注:`/api/user/{id}/account/create` 走的 `CreateAccountRequest`(models.rs:195)
/// **没有密码字段**,那条路径仍沿用默认值 —— 要改需先给请求体加字段。
pub fn seed_account_password(account_id: &str, password: &str) {
    if password.is_empty() {
        return;
    }
    ACCOUNT_PASSWORDS.insert(
        account_id.to_string(),
        (password.to_string(), password.to_string()),
    );
    log::info!("账户 {} 的交易/资金密码已初始化", account_id);
}

fn current_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

// ==================== 初始化默认数据 ====================

fn init_default_commission_rates(map: &DashMap<String, CommissionRate>) {
    // 股指期货
    for code in &["IF", "IH", "IC", "IM"] {
        map.insert(code.to_string(), CommissionRate {
            instrument_id: code.to_string(),
            exchange_id: "CFFEX".to_string(),
            product_id: code.to_string(),
            open_ratio_by_money: 0.000023,
            open_ratio_by_volume: 0.0,
            close_ratio_by_money: 0.000023,
            close_ratio_by_volume: 0.0,
            close_today_ratio_by_money: 0.000345,
            close_today_ratio_by_volume: 0.0,
        });
    }
    // 国债期货
    for code in &["T", "TF", "TL", "TS"] {
        map.insert(code.to_string(), CommissionRate {
            instrument_id: code.to_string(),
            exchange_id: "CFFEX".to_string(),
            product_id: code.to_string(),
            open_ratio_by_money: 0.0,
            open_ratio_by_volume: 3.0,
            close_ratio_by_money: 0.0,
            close_ratio_by_volume: 3.0,
            close_today_ratio_by_money: 0.0,
            close_today_ratio_by_volume: 0.0,
        });
    }
    // 商品期货
    map.insert("cu".to_string(), CommissionRate {
        instrument_id: "cu".to_string(),
        exchange_id: "SHFE".to_string(),
        product_id: "cu".to_string(),
        open_ratio_by_money: 0.00005,
        open_ratio_by_volume: 0.0,
        close_ratio_by_money: 0.00005,
        close_ratio_by_volume: 0.0,
        close_today_ratio_by_money: 0.00005,
        close_today_ratio_by_volume: 0.0,
    });
    map.insert("au".to_string(), CommissionRate {
        instrument_id: "au".to_string(),
        exchange_id: "SHFE".to_string(),
        product_id: "au".to_string(),
        open_ratio_by_money: 0.0,
        open_ratio_by_volume: 10.0,
        close_ratio_by_money: 0.0,
        close_ratio_by_volume: 10.0,
        close_today_ratio_by_money: 0.0,
        close_today_ratio_by_volume: 0.0,
    });
    map.insert("i".to_string(), CommissionRate {
        instrument_id: "i".to_string(),
        exchange_id: "DCE".to_string(),
        product_id: "i".to_string(),
        open_ratio_by_money: 0.0001,
        open_ratio_by_volume: 0.0,
        close_ratio_by_money: 0.0001,
        close_ratio_by_volume: 0.0,
        close_today_ratio_by_money: 0.0001,
        close_today_ratio_by_volume: 0.0,
    });
    map.insert("sc".to_string(), CommissionRate {
        instrument_id: "sc".to_string(),
        exchange_id: "INE".to_string(),
        product_id: "sc".to_string(),
        open_ratio_by_money: 0.0,
        open_ratio_by_volume: 20.0,
        close_ratio_by_money: 0.0,
        close_ratio_by_volume: 20.0,
        close_today_ratio_by_money: 0.0,
        close_today_ratio_by_volume: 0.0,
    });
}

fn init_default_margin_rates(map: &DashMap<String, MarginRate>) {
    // 股指期货
    for (code, ratio) in &[("IF", 0.12), ("IH", 0.12), ("IC", 0.14), ("IM", 0.15)] {
        map.insert(code.to_string(), MarginRate {
            instrument_id: code.to_string(),
            exchange_id: "CFFEX".to_string(),
            product_id: code.to_string(),
            long_margin_ratio_by_money: *ratio,
            long_margin_ratio_by_volume: 0.0,
            short_margin_ratio_by_money: *ratio,
            short_margin_ratio_by_volume: 0.0,
        });
    }
    // 国债期货
    for (code, ratio) in &[("T", 0.02), ("TF", 0.012), ("TL", 0.035), ("TS", 0.005)] {
        map.insert(code.to_string(), MarginRate {
            instrument_id: code.to_string(),
            exchange_id: "CFFEX".to_string(),
            product_id: code.to_string(),
            long_margin_ratio_by_money: *ratio,
            long_margin_ratio_by_volume: 0.0,
            short_margin_ratio_by_money: *ratio,
            short_margin_ratio_by_volume: 0.0,
        });
    }
    // 商品期货
    for (code, exchange, ratio) in &[
        ("cu", "SHFE", 0.09),
        ("au", "SHFE", 0.10),
        ("i", "DCE", 0.11),
        ("sc", "INE", 0.12),
    ] {
        map.insert(code.to_string(), MarginRate {
            instrument_id: code.to_string(),
            exchange_id: exchange.to_string(),
            product_id: code.to_string(),
            long_margin_ratio_by_money: *ratio,
            long_margin_ratio_by_volume: 0.0,
            short_margin_ratio_by_money: *ratio,
            short_margin_ratio_by_volume: 0.0,
        });
    }
}

// ==================== Phase 12: 密码管理 ====================

/// 修改密码
pub async fn change_password(
    req: web::Json<ChangePasswordRequest>,
    _account_mgr: web::Data<Arc<AccountManager>>,
) -> HttpResponse {
    let account_id = &req.account_id;

    // 验证旧密码
    if let Some(passwords) = ACCOUNT_PASSWORDS.get(account_id) {
        let current_password = match req.password_type {
            PasswordType::Trading => &passwords.0,
            PasswordType::Fund => &passwords.1,
        };

        if current_password != &req.old_password {
            // 记录审计日志
            log_audit(
                account_id.clone(),
                account_id.clone(),
                AuditLogType::PasswordChange,
                "修改密码".to_string(),
                format!("密码类型: {:?}, 验证失败", req.password_type),
                None,
                AuditResult::Failed,
            );
            return HttpResponse::BadRequest().json(ApiResponse::<()>::error(4001, "原密码错误".to_string()));
        }
    } else {
        // 新账户，使用默认密码 "123456"
        if req.old_password != "123456" {
            return HttpResponse::BadRequest().json(ApiResponse::<()>::error(4001, "原密码错误".to_string()));
        }
    }

    // 更新密码
    let mut entry = ACCOUNT_PASSWORDS.entry(account_id.clone())
        .or_insert(("123456".to_string(), "123456".to_string()));

    match req.password_type {
        PasswordType::Trading => entry.0 = req.new_password.clone(),
        PasswordType::Fund => entry.1 = req.new_password.clone(),
    }

    // 记录审计日志
    log_audit(
        account_id.clone(),
        account_id.clone(),
        AuditLogType::PasswordChange,
        "修改密码".to_string(),
        format!("密码类型: {:?}", req.password_type),
        None,
        AuditResult::Success,
    );

    HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
        "message": "密码修改成功"
    })))
}

/// 重置密码（管理员操作）
pub async fn reset_password(
    req: web::Json<ResetPasswordRequest>,
) -> HttpResponse {
    if !verify_admin_token(&req.admin_token) {
        return HttpResponse::Unauthorized().json(ApiResponse::<()>::error(4010, "管理员认证失败".to_string()));
    }

    let account_id = &req.account_id;
    let mut entry = ACCOUNT_PASSWORDS.entry(account_id.clone())
        .or_insert(("123456".to_string(), "123456".to_string()));

    match req.password_type {
        PasswordType::Trading => entry.0 = req.new_password.clone(),
        PasswordType::Fund => entry.1 = req.new_password.clone(),
    }

    // 记录审计日志
    log_audit(
        account_id.clone(),
        "admin".to_string(),
        AuditLogType::PasswordChange,
        "管理员重置密码".to_string(),
        format!("密码类型: {:?}", req.password_type),
        None,
        AuditResult::Success,
    );

    HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
        "message": "密码重置成功"
    })))
}

// ==================== Phase 12: 手续费查询 ====================

/// 查询手续费率
pub async fn get_commission_rates(
    query: web::Query<CommissionQueryRequest>,
) -> HttpResponse {
    let mut rates = Vec::new();

    if let Some(instrument_id) = &query.instrument_id {
        // 提取品种代码（去除合约月份）
        let product_id = extract_product_id(instrument_id);
        if let Some(rate) = COMMISSION_RATES.get(&product_id) {
            let mut rate_clone = rate.clone();
            rate_clone.instrument_id = instrument_id.clone();
            rates.push(rate_clone);
        }
    } else {
        // 返回全部手续费率
        for entry in COMMISSION_RATES.iter() {
            rates.push(entry.value().clone());
        }
    }

    HttpResponse::Ok().json(ApiResponse::success(rates))
}

/// 查询手续费统计
pub async fn get_commission_statistics(
    path: web::Path<String>,
    account_mgr: web::Data<Arc<AccountManager>>,
) -> HttpResponse {
    let account_id = path.into_inner();

    // 从账户管理器获取账户信息（包含手续费）
    match account_mgr.get_account(&account_id) {
        Ok(account) => {
            let account_read = account.read();
            let commission = account_read.accounts.commission;

            // 模拟按合约统计（实际应从交易记录中计算）
            let statistics = CommissionStatistics {
                account_id: account_id.clone(),
                total_commission: commission,
                today_commission: commission * 0.1,  // 模拟今日手续费
                commission_by_instrument: vec![],
            };

            HttpResponse::Ok().json(ApiResponse::success(statistics))
        }
        Err(_) => {
            HttpResponse::NotFound().json(ApiResponse::<()>::error(4040, "账户不存在".to_string()))
        }
    }
}

// ==================== Phase 12: 保证金率管理 ====================

/// 查询保证金率
pub async fn get_margin_rates(
    query: web::Query<MarginRateQueryRequest>,
) -> HttpResponse {
    let mut rates = Vec::new();

    if let Some(instrument_id) = &query.instrument_id {
        let product_id = extract_product_id(instrument_id);
        if let Some(rate) = MARGIN_RATES.get(&product_id) {
            let mut rate_clone = rate.clone();
            rate_clone.instrument_id = instrument_id.clone();
            rates.push(rate_clone);
        }
    } else {
        for entry in MARGIN_RATES.iter() {
            rates.push(entry.value().clone());
        }
    }

    HttpResponse::Ok().json(ApiResponse::success(rates))
}

/// 查询保证金汇总
pub async fn get_margin_summary(
    path: web::Path<String>,
    account_mgr: web::Data<Arc<AccountManager>>,
) -> HttpResponse {
    let account_id = path.into_inner();

    match account_mgr.get_account(&account_id) {
        Ok(account) => {
            // @yutiansut @quantaxis: 使用写锁以调用 qars 的 volume_long()/volume_short()
            let mut account_write = account.write();

            // ⚠️ `acc.accounts.<派生字段>` 盘中**永远是开户时的初始值** ——
            // qars 只在 `settle()` 里整体重建 self.accounts(account.rs:574),
            // 成交/撤单都不回写。实测 NOISE_SELL 成交 6.8 万笔后
            // accounts.balance 仍是 1,500,000,000(init_cash)、risk_ratio 仍是 0.0。
            // 本处已持写锁(为 get_margin 取的),改用动态 getter 零额外成本。
            // @yutiansut @quantaxis
            let balance = account_write.get_balance();
            let frozen_margin = account_write.get_frozen_margin();
            let risk_ratio = account_write.safe_risk_ratio();
            let available = account_write.money;

            // 收集持仓数据
            let mut position_details = Vec::new();
            let mut total_margin = 0.0;

            for (instrument_id, pos) in account_write.hold.iter_mut() {
                let product_id = extract_product_id(instrument_id);
                let margin_rate = MARGIN_RATES.get(&product_id)
                    .map(|r| r.long_margin_ratio_by_money)
                    .unwrap_or(0.1);

                let volume_long = pos.volume_long();
                let volume_short = pos.volume_short();

                total_margin += pos.margin_long + pos.margin_short;

                // ✨ 跳过零手空壳 —— 与 handlers.rs 的两个持仓端点保持一致
                // @yutiansut @quantaxis
                //
                // qars 的 `retain` 清理只活在 `settle()` 里,而本系统**从不结算**
                // (见 DECISIONS-PENDING「从不结算」一节),所以 `hold` 里会留下
                // 已平干净的合约。实测线上某账户本端点返回 5 条,其中 1 条是
                // 探针留下的 `NOSUCH0`(多 0 空 0)。
                // `handlers.rs:967 / :1027` 早已过滤,这里**漏了**。
                if volume_long == 0.0 && volume_short == 0.0 {
                    continue;
                }

                position_details.push(PositionMarginDetail {
                    instrument_id: instrument_id.clone(),
                    volume_long,
                    volume_short,
                    margin_long: pos.margin_long,
                    margin_short: pos.margin_short,
                    margin_rate_long: margin_rate,
                    margin_rate_short: margin_rate,
                    last_price: pos.lastest_price,
                    // ✨ 乘数取自持仓自带的 preset,不再硬编码 1.0
                    //
                    // 原值是 `1.0` 且注释写着「实际应从合约信息中获取」。
                    // IF 合约乘数是 300 —— 前端拿 1.0 去算名义金额会小 300 倍。
                    // `QA_Position` 自带 `preset.unit_table`(qars
                    // `account.rs:342` 自己算保证金时用的就是它),无需改本函数签名。
                    multiplier: pos.preset.unit_table as f64,
                });
            }

            let summary = MarginSummary {
                account_id: account_id.clone(),
                total_margin,
                frozen_margin,
                available_margin: available,
                margin_ratio: if balance > 0.0 { total_margin / balance } else { 0.0 },
                risk_degree: risk_ratio,
                positions: position_details,
            };

            HttpResponse::Ok().json(ApiResponse::success(summary))
        }
        Err(_) => {
            HttpResponse::NotFound().json(ApiResponse::<()>::error(4040, "账户不存在".to_string()))
        }
    }
}

// ==================== Phase 13: 账户冻结 ====================

/// 冻结账户
pub async fn freeze_account(
    req: web::Json<FreezeAccountRequest>,
    account_mgr: web::Data<Arc<AccountManager>>,
) -> HttpResponse {
    if !verify_admin_token(&req.admin_token) {
        return HttpResponse::Unauthorized().json(ApiResponse::<()>::error(4010, "管理员认证失败".to_string()));
    }

    let account_id = &req.account_id;

    // ✨ 冻结前必须确认账户存在。@yutiansut @quantaxis
    //
    // 原来没有这一步:`ACCOUNT_STATUS` 是个独立的 DashMap,冻结只是往里 insert 一条,
    // 完全不问这个 account_id 是不是真账户。实测(2026-09-06)对一个凭空编的
    // `NOSUCH_PROBE_ZZ` 调用本接口,返回 `success:true` 且状态变 FROZEN,
    // 审计日志也照记一条 —— 一条指向不存在账户的冻结记录。
    //
    // 危害:① 打错一个字符就会产生一条永远不会生效、也永远不会被发现的「冻结」,
    // 而操作员以为已经冻住了;② 审计日志被这类幽灵记录污染
    // (实测生产至今仅有的 2 条审计记录全是这么来的)。
    if account_mgr.get_account(account_id).is_err() {
        return HttpResponse::NotFound().json(ApiResponse::<()>::error(
            404,
            format!("账户不存在:{}", account_id),
        ));
    }
    let now = current_timestamp();

    // 计算 can_trade 和 can_withdraw @yutiansut @quantaxis
    let (can_trade_val, can_withdraw_val) = match &req.freeze_type {
        FreezeType::TradingOnly => (false, true),
        FreezeType::WithdrawOnly => (true, false),
        FreezeType::Full => (false, false),
    };

    let status_info = AccountStatusInfo {
        account_id: account_id.clone(),
        status: AccountStatus::Frozen,
        freeze_type: Some(req.freeze_type.clone()),
        freeze_reason: Some(req.reason.clone()),
        frozen_at: Some(now),
        frozen_by: Some("admin".to_string()),
        can_trade: can_trade_val,
        can_withdraw: can_withdraw_val,
    };

    ACCOUNT_STATUS.insert(account_id.clone(), status_info.clone());

    // 记录审计日志
    log_audit(
        account_id.clone(),
        "admin".to_string(),
        AuditLogType::AccountFreeze,
        "冻结账户".to_string(),
        format!("冻结类型: {:?}, 原因: {}", req.freeze_type, req.reason),
        None,
        AuditResult::Success,
    );

    HttpResponse::Ok().json(ApiResponse::success(status_info))
}

/// 解冻账户
pub async fn unfreeze_account(
    req: web::Json<UnfreezeAccountRequest>,
    account_mgr: web::Data<Arc<AccountManager>>,
) -> HttpResponse {
    if !verify_admin_token(&req.admin_token) {
        return HttpResponse::Unauthorized().json(ApiResponse::<()>::error(4010, "管理员认证失败".to_string()));
    }

    let account_id = &req.account_id;

    // 同 freeze_account:解冻前也要确认账户存在,否则会给幽灵账户记一条解冻审计。
    // @yutiansut @quantaxis
    if account_mgr.get_account(account_id).is_err() {
        return HttpResponse::NotFound().json(ApiResponse::<()>::error(
            404,
            format!("账户不存在:{}", account_id),
        ));
    }

    let status_info = AccountStatusInfo {
        account_id: account_id.clone(),
        status: AccountStatus::Active,
        freeze_type: None,
        freeze_reason: None,
        frozen_at: None,
        frozen_by: None,
        can_trade: true,     // Active状态可以交易
        can_withdraw: true,  // Active状态可以出金
    };

    ACCOUNT_STATUS.insert(account_id.clone(), status_info.clone());

    // 记录审计日志
    log_audit(
        account_id.clone(),
        "admin".to_string(),
        AuditLogType::AccountUnfreeze,
        "解冻账户".to_string(),
        format!("原因: {}", req.reason),
        None,
        AuditResult::Success,
    );

    HttpResponse::Ok().json(ApiResponse::success(status_info))
}

/// 查询账户状态
pub async fn get_account_status(
    path: web::Path<String>,
) -> HttpResponse {
    let account_id = path.into_inner();

    let status_info = ACCOUNT_STATUS.get(&account_id)
        .map(|s| s.clone())
        .unwrap_or(AccountStatusInfo {
            account_id: account_id.clone(),
            status: AccountStatus::Active,
            freeze_type: None,
            freeze_reason: None,
            frozen_at: None,
            frozen_by: None,
            can_trade: true,     // 默认Active状态可交易
            can_withdraw: true,  // 默认Active状态可出金
        });

    HttpResponse::Ok().json(ApiResponse::success(status_info))
}

/// 检查账户是否可以交易
pub fn can_trade(account_id: &str) -> bool {
    if let Some(status) = ACCOUNT_STATUS.get(account_id) {
        match status.status {
            AccountStatus::Active => true,
            AccountStatus::Frozen => {
                // 检查冻结类型
                match &status.freeze_type {
                    Some(FreezeType::WithdrawOnly) => true,
                    _ => false,
                }
            }
            AccountStatus::Suspended => false,
            AccountStatus::Closed => false,
        }
    } else {
        true
    }
}

/// 检查账户是否可以出金
pub fn can_withdraw(account_id: &str) -> bool {
    if let Some(status) = ACCOUNT_STATUS.get(account_id) {
        match status.status {
            AccountStatus::Active => true,
            AccountStatus::Frozen => {
                match &status.freeze_type {
                    Some(FreezeType::TradingOnly) => true,
                    _ => false,
                }
            }
            AccountStatus::Suspended => false,
            AccountStatus::Closed => false,
        }
    } else {
        true
    }
}

// ==================== Phase 13: 审计日志 ====================

/// 记录审计日志
pub fn log_audit(
    account_id: String,
    user_id: String,
    log_type: AuditLogType,
    action: String,
    details: String,
    ip_address: Option<String>,
    result: AuditResult,
) {
    let id = Uuid::new_v4().to_string();
    let entry = AuditLogEntry {
        id: id.clone(),
        timestamp: current_timestamp(),
        account_id,
        user_id,
        log_type,
        action,
        details,
        ip_address,
        user_agent: None,
        result,
    };
    AUDIT_LOGS.insert(id, entry);
}

/// 查询审计日志
pub async fn query_audit_logs(
    query: web::Query<AuditLogQueryRequest>,
) -> HttpResponse {
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(20).min(100);

    let mut logs: Vec<AuditLogEntry> = AUDIT_LOGS.iter()
        .map(|e| e.value().clone())
        .filter(|log| {
            // 按账户过滤
            if let Some(ref account_id) = query.account_id {
                if &log.account_id != account_id {
                    return false;
                }
            }
            // 按用户过滤
            if let Some(ref user_id) = query.user_id {
                if &log.user_id != user_id {
                    return false;
                }
            }
            // 按类型过滤
            if let Some(ref log_type) = query.log_type {
                if &log.log_type != log_type {
                    return false;
                }
            }
            // 按时间范围过滤
            if let Some(start_time) = query.start_time {
                if log.timestamp < start_time {
                    return false;
                }
            }
            if let Some(end_time) = query.end_time {
                if log.timestamp > end_time {
                    return false;
                }
            }
            true
        })
        .collect();

    // 按时间倒序
    logs.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    let total = logs.len() as u64;
    let start = ((page - 1) * page_size) as usize;
    let end = (start + page_size as usize).min(logs.len());
    let paged_logs = if start < logs.len() {
        logs[start..end].to_vec()
    } else {
        vec![]
    };

    let response = AuditLogResponse {
        total,
        page,
        page_size,
        logs: paged_logs,
    };

    HttpResponse::Ok().json(ApiResponse::success(response))
}

/// 获取审计日志详情
pub async fn get_audit_log(
    path: web::Path<String>,
) -> HttpResponse {
    let log_id = path.into_inner();

    if let Some(log) = AUDIT_LOGS.get(&log_id) {
        HttpResponse::Ok().json(ApiResponse::success(log.clone()))
    } else {
        HttpResponse::NotFound().json(ApiResponse::<()>::error(4040, "日志不存在".to_string()))
    }
}

// ==================== Phase 13: 系统公告 ====================

/// 创建公告
/// 创建后会通过 WebSocket 实时推送到所有已连接的用户
/// @yutiansut @quantaxis
pub async fn create_announcement(
    req: web::Json<CreateAnnouncementRequest>,
) -> HttpResponse {
    if !verify_admin_token(&req.admin_token) {
        return HttpResponse::Unauthorized().json(ApiResponse::<()>::error(4010, "管理员认证失败".to_string()));
    }

    let id = Uuid::new_v4().to_string();
    let now = current_timestamp();

    let announcement = Announcement {
        id: id.clone(),
        title: req.title.clone(),
        content: req.content.clone(),
        announcement_type: req.announcement_type.clone(),
        priority: req.priority.clone(),
        // 用请求里的生效时间;不传才落到当前时间 @yutiansut @quantaxis
        publish_time: req.publish_time.unwrap_or(now),
        expire_time: req.expire_time,
        is_active: true,
        author: "admin".to_string(),
        attachments: vec![],
    };

    ANNOUNCEMENTS.insert(id.clone(), announcement.clone());

    // ✨ 通过 WebSocket 广播公告到所有已连接用户 @yutiansut @quantaxis
    if let Some(snapshot_mgr) = get_global_snapshot_manager() {
        let notify_patch = serde_json::json!({
            "notify": {
                format!("announcement_{}", id): {
                    "type": "ANNOUNCEMENT",
                    "level": match announcement.priority {
                        AnnouncementPriority::Urgent => "ERROR",  // 紧急公告用红色
                        AnnouncementPriority::High => "WARNING",  // 高优先级用黄色
                        _ => "INFO"
                    },
                    "code": 2000,
                    "announcement_id": id,
                    "title": announcement.title,
                    "content": announcement.content,
                    "announcement_type": format!("{:?}", announcement.announcement_type),
                    "priority": format!("{:?}", announcement.priority),
                    "publish_time": announcement.publish_time
                }
            }
        });
        let count = snapshot_mgr.broadcast_patch(notify_patch).await;
        log::info!("📢 Announcement '{}' broadcasted to {} connected users", announcement.title, count);
    } else {
        log::warn!("⚠️ SnapshotManager not available, announcement not broadcasted via WebSocket");
    }

    HttpResponse::Ok().json(ApiResponse::success(announcement))
}

/// 查询公告列表
pub async fn query_announcements(
    query: web::Query<AnnouncementQueryRequest>,
) -> HttpResponse {
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(20).min(100);
    let now = current_timestamp();

    let mut announcements: Vec<Announcement> = ANNOUNCEMENTS.iter()
        .map(|e| e.value().clone())
        .filter(|a| {
            // 按类型过滤
            if let Some(ref announcement_type) = query.announcement_type {
                if &a.announcement_type != announcement_type {
                    return false;
                }
            }
            // 按是否有效过滤
            if query.only_active.unwrap_or(true) {
                if !a.is_active {
                    return false;
                }
                // 检查是否过期
                if let Some(expire_time) = a.expire_time {
                    if now > expire_time {
                        return false;
                    }
                }
            }
            true
        })
        .collect();

    // 按发布时间倒序，优先级高的排前面
    announcements.sort_by(|a, b| {
        let priority_order = |p: &AnnouncementPriority| match p {
            AnnouncementPriority::Urgent => 0,
            AnnouncementPriority::High => 1,
            AnnouncementPriority::Normal => 2,
            AnnouncementPriority::Low => 3,
        };
        let pa = priority_order(&a.priority);
        let pb = priority_order(&b.priority);
        if pa != pb {
            pa.cmp(&pb)
        } else {
            b.publish_time.cmp(&a.publish_time)
        }
    });

    let total = announcements.len() as u64;
    let start = ((page - 1) * page_size) as usize;
    let end = (start + page_size as usize).min(announcements.len());
    let paged_announcements = if start < announcements.len() {
        announcements[start..end].to_vec()
    } else {
        vec![]
    };

    let response = AnnouncementListResponse {
        total,
        page,
        page_size,
        announcements: paged_announcements,
    };

    HttpResponse::Ok().json(ApiResponse::success(response))
}

/// 获取公告详情
pub async fn get_announcement(
    path: web::Path<String>,
) -> HttpResponse {
    let announcement_id = path.into_inner();

    if let Some(announcement) = ANNOUNCEMENTS.get(&announcement_id) {
        HttpResponse::Ok().json(ApiResponse::success(announcement.clone()))
    } else {
        HttpResponse::NotFound().json(ApiResponse::<()>::error(4040, "公告不存在".to_string()))
    }
}

/// 删除/停用公告
pub async fn delete_announcement(
    path: web::Path<String>,
    query: web::Query<std::collections::HashMap<String, String>>,
) -> HttpResponse {
    let admin_token = query.get("admin_token").map(|s| s.as_str()).unwrap_or("");
    if !verify_admin_token(admin_token) {
        return HttpResponse::Unauthorized().json(ApiResponse::<()>::error(4010, "管理员认证失败".to_string()));
    }

    let announcement_id = path.into_inner();

    if let Some(mut announcement) = ANNOUNCEMENTS.get_mut(&announcement_id) {
        announcement.is_active = false;
        HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
            "message": "公告已停用"
        })))
    } else {
        HttpResponse::NotFound().json(ApiResponse::<()>::error(4040, "公告不存在".to_string()))
    }
}

// ==================== 辅助函数 ====================

/// 从合约代码中提取品种代码
fn extract_product_id(instrument_id: &str) -> String {
    // 提取字母部分作为品种代码
    // 例如: IF2312 -> IF, cu2312 -> cu, au2512P968 -> au
    let mut product = String::new();
    for c in instrument_id.chars() {
        if c.is_alphabetic() {
            product.push(c);
        } else {
            break;
        }
    }
    product
}
