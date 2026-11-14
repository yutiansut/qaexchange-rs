//! 账户管理中心
//!
//! 负责账户的开户、销户、查询等管理功能

use crate::core::account_ext::{AccountType, OpenAccountRequest};
use crate::core::{Account, QA_Account, QIFI};
use crate::notification::message::{
    AccountOpenNotify, Notification, NotificationPayload, NotificationType,
};
use crate::notification::NotificationBroker;
use crate::user::UserManager;
use crate::ExchangeError;
use chrono::Local;
use dashmap::DashMap;
use parking_lot::RwLock;
use std::path::Path;
use std::sync::Arc;

/// 账户元数据
#[derive(Debug, Clone)]
struct AccountMetadata {
    /// 所属用户ID
    user_id: String,

    /// 账户名称
    account_name: String,

    /// 账户类型
    account_type: AccountType,

    /// 创建时间
    created_at: i64,
}

/// 账户管理器
pub struct AccountManager {
    /// 账户映射 (account_id -> QA_Account)
    accounts: DashMap<String, Arc<RwLock<QA_Account>>>,

    /// 账户元数据映射 (account_id -> AccountMetadata)
    metadata: DashMap<String, AccountMetadata>,

    /// 用户账户索引 (user_id -> [account_ids])
    user_accounts: DashMap<String, Vec<String>>,

    /// 通知中心（用于WAL恢复）
    notification_broker: Option<Arc<NotificationBroker>>,

    /// 用户管理器（用于验证用户和自动绑定）
    user_manager: Option<Arc<UserManager>>,

    /// 快照落盘互斥:同一时刻只允许一次 `save_snapshots`。@yutiansut @quantaxis
    ///
    /// 2026-11-08 06:39 停机实测:60 秒周期快照(spawn_blocking)还没写完,停机最终快照就开始了,
    /// 两次落盘写**同一批** `<account>.json.tmp` → 一方 rename 走了另一方的 tmp →
    /// 「Rename snapshot failed: No such file」,最终快照整批中止。更危险的是另一种交错:
    /// **先开始的旧快照后完成**,用旧状态覆盖刚写好的新快照。串行化后,最终快照会等
    /// 在途的周期快照写完,再写出最新状态。
    save_lock: parking_lot::Mutex<()>,
}

/// 执行出金并**核对确实扣款成功** @yutiansut @quantaxis
///
/// qars 的 `QA_Account::withdraw`(qars2 account.rs:541)用的是**严格大于**:
/// ```ignore
/// if self.money > amount { self.money -= amount; ... } else { warn!("余额不足") }
/// ```
/// 而且**没有返回值**。本仓库三个调用点的前置校验都是 `if money < amount { 拒绝 }`,
/// 允许相等通过。于是「出金金额 == 全部可用资金」时:
///   校验放行 → `withdraw()` 空转 → 上层照常返回「成功」并写一条
///   `status: completed` 的资金流水,**余额一分不少**。
///
/// 实测(2026-09-04):balance 1444.0 出金 1444.0 → HTTP 200「转账成功」,
/// 事后余额仍是 1444.0;`/api/management/withdraw` 更是记下
/// `balance_before=1444, balance_after=1444, status=completed`。
/// 三个入口(银期转账 / 用户出金 / 管理端出金)同时中招,
/// 正因为 `withdraw()` 没有任何返回值可判 —— 这是「无返回值的失败」的典型代价。
///
/// 根治要改 qars 的 `>` → `>=`(共享库,需另行决策)。本函数在那之后依然正确:
/// 届时全额出金会真正成功,余额核对自然通过。
pub fn withdraw_checked(acc: &mut QA_Account, amount: f64) -> Result<(), ExchangeError> {
    let before = acc.money;
    acc.withdraw(amount);
    let expected = before - amount;
    if (acc.money - expected).abs() > 1e-6 {
        return Err(ExchangeError::InsufficientBalance(format!(
            "出金未生效: 请求 {:.4},余额 {:.4} → {:.4}(应为 {:.4})。\
             qars withdraw 要求 money > amount(严格大于),全额出金被静默忽略。",
            amount, before, acc.money, expected
        )));
    }
    Ok(())
}

/// 把 qars 的 QIFI 投影修正为**符合 QIFI 规范**的形式 @yutiansut @quantaxis
///
/// qars 的 `get_qifi_slice`(qars2 account.rs:417)直接输出原始字段
/// `self.accounts.static_balance`,而 `deposit()`/`withdraw()`(:536-548)
/// **只更新 `money` 和 `accounts.deposit`,从不更新 `accounts.static_balance`**。
/// 于是任何发生过出入金的账户,发出去的 QIFI 里:
///   · `balance` 是对的(`get_balance()` 内部算了 `+ deposit - withdraw`)
///   · `static_balance` 却停在 init_cash
/// 两者不再满足规范的恒等式,下游按
/// `balance = static_balance + close_profit + position_profit - commission + premium`
/// 反推就会错一个净入金额。
///
/// 实测(2026-09-04):`pre_balance=10000, deposit=-3900, withdraw=0,
/// static_balance=10000, balance=6100` —— 规范要求 static_balance = 6100。
/// 守恒校验里 4 个账户的偏差恰好等于各自的净入出金。
///
/// ⚠️ qars 内部对这两个量的语义与规范不同:`settle()`(:574-596)把
/// `pre_balance` 设为含浮动盈亏的结算余额、`static_balance` 设为不含浮动盈亏的量,
/// 二者**故意**不等(account.rs:68 的注释)。本函数按**规范**输出,
/// 只影响对外投影,不改 qars 内部状态。当前系统尚未跑过结算
/// (`/api/admin/settlement/history` 为空),两者相等,修正无歧义;
/// 若将来启用日终结算,需要重新确认这里该取哪个语义。
fn conform_qifi_static_balance(qifi: &mut crate::QIFI) {
    conform_qifi_account(&mut qifi.accounts);
}

/// `conform_qifi_static_balance` 的账户部分 —— DIFF `trade.<account>.accounts.CNY` 投影也用它,
/// 保证 DIFF 与 HTTP QIFI 两条只读投影口径一致。⚠️ 同样**只能用于只读投影**。@yutiansut @quantaxis
pub(crate) fn conform_qifi_account(accounts: &mut qars::qaprotocol::qifi::account::Account) {
    // ⚠️⚠️ 只能用于**只读投影**(HTTP 响应)。**绝不可用在任何会被持久化的路径上**。
    //      @yutiansut @quantaxis
    //
    // 事故复盘(2026-09-05):本函数原先也被 `save_snapshots` 调用,造成一个闭合的污染回路 ——
    //   qars 内部 `static_balance` 的语义是「初始资金」:`account.rs:145` 构造时设为
    //   `init_cash` 后**永不更新**(`deposit()` 只动 `money` 与 `accounts.deposit`),
    //   所以 qars 的 `get_balance()`(:525)**必须**再加一次 `deposit - withdraw` 才对。
    //   本函数把对外投影改成 QIFI 语义(含 deposit)→ 被 save_snapshots 写进
    //   `snapshots/*.json` → 重启时 `restore_account_from_qifi` → `new_from_qifi`
    //   (qars :224)把它读回 qars 内部 → `get_balance()` 又加一次 deposit。
    //   实测后果:59 个账户里 9 个中招,虚增权益 ¥1,994,454,806(占全所 balance 9.76%),
    //   风险率同步被低估(MM_ALPHA_BID 显示 0.76% 而非 1.14%)。
    //   不累积(每次快照都重写成同一个值),但一次污染即长期存在。
    //
    // 这里同时校正 `balance`:只改 `static_balance` 会让响应自相矛盾 ——
    // 律1(`static = pre + deposit - withdraw`)成立而律2
    // (`balance = static + close_profit + position_profit - commission + premium`)不成立。
    // 校正后两条同时成立,且与 QIFI 规范样本 55550081 的算法一致。
    //
    // 注:qars 的 `get_balance()` 用 `float_profit`(对开仓价)且不含 `premium`,
    // 规范用 `position_profit`(对持仓价)且含 `premium`。当前系统两者相等、premium 恒 0,
    // 所以此处改动不产生数值差异;将来启用日终结算后会分叉,届时以本函数为准。
    accounts.static_balance =
        accounts.pre_balance + accounts.deposit - accounts.withdraw;
    // 与 `AccountExt::qifi_balance()` 同一公式(那里从 QA_Account 直接算,
    // 这里从已投影的 QIFI 字段算,两条路径必须一致)。@yutiansut @quantaxis
    accounts.balance = accounts.static_balance
        + accounts.close_profit
        + accounts.position_profit
        - accounts.commission
        + accounts.premium;

    // ⚠️ `risk_ratio` 必须跟着 `balance` 一起重算,否则同一份响应自相矛盾。
    //    @yutiansut @quantaxis
    //
    // qars 的 `get_riskratio()` 用的是它**内部**的 balance;上面刚把 balance 换成
    // 规范口径,不同步 risk_ratio 就会出现 `risk_ratio != margin / balance`。
    // 实测(受污染的 9 个账户):MM_ALPHA_BID 报 0.00750,而 margin/修正balance
    // = 0.01136 —— **风险被低估 34%**;fe_verify 低估 50%。低估是危险方向。
    //
    // 非有限值一律落到 0.0(与 `AccountExt::safe_risk_ratio` 同语义:
    // balance<=0 意味着没有可用权益作分母,此处不臆造风险度)。
    accounts.risk_ratio = if accounts.balance > 0.0 {
        let r = accounts.margin / accounts.balance;
        if r.is_finite() { r } else { 0.0 }
    } else {
        0.0
    };

    // ⚠️ 归一化负零 —— `-0.0` 会原样序列化进 JSON,前端渲染出 "+-0.00"。
    //    @yutiansut @quantaxis
    //
    // 「冻结 +X / 成交 -X」精确抵消后会留下 `-0.0`(不是残差,是真的等于 0,
    // 只是带了符号位)。实测线上:frozen_margin 50/59 个账户、
    // position_profit 与 float_profit 各 31 个账户是 `-0.0`。
    // 危害在前端:JS 里 `-0 >= 0` 为 **true**,于是
    // `admin/risk.vue:123` 的 `{{ v >= 0 ? '+' : '' }}{{ v.toLocaleString(...) }}`
    // 先输出 `+` 再输出 `-0.00` ⇒ 渲染成 **"+-0.00"**。
    // (`toFixed` 不带符号,`toLocaleString` 带 —— 仓内两种格式化都在用。)
    //
    // `x == 0.0` 对 +0.0 与 -0.0 都为真,所以这是纯恒等归一,不改变任何数值。
    let z = |x: f64| if x == 0.0 { 0.0 } else { x };
    let a = &mut *accounts;
    a.balance = z(a.balance);
    a.available = z(a.available);
    a.margin = z(a.margin);
    a.frozen_margin = z(a.frozen_margin);
    a.frozen_commission = z(a.frozen_commission);
    a.frozen_premium = z(a.frozen_premium);
    a.position_profit = z(a.position_profit);
    a.float_profit = z(a.float_profit);
    a.close_profit = z(a.close_profit);
    a.commission = z(a.commission);
    a.premium = z(a.premium);
    a.static_balance = z(a.static_balance);
    a.pre_balance = z(a.pre_balance);
    a.deposit = z(a.deposit);
    a.withdraw = z(a.withdraw);
    a.risk_ratio = z(a.risk_ratio);
}

impl AccountManager {
    pub fn new() -> Self {
        Self {
            accounts: DashMap::new(),
            metadata: DashMap::new(),
            user_accounts: DashMap::new(),
            notification_broker: None,
            user_manager: None,
            save_lock: parking_lot::Mutex::new(()),
        }
    }

    /// 创建带通知功能的账户管理器（用于WAL恢复）
    pub fn with_notification_broker(broker: Arc<NotificationBroker>) -> Self {
        Self {
            accounts: DashMap::new(),
            metadata: DashMap::new(),
            user_accounts: DashMap::new(),
            notification_broker: Some(broker),
            user_manager: None,
            save_lock: parking_lot::Mutex::new(()),
        }
    }

    /// 设置通知中心
    pub fn set_notification_broker(&mut self, broker: Arc<NotificationBroker>) {
        self.notification_broker = Some(broker);
    }

    /// 获取通知中心
    pub fn notification_broker(&self) -> Option<&Arc<NotificationBroker>> {
        self.notification_broker.as_ref()
    }

    /// 设置用户管理器
    pub fn set_user_manager(&mut self, user_manager: Arc<UserManager>) {
        self.user_manager = Some(user_manager);
    }

    /// 开户
    ///
    /// 为指定用户创建一个新的交易账户。
    ///
    /// # 参数
    /// - `req`: 开户请求，包含用户ID、账户名称、初始资金等信息
    ///
    /// # 返回
    /// - `Ok(account_id)`: 成功创建的账户ID
    /// - `Err(...)`: 创建失败的错误信息
    pub fn open_account(&self, req: OpenAccountRequest) -> Result<String, ExchangeError> {
        // ✨ 安全:限制字符串字段长度。account_name 会进每 60s 的快照,未鉴权开户传巨名(如 100MB)
        //    可导致快照膨胀 → 磁盘/内存 DoS。256/128 字节远超合法用途(中文名 20 字 ≈ 60 字节)。@yutiansut @quantaxis
        const MAX_NAME_LEN: usize = 256;
        const MAX_ID_LEN: usize = 128;
        if req.account_name.len() > MAX_NAME_LEN {
            return Err(ExchangeError::AccountError(format!(
                "account_name 过长: {} 字节(上限 {})",
                req.account_name.len(),
                MAX_NAME_LEN
            )));
        }
        if req.user_id.len() > MAX_ID_LEN {
            return Err(ExchangeError::AccountError(format!(
                "user_id 过长: {} 字节(上限 {})",
                req.user_id.len(),
                MAX_ID_LEN
            )));
        }
        // 验证用户是否存在（如果设置了UserManager）
        if let Some(user_mgr) = &self.user_manager {
            user_mgr.get_user(&req.user_id)?;
        }

        // 生成或使用提供的账户ID
        let account_id = req.account_id.unwrap_or_else(|| {
            format!("ACC_{}", uuid::Uuid::new_v4().to_string().replace("-", ""))
        });

        // 检查账户是否已存在
        if self.accounts.contains_key(&account_id) {
            return Err(ExchangeError::AccountError(format!(
                "Account already exists: {}",
                account_id
            )));
        }

        // 创建账户 (复用 QA_Account)
        // portfolio_cookie 使用 user_id，建立 User -> Account 关系
        // user_cookie 使用 account_name，对应 QIFI 的 investor_name
        // ✨ environment 设置为 "sim"：send_order 只冻结资金，由 receive_deal_sim 更新持仓
        // 注意：不能用 "real"，否则 send_order 会立即调用 receive_deal_real 导致持仓双倍计算
        // @yutiansut @quantaxis
        let account = QA_Account::new(
            &account_id,       // account_cookie (账户唯一标识)
            &req.user_id,      // portfolio_cookie (用户ID - 建立User关联)
            &req.account_name, // user_cookie (账户名称 -> QIFI investor_name)
            req.init_cash,     // init_cash
            false,             // auto_reload
            "sim",             // environment (sim模式：send_order不立即成交，需等receive_deal_sim)
        );

        // 存储账户
        self.accounts
            .insert(account_id.clone(), Arc::new(RwLock::new(account)));

        // 存储元数据
        let metadata = AccountMetadata {
            user_id: req.user_id.clone(),
            account_name: req.account_name.clone(),
            account_type: req.account_type,
            created_at: chrono::Utc::now().timestamp(),
        };
        self.metadata.insert(account_id.clone(), metadata);

        // 更新用户账户索引
        self.user_accounts
            .entry(req.user_id.clone())
            .or_default()
            .push(account_id.clone());

        log::info!(
            "Account opened: {} for user {} (type: {:?}, name: {})",
            account_id,
            req.user_id,
            req.account_type,
            req.account_name
        );

        // 绑定账户到用户（如果设置了UserManager）
        if let Some(user_mgr) = &self.user_manager {
            if let Err(e) = user_mgr.bind_account(&req.user_id, account_id.clone()) {
                log::warn!("Failed to bind account to user: {}", e);
                // 不返回错误，因为账户已成功创建
            }
        }

        // 发送AccountOpen通知（用于WAL恢复）
        if let Some(broker) = &self.notification_broker {
            let notification = Notification::new(
                NotificationType::AccountOpen,
                Arc::from(account_id.clone()),
                NotificationPayload::AccountOpen(AccountOpenNotify {
                    account_id: account_id.clone(),
                    user_id: req.user_id.clone(),
                    account_name: req.account_name.clone(),
                    init_cash: req.init_cash,
                    account_type: req.account_type as u8,
                    timestamp: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
                }),
                "AccountManager",
            );

            if let Err(e) = broker.publish(notification) {
                log::error!("Failed to publish AccountOpen notification: {}", e);
                // 不返回错误，因为账户已成功创建
            }
        }

        Ok(account_id)
    }

    /// 销户
    pub fn close_account(&self, account_id: &str) -> Result<(), ExchangeError> {
        // 获取元数据（用于更新用户账户索引）
        let metadata = self.metadata.get(account_id).map(|m| m.clone());

        // ✨ 先校验,后删除 @yutiansut @quantaxis
        //
        // 原实现是 `if let Some((_, account)) = self.accounts.remove(account_id)` ——
        // **remove 无条件先执行**,然后才检查持仓和余额。校验失败时直接
        // `return Err(...)`,账户**永不回插**:它已经从 accounts 表里消失,
        // 而 metadata / user_accounts / user_manager 的绑定还留着 → 状态不一致,
        // 账户静默丢失且无法通过任何接口找回。
        //
        // 该函数目前**无 HTTP 端点**(routes.rs 里没有销户路由),唯一的其他引用是
        // `test_close_account_not_found`,而那个测试只覆盖「账户不存在」,
        // 恰好绕过了这两条会丢数据的分支 —— 所以这是个等着被暴露的雷。
        //
        // ⚠️ 校验块必须自成一个作用域:DashMap 的 Ref 未释放就对同一张表
        // 调 remove 会死锁。
        {
            let account = self.accounts.get(account_id).ok_or_else(|| {
                ExchangeError::NotFound(format!("Account not found: {}", account_id))
            })?;
            let acc = account.read();

            if !acc.hold.is_empty() {
                return Err(ExchangeError::AccountError(
                    "Cannot close account with open positions".to_string(),
                ));
            }

            if acc.money > 0.0 {
                return Err(ExchangeError::AccountError(
                    "Cannot close account with remaining balance".to_string(),
                ));
            }
        }

        if self.accounts.remove(account_id).is_some() {
            // 从用户账户索引中移除
            if let Some(meta) = metadata {
                if let Some(mut accounts) = self.user_accounts.get_mut(&meta.user_id) {
                    accounts.retain(|id| id != account_id);
                }

                // 从用户解绑账户
                if let Some(user_mgr) = &self.user_manager {
                    if let Err(e) = user_mgr.unbind_account(&meta.user_id, account_id) {
                        log::warn!("Failed to unbind account from user: {}", e);
                    }
                }
            }

            self.metadata.remove(account_id);

            log::info!("Account closed: {}", account_id);
            Ok(())
        } else {
            Err(ExchangeError::NotFound(format!(
                "Account not found: {}",
                account_id
            )))
        }
    }

    /// 查询账户（通过账户ID）
    pub fn get_account(&self, account_id: &str) -> Result<Arc<RwLock<QA_Account>>, ExchangeError> {
        self.accounts
            .get(account_id)
            .map(|r| r.value().clone())
            .ok_or_else(|| {
                ExchangeError::NotFound(format!("Account not found: {}", account_id))
            })
    }

    /// 查询用户的所有账户
    /// 获取用户的账户列表
    ///
    /// ✨ 支持两种模式 @yutiansut @quantaxis：
    /// 1. 传入user_id（UUID格式）→ 查找该用户的所有账户（经纪商模式）
    /// 2. 传入account_id（ACC_xxx格式）→ 直接返回该账户（交易所模式/直接登录）
    pub fn get_accounts_by_user(&self, user_id: &str) -> Vec<Arc<RwLock<QA_Account>>> {
        // 1. 先尝试作为user_id查找（经纪商模式）
        if let Some(account_ids) = self.user_accounts.get(user_id) {
            return account_ids
                .iter()
                .filter_map(|id| self.accounts.get(id).map(|r| r.value().clone()))
                .collect();
        }

        // 2. ✨ 如果找不到，检查是否是账户ID本身（交易所模式/直接登录）
        if user_id.starts_with("ACC_") {
            if let Some(account) = self.accounts.get(user_id) {
                log::debug!(
                    "get_accounts_by_user: treating {} as account_id (exchange mode)",
                    user_id
                );
                return vec![account.value().clone()];
            }
        }

        // 3. 都找不到，返回空列表
        vec![]
    }

    /// 获取用户的默认账户（第一个账户）
    ///
    /// 这是一个便捷方法，用于兼容旧代码（当用户只有一个账户时）
    /// 如果用户有多个账户，返回第一个账户
    pub fn get_default_account(
        &self,
        user_id: &str,
    ) -> Result<Arc<RwLock<QA_Account>>, ExchangeError> {
        self.user_accounts
            .get(user_id)
            .and_then(|account_ids| {
                account_ids
                    .first()
                    .and_then(|id| self.accounts.get(id).map(|r| r.value().clone()))
            })
            .ok_or_else(|| {
                ExchangeError::AccountError(format!("No account found for user: {}", user_id))
            })
    }

    /// 查询用户的账户数量
    pub fn get_user_account_count(&self, user_id: &str) -> usize {
        self.user_accounts
            .get(user_id)
            .map(|ids| ids.len())
            .unwrap_or(0)
    }

    /// 验证账户所有权
    ///
    /// 验证指定的 account_id 是否属于指定的 user_id
    ///
    /// # 参数
    /// - `account_id`: 账户ID（如 "ACC_xxx"）
    /// - `user_id`: 用户ID（UUID）
    ///
    /// # 返回
    /// - `Ok(())` - 验证通过，账户属于该用户
    /// - `Err(ExchangeError::AccountError)` - 账户不存在
    /// - `Err(ExchangeError::PermissionDenied)` - 账户不属于该用户
    ///
    /// # 示例
    /// ```ignore
    /// account_mgr.verify_account_ownership("ACC_xxx", "user123")?;
    /// ```
    /// 验证账户所有权
    ///
    /// ✨ 修改逻辑：交易所只关心账户是否存在，不强制验证User所有权 @yutiansut @quantaxis
    ///
    /// 规则：
    /// - 如果user_id为空字符串，只验证账户存在（交易所模式）
    /// - 如果user_id与account_id相同，只验证账户存在（直接账户登录）
    /// - 否则验证账户所有权（经纪商模式）
    pub fn verify_account_ownership(
        &self,
        account_id: &str,
        user_id: &str,
    ) -> Result<(), ExchangeError> {
        // 1. ✨ 先检查账户是否在accounts中存在（支持WAL恢复后metadata缺失的情况）
        if !self.accounts.contains_key(account_id) {
            // 第四处「账户不存在」—— 先前收敛时我的 grep 只匹配单行 format!,
            // 漏了这个多行形式。@yutiansut @quantaxis
            return Err(ExchangeError::NotFound(format!(
                "Account not found: {}",
                account_id
            )));
        }

        // 2. ✨ 交易所模式：如果user_id为空或等于account_id，只验证账户存在即可
        if user_id.is_empty() || user_id == account_id {
            log::debug!(
                "Account ownership verification passed: exchange mode (account={})",
                account_id
            );
            return Ok(());
        }

        // 3. ✨ 经纪商模式：需要验证所有权，需要metadata
        match self.metadata.get(account_id) {
            Some(metadata) => {
                if metadata.user_id != user_id {
                    return Err(ExchangeError::PermissionDenied(format!(
                        "Account {} does not belong to user {} (owner: {})",
                        account_id, user_id, metadata.user_id
                    )));
                }
                Ok(())
            }
            None => {
                // ⚠️ metadata缺失（可能是WAL恢复后的情况）
                // 在经纪商模式下无法验证所有权，返回错误
                log::warn!(
                    "Account {} exists but metadata is missing (WAL recovery issue?)",
                    account_id
                );
                Err(ExchangeError::AccountError(format!(
                    "Account metadata missing for {} (server may need restart with full recovery)",
                    account_id
                )))
            }
        }
    }

    /// 统计指定合约的总持仓量（多空取最大值，避免重复计数）
    // @yutiansut @quantaxis: 用写锁调用 qars 的 volume_long()/volume_short()
    /// 计算某合约的持仓量(OI)
    ///
    /// ⚠️ **当前实现有两个问题,且这条路径没有消费者** @yutiansut @quantaxis
    ///
    /// ### 问题 1:`max(long, short)` 求和不是持仓量
    /// 交易所的 OI = 未平仓合约数 = **全市场多头总量**(应等于空头总量)。
    /// 这里对**每个账户**取多空较大值再求和:
    ///   · 持 10 多 0 空 → 贡献 10  ✓
    ///   · 持 3  多 10 空 → 贡献 10  ✗ 应为 3(多头侧)
    /// 双边持仓的账户会被高估。正解是 `sum(volume_long)` 或 `sum(volume_short)`。
    ///
    /// ### 问题 2:为了读持仓对每个账户取**写锁**
    /// `entry.value().write()` —— O(账户数) 次写锁,且被 1 秒一轮的
    /// 快照生成器调用(snapshot_generator.rs:430)。
    ///
    /// ### 为什么不修
    /// 唯一调用方是 `MarketSnapshotGenerator`,而它的输出
    /// (`MarketDataService::subscribe_snapshots`)**零消费者** ——
    /// 全库 grep 无非定义引用。所以这个值目前不会到达任何客户端。
    ///
    /// 与此同时,**有消费者的那条路不算 OI**:
    /// DIFF 协议只从 `MarketDataEvent` 转换
    /// (diff_handler.rs `convert_market_event_to_diff`),而
    /// `MarketDataEvent` 的四个变体(Tick / LastPrice / OrderBookSnapshot /
    /// KLineFinished)**一个都不带 OI 字段** —— 尽管
    /// `protocol/diff/types.rs:157` 声明了 `open_interest` / `pre_open_interest`。
    /// 实测:DIFF 客户端收到的 quote 里 `open_interest` 恒为 undefined。
    ///
    /// 即:**算了 OI 的那条路没有消费者,有消费者的那条路不算 OI。**
    /// 要修先决定启用哪条,再一并处理算法与锁的问题。
    pub fn get_instrument_open_interest(&self, instrument_id: &str) -> i64 {
        self.accounts
            .iter()
            .map(|entry| {
                let mut acc = entry.value().write();
                if let Some(pos) = acc.get_position(instrument_id) {
                    let long_total = pos.volume_long();
                    let short_total = pos.volume_short();
                    long_total.max(short_total)
                } else {
                    0.0
                }
            })
            .sum::<f64>() as i64
    }

    /// 查询账户 QIFI 格式（实时 - 仅账户信息）
    /// 直接使用 qars 的 get_accountmessage() 方法获取实时账户数据
    pub fn get_account_qifi(&self, account_id: &str) -> Result<Account, ExchangeError> {
        let account = self.get_account(account_id)?;
        let mut acc = account.write();

        // 直接调用 qars 的方法，它会自动计算所有实时数据
        // balance = get_balance() (实时总权益)
        // available = money (实时现金)
        // margin = get_margin() (实时保证金)
        // position_profit = get_positionprofit() (实时持仓盈亏)
        // risk_ratio = get_riskratio() (实时风险度)
        Ok(acc.get_accountmessage())
    }

    /// 获取完整 QIFI 切片（包含账户+持仓+订单+成交）
    pub fn get_qifi_slice(&self, account_id: &str) -> Result<crate::QIFI, ExchangeError> {
        let account = self.get_account(account_id)?;
        let mut acc = account.write();
        let mut qifi = acc.get_qifi_slice();
        drop(acc);
        conform_qifi_static_balance(&mut qifi);
        Ok(qifi)
    }

    /// 获取 MOM 资金切片（轻量级实时资金快照）
    pub fn get_mom_slice(
        &self,
        account_id: &str,
    ) -> Result<crate::qars::qaaccount::account::QAMOMSlice, ExchangeError> {
        let account = self.get_account(account_id)?;
        let mut acc = account.write();
        Ok(acc.get_mom_slice())
    }

    /// 获取所有账户
    pub fn get_all_accounts(&self) -> Vec<Arc<RwLock<QA_Account>>> {
        self.accounts.iter().map(|r| r.value().clone()).collect()
    }

    /// 获取账户数量
    pub fn get_account_count(&self) -> usize {
        self.accounts.len()
    }

    /// 同步所有账户时间
    pub fn sync_time(&self) {
        let current_time = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        for account in self.accounts.iter() {
            account
                .value()
                .write()
                .change_datetime(current_time.clone());
        }
    }

    /// 获取账户元数据
    pub fn get_account_metadata(
        &self,
        account_id: &str,
    ) -> Option<(String, String, AccountType, i64)> {
        self.metadata.get(account_id).map(|m| {
            (
                m.user_id.clone(),
                m.account_name.clone(),
                m.account_type,
                m.created_at,
            )
        })
    }

    /// 获取账户类型
    pub fn get_account_type(&self, account_id: &str) -> Option<AccountType> {
        self.metadata.get(account_id).map(|m| m.account_type)
    }

    /// 获取账户所属用户
    pub fn get_account_owner(&self, account_id: &str) -> Option<String> {
        self.metadata.get(account_id).map(|m| m.user_id.clone())
    }

    // ========== 方案A: QIFI快照保存与恢复 ==========

    /// 保存所有账户快照到QIFI文件
    /// 两份 QIFI JSON 是否**除 `updatetime` 外完全相同**。@yutiansut @quantaxis
    ///
    /// 用于 `save_snapshots` 跳过无变化的快照。`updatetime` 是快照生成时刻,
    /// 每轮必变,不代表账户状态 —— 实测几十 MB 的快照跨轮 diff **只有它变**。
    ///
    /// 实现上不做完整 JSON 解析(88 MB 解析代价太高),而是按行比对:
    /// `to_string_pretty` 产出的是每字段一行,`updatetime` 恰好独占一行。
    /// 行数不同 ⇒ 直接判不等;只有该行不同 ⇒ 判等。
    fn qifi_json_eq_ignoring_updatetime(prev: &str, cur: &str) -> bool {
        let mut a = prev.lines();
        let mut b = cur.lines();
        loop {
            match (a.next(), b.next()) {
                (None, None) => return true,
                (Some(x), Some(y)) => {
                    if x != y {
                        // 只容忍 updatetime 那一行不同
                        let is_ut = |l: &str| l.trim_start().starts_with("\"updatetime\":");
                        if !(is_ut(x) && is_ut(y)) {
                            return false;
                        }
                    }
                }
                _ => return false, // 行数不同
            }
        }
    }

    pub fn save_snapshots(&self, snapshot_dir: &str) -> Result<usize, ExchangeError> {
        // 串行化:见 `save_lock` 字段注释(周期快照与停机最终快照并发写同一批 tmp)
        let _save_guard = self.save_lock.lock();
        std::fs::create_dir_all(snapshot_dir)
            .map_err(|e| ExchangeError::IOError(format!("Create snapshot dir failed: {}", e)))?;

        let mut saved_count = 0;
        let mut skipped_count = 0usize;

        for entry in self.accounts.iter() {
            let account_id = entry.key();
            // ✨ 安全:account_id 在下方(:738)被 format! 拼进快照文件名,绝不能含路径分隔符或 `..`。
            //    否则未鉴权 POST /api/account/open 传 account_id="../../../tmp/x" 会穿越写出快照目录
            //    (任意文件写)。含不安全字符的账户**跳过落盘 + 告警**,不影响其他账户的快照。
            //    这是防御纵深:开户侧也应做字符白名单,但这一层保证「快照文件名恒安全」。@yutiansut @quantaxis
            if account_id.is_empty()
                || account_id.contains('/')
                || account_id.contains("..")
            {
                log::error!(
                    "快照跳过: account_id {:?} 含不安全路径字符(潜在穿越攻击), 不落盘",
                    account_id
                );
                continue;
            }
            let account = entry.value();

            // 获取QIFI快照
            let mut acc = account.write();
            let qifi = acc.get_qifi_slice();
            drop(acc);
            // ⚠️ 这里**故意不调用** `conform_qifi_static_balance` —— 快照必须原样保留
            //    qars 的内部语义(`static_balance` = 初始资金),否则重启读回后
            //    `get_balance()` 会重复计入 deposit。详见该函数上的事故复盘。
            //    @yutiansut @quantaxis

            // 序列化为JSON
            let json = serde_json::to_string_pretty(&qifi).map_err(|e| {
                ExchangeError::SerializationError(format!("QIFI serialization failed: {}", e))
            })?;

            // 写入文件（使用账户ID作为文件名）—— **原子写** @yutiansut @quantaxis
            //
            // ⚠️ 原实现是 `std::fs::write(&file_path, json)`,**非原子**:
            //    先截断再写,写到一半被杀就留下截断文件。
            //    实测事故(2026-09-05 04:13):做市账户 MM_ALPHA_BID 的快照有
            //    **131,626,368 字节**(QIFI 把 171,983 条订单全部内嵌),
            //    每 60 秒重写一次,写入窗口很长;一次 SIGKILL 正好打在中途,
            //    文件只剩 **1,220 字节**。叠加下面 `restore_from_snapshots`
            //    「一个坏文件中止全部恢复」的缺陷,导致 59 个账户全部丢状态、
            //    **172,035 条订单与 59,209 笔成交消失**。
            //
            //    改为 temp + rename:同目录下先写 `.tmp`,fsync 后 rename 覆盖。
            //    同文件系统内的 rename 是原子的 —— 崩溃时要么是完整的旧文件,
            //    要么是完整的新文件,不会出现半截。
            let file_path = format!("{}/{}.json", snapshot_dir, account_id);
            let tmp_path = format!("{}/{}.json.tmp", snapshot_dir, account_id);

            // ✨ 内容没变就不写盘。@yutiansut @quantaxis
            //
            // 实测(2026-09-06 生产):快照总量 **304 MB**、每 60 秒全量重写
            // ⇒ **428 GB/天**。而 82 个账户里 **75 个合计只有 248 KB**(中位 1 KB),
            // 最大 7 个占 99.9%(单个最大 **88.1 MB**)。
            //
            // 更关键的是:把这几个大文件跨轮 diff,**唯一变化的字段是 `updatetime`**
            // (三个样本 88.1M / 37.4M / 35.9M 全都只有这一个字段变)——
            // 订单、成交、持仓、资金一动没动,却把几十 MB 重写一遍。
            //
            // ⚠️ 为什么用**内容比对**而不是脏标记:脏标记要在每个改账户的路径上打点,
            // **漏标一处就丢状态**,而丢状态比多写严重得多。内容比对拿最终字节说话,
            // 零漏标风险 —— 代价是仍要序列化一次(那部分本来就要做)。
            //
            // ⚠️ `updatetime` 会导致比对恒不相等,所以**比对时把它剔除**:
            // 它是快照生成时刻,不代表账户状态。若账户真有变化,
            // orders/trades/positions/accounts 任一字段必然跟着变。
            let prev_matches = match std::fs::read_to_string(&file_path) {
                Ok(prev) => Self::qifi_json_eq_ignoring_updatetime(&prev, &json),
                Err(_) => false, // 文件不存在/读不了 ⇒ 必须写
            };
            if prev_matches {
                skipped_count += 1;
                continue;
            }
            {
                use std::io::Write;
                let mut f = std::fs::File::create(&tmp_path).map_err(|e| {
                    ExchangeError::IOError(format!("Create snapshot tmp failed: {}", e))
                })?;
                f.write_all(json.as_bytes()).map_err(|e| {
                    ExchangeError::IOError(format!("Write snapshot tmp failed: {}", e))
                })?;
                // fsync:确保数据落盘后再 rename,否则崩溃后 rename 可能先于数据可见
                f.sync_all().map_err(|e| {
                    ExchangeError::IOError(format!("Sync snapshot tmp failed: {}", e))
                })?;
            }
            std::fs::rename(&tmp_path, &file_path)
                .map_err(|e| ExchangeError::IOError(format!("Rename snapshot failed: {}", e)))?;

            saved_count += 1;
        }

        log::info!(
            "Saved {} account snapshots to {} ({} unchanged, skipped)",
            saved_count,
            snapshot_dir,
            skipped_count
        );
        Ok(saved_count)
    }

    /// 从QIFI快照恢复所有账户
    pub fn restore_from_snapshots(&self, snapshot_dir: &str) -> Result<usize, ExchangeError> {
        let snapshot_path = Path::new(snapshot_dir);

        if !snapshot_path.exists() {
            log::info!(
                "No snapshot directory found at {}, skipping recovery",
                snapshot_dir
            );
            return Ok(0);
        }

        let mut restored_count = 0;
        // 损坏/无法解析的快照清单 —— 不中止恢复,末尾统一告警 @yutiansut @quantaxis
        let mut corrupted_files: Vec<String> = Vec::new();

        for entry in std::fs::read_dir(snapshot_path)
            .map_err(|e| ExchangeError::IOError(format!("Read snapshot dir failed: {}", e)))?
        {
            let entry = entry
                .map_err(|e| ExchangeError::IOError(format!("Read dir entry failed: {}", e)))?;

            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }

            // ⚠️ **单个坏文件不得中止整批恢复** @yutiansut @quantaxis
            //
            // 原实现对读取失败和反序列化失败都用 `?` 向外传播,于是
            // **一个损坏的快照 = 全部账户都恢复不了**。
            // 实测事故(2026-09-05 04:13):MM_ALPHA_BID 的 131 MB 快照因
            // 非原子写 + SIGKILL 被截断成 1,220 字节,本函数直接整体返回 Err,
            // 58 个完好快照一个都没恢复;随后 WAL 兜底按 `init_cash` 重建账户,
            // **172,035 条订单与 59,209 笔成交丢失**。
            //
            // 快照之间是**互相独立**的(一个文件一个账户),坏一个只应损失一个。
            // 改为:记录坏文件、计数、继续处理下一个;函数末尾汇总告警。
            // 这样即使发生损坏,也只丢那一个账户,其余全部正常恢复,
            // 且运维能从告警里看到到底坏了哪些。
            let json = match std::fs::read_to_string(&path) {
                Ok(j) => j,
                Err(e) => {
                    log::error!(
                        "⚠️ 快照文件读取失败,**跳过该账户继续恢复其余**: {:?}, error: {}",
                        path, e
                    );
                    corrupted_files.push(format!("{:?} (read: {})", path.file_name(), e));
                    continue;
                }
            };

            // 容错处理：将 JSON 中的 null 替换为 0.0（兼容旧版快照）
            // 这是临时解决方案，防止因为旧快照中的 null 值导致启动失败
            let sanitized_json = json
                .replace(": null,", ": 0.0,")
                .replace(": null\n", ": 0.0\n");

            let qifi: QIFI = match serde_json::from_str(&sanitized_json) {
                Ok(q) => q,
                Err(e) => {
                    log::error!(
                        "⚠️ 快照反序列化失败,**跳过该账户继续恢复其余**: {:?} ({} 字节), error: {}",
                        path,
                        sanitized_json.len(),
                        e
                    );
                    corrupted_files.push(format!("{:?} ({} bytes: {})", path.file_name(), sanitized_json.len(), e));
                    continue;
                }
            };

            // 恢复账户
            match self.restore_account_from_qifi(qifi) {
                Ok(_) => {
                    log::debug!("Successfully restored account from {:?}", path.file_name());
                    restored_count += 1;
                }
                Err(e) => {
                    log::warn!(
                        "Failed to restore account from {:?}: {}, skipping",
                        path.file_name(),
                        e
                    );
                    // 继续处理其他文件，不中断恢复流程
                }
            }
        }

        log::info!(
            "Restored {} accounts from snapshots in {}",
            restored_count,
            snapshot_dir
        );
        if !corrupted_files.is_empty() {
            // ⚠️ 措辞按 2026-09-06 实测改准:原文说「可能丢失」,实测是**必然丢失**。
            //
            // 零快照仅 WAL 启动(生产 289MB 账户 WAL + 2.5MB 用户 WAL)实测:
            //   用户 36 ✅  账户 82 ✅  —— 账户和资金回得来
            //   总保证金 10.23 亿 → **0**;前 20 账户持仓 9 → **0**、订单 116 → **0**
            // 根因:`recovery.rs::apply_record` 对 `OrderInsert`/`TradeExecuted` 写着
            // 「暂不处理(仅用于审计)」,对 `PositionSnapshot` 写着「由
            // `UnifiedRecoveryManager` 处理」—— 而那个模块(1372 行、4 个测试)
            // **生产零调用**(lib.rs:207 已记录)。数据在 WAL 里,只是没人读。
            //
            // 这比「整个账户丢失」更难发现:账户在、余额正常,只是持仓空了。
            // 所以这条告警必须明确到「需要人工对账补回持仓」。@yutiansut @quantaxis
            log::error!(
                "⚠️⚠️ 有 {} 个快照文件损坏、已跳过。这些账户会退回 WAL 恢复,\
                 但 WAL 恢复**只还原账户与资金,不还原持仓/委托/成交** \
                 (实测:保证金归零、持仓归零、订单归零,而余额正常)—— \
                 账实将不符且不易察觉,**需人工从上游对账补回持仓**。损坏清单: {:?}",
                corrupted_files.len(),
                corrupted_files
            );
        }

        Ok(restored_count)
    }

    /// 从单个QIFI恢复账户
    pub fn restore_account_from_qifi(&self, qifi: QIFI) -> Result<(), ExchangeError> {
        let account_id = qifi.account_cookie.clone();
        let user_id = qifi.portfolio.clone();
        let account_name = qifi.investor_name.clone(); // 从 QIFI investor_name 恢复账户名称

        // 检查账户是否已存在
        if self.accounts.contains_key(&account_id) {
            log::warn!(
                "Account {} already exists, skipping restoration",
                account_id
            );
            return Ok(());
        }

        // 从QIFI创建QA_Account
        // ⚠️ 重要修复：new_from_qifi 默认设置 environment="real"，会导致 send_order 立即成交
        // 必须重置为 "sim" 以支持正常的订单生命周期：send_order → frozen → cancel_order/receive_deal
        // @yutiansut @quantaxis
        let mut account = QA_Account::new_from_qifi(qifi);
        account.environment = "sim".to_string(); // 重置为 sim 模式
        // 按合约覆盖的费率 / 保证金写回持仓副本(恢复时副本按 qars 预设重建)@yutiansut @quantaxis
        crate::core::account_ext::apply_preset_overrides_all(&mut account);
        // 持仓均价按成本校正(旧快照里 qars 平仓公式漂移出来的值、以及 NaN 落盘成 null 再被换成 0 的值)
        crate::core::account_ext::conform_position_prices_all(&mut account);

        // ✨ 从待处理订单重建 frozen HashMap @yutiansut @quantaxis
        // WAL 只存储 frozen 的总金额，不存储每个订单的 frozen 详情
        // 需要从 dailyorders 中的待处理订单重建
        self.rebuild_frozen_from_pending_orders(&mut account);

        // 存储账户
        self.accounts
            .insert(account_id.clone(), Arc::new(RwLock::new(account)));

        // 恢复元数据（从QIFI恢复）
        // 注意：account_type 和 created_at 在使用 update_metadata_for_recovery() 后会被正确恢复
        let metadata = AccountMetadata {
            user_id: user_id.clone(),
            account_name: if account_name.is_empty() {
                account_id.clone()
            } else {
                account_name
            }, // 从 QIFI investor_name 恢复
            account_type: AccountType::Individual, // 默认值，恢复时会被 update_metadata_for_recovery() 覆盖
            created_at: chrono::Utc::now().timestamp(), // 默认值，恢复时会被 update_metadata_for_recovery() 覆盖
        };
        self.metadata.insert(account_id.clone(), metadata);

        // 更新用户账户索引
        self.user_accounts
            .entry(user_id.clone())
            .or_default()
            .push(account_id.clone());

        // 绑定账户到用户（如果设置了UserManager）
        if let Some(user_mgr) = &self.user_manager {
            if let Err(e) = user_mgr.bind_account(&user_id, account_id.clone()) {
                log::warn!("Failed to bind restored account to user: {}", e);
            }
        }

        log::info!(
            "Restored account {} (user: {}) from QIFI snapshot",
            account_id,
            user_id
        );
        Ok(())
    }

    /// 从待处理订单重建 frozen HashMap
    ///
    /// WAL 只存储 frozen 的总金额，不存储每个订单的 frozen 详情。
    /// 此方法从 dailyorders 中的待处理订单（状态为 SUBMITTED）重建 frozen HashMap。
    ///
    /// # 计算公式
    /// - coeff = preset.calc_coeff() * price (对于 BUY/SELL OPEN)
    /// - frozen_money = coeff * volume_left
    ///
    /// @yutiansut @quantaxis
    fn rebuild_frozen_from_pending_orders(&self, account: &mut crate::QA_Account) {
        use qars::qaprotocol::qifi::account::Frozen;

        // 只处理 SUBMITTED 状态的订单
        let pending_orders: Vec<(String, String, f64, f64, String, String)> = account
            .dailyorders
            .iter()
            .filter(|(_, order)| order.status == "SUBMITTED" || order.status == "ALIVE")
            .map(|(order_id, order)| {
                (
                    order_id.clone(),
                    order.instrument_id.clone(),
                    order.volume_left,
                    order.limit_price,
                    order.direction.clone(),
                    order.offset.clone(),
                )
            })
            .collect();

        if pending_orders.is_empty() {
            log::debug!(
                "No pending orders to rebuild frozen for account {}",
                account.account_cookie
            );
            return;
        }

        log::info!(
            "Rebuilding frozen HashMap for account {} from {} pending orders",
            account.account_cookie,
            pending_orders.len()
        );

        for (order_id, instrument_id, volume_left, price, direction, offset) in pending_orders {
            // 只有 OPEN 订单需要冻结保证金
            if offset != "OPEN" {
                continue;
            }

            // 获取或创建持仓以获取 preset
            if !account.hold.contains_key(&instrument_id) {
                account.init_h(&instrument_id);
            }

            if let Some(pos) = account.hold.get(&instrument_id) {
                // 计算 coeff: 根据买卖方向使用不同的保证金系数
                let coeff = if direction == "BUY" {
                    pos.preset.calc_coeff() * price
                } else {
                    pos.preset.calc_sellopencoeff() * price
                };

                let frozen_money = coeff * volume_left;

                // 插入到 frozen HashMap
                account.frozen.insert(
                    order_id.clone(),
                    Frozen {
                        amount: volume_left,
                        coeff,
                        money: frozen_money,
                    },
                );

                log::info!(
                    "🔄 Rebuilt frozen: account={}, order_id={}, instrument={}, frozen_money={:.2}",
                    account.account_cookie,
                    order_id,
                    instrument_id,
                    frozen_money
                );
            }
        }

        let total_frozen: f64 = account.frozen.values().map(|f| f.money).sum();
        log::info!(
            "✅ Rebuilt frozen HashMap complete: account={}, frozen_count={}, total_frozen={:.2}",
            account.account_cookie,
            account.frozen.len(),
            total_frozen
        );
    }

    /// 更新账户余额（仅用于恢复）
    ///
    /// 这是一个特殊方法，仅在从WAL恢复账户时使用。
    /// 正常交易过程中不应使用此方法，而应通过交易回报更新余额。
    ///
    /// # 参数
    /// - `account_id`: 账户ID
    /// - `balance`: 新的账户余额
    /// - `available`: 可用资金
    /// - `deposit`: 累计入金
    /// - `withdraw`: 累计出金
    ///
    /// # 安全性
    /// 此方法直接修改账户余额，绕过了正常的交易流程。
    /// 仅在恢复流程中使用，确保数据一致性由调用者负责。
    pub fn update_balance_for_recovery(
        &self,
        account_id: &str,
        balance: f64,
        available: f64,
        deposit: f64,
        withdraw: f64,
    ) -> Result<(), ExchangeError> {
        let account = self.get_account(account_id)?;
        let mut acc = account.write();

        // 直接设置字段值（通过 accounts QIFI 结构）
        acc.accounts.balance = balance;
        acc.accounts.available = available;
        acc.accounts.deposit = deposit;
        acc.accounts.withdraw = withdraw;

        // ⚠️ **不要**在这里重算 static_balance。@yutiansut @quantaxis
        //
        // 原代码是 `acc.accounts.static_balance = pre_balance + deposit - withdraw;`
        // —— 那是 **QIFI 规范**的语义,但 qars 内部用的是**另一套**:
        // `account.rs:145` 构造时把 static_balance 设为 `init_cash` 后**永不更新**
        // (`deposit()` 只动 `money` 与 `accounts.deposit`),因此它的 `get_balance()`
        // (`account.rs:525`)**必须**自己再加一次 `deposit - withdraw`。
        // 把规范语义写进 qars 内部 ⇒ `get_balance()` 重复计入 deposit,凭空造权益。
        // (同一错误在 `save_snapshots` 上造成过 ¥1,994,454,806 的虚增,见
        //  `conform_qifi_static_balance` 上的事故复盘。)
        // `open_account` 已经把 static_balance 正确置为 init_cash,这里保持不动。

        // ✨ 恢复现金余额 —— qars 的 `available` 取的是 `self.money`
        // (`account.rs:425: available: self.money`),而不是 `accounts.available`。
        // 上面那行 `acc.accounts.available = available` 对输出**毫无作用**;
        // 不同步 `money` 会让恢复出来的账户可用资金停在 init_cash,少掉净入金。
        acc.money = available;

        log::debug!(
            "Updated balance for account {} during recovery: balance={}, available={}, deposit={}, withdraw={}",
            account_id, balance, available, deposit, withdraw
        );

        Ok(())
    }

    /// 更新账户元数据（仅用于恢复）
    ///
    /// 这是一个特殊方法，仅在从WAL恢复账户时使用。
    /// 用于恢复 account_type 和 created_at 字段。
    ///
    /// # 参数
    /// - `account_id`: 账户ID
    /// - `account_type`: 账户类型
    /// - `created_at`: 创建时间戳
    pub fn update_metadata_for_recovery(
        &self,
        account_id: &str,
        account_type: AccountType,
        created_at: i64,
    ) -> Result<(), ExchangeError> {
        let mut metadata = self.metadata.get_mut(account_id).ok_or_else(|| {
            ExchangeError::NotFound(format!("Account not found: {}", account_id))
        })?;

        metadata.account_type = account_type;
        metadata.created_at = created_at;

        log::debug!(
            "Updated metadata for account {} during recovery: account_type={:?}, created_at={}",
            account_id,
            account_type,
            created_at
        );

        Ok(())
    }
}

impl Default for AccountManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== AccountManager 基础测试 @yutiansut @quantaxis ====================

    /// 测试 AccountManager::new()
    #[test]
    fn test_account_manager_new() {
        let mgr = AccountManager::new();

        assert_eq!(mgr.accounts.len(), 0);
        assert_eq!(mgr.metadata.len(), 0);
        assert_eq!(mgr.user_accounts.len(), 0);
        assert!(mgr.notification_broker.is_none());
        assert!(mgr.user_manager.is_none());
    }

    /// 测试 Default trait
    #[test]
    fn test_account_manager_default() {
        let mgr = AccountManager::default();

        assert_eq!(mgr.accounts.len(), 0);
        assert!(mgr.notification_broker.is_none());
    }

    /// 测试 with_notification_broker
    #[test]
    fn test_with_notification_broker() {
        let broker = Arc::new(NotificationBroker::new());
        let mgr = AccountManager::with_notification_broker(broker);

        assert!(mgr.notification_broker.is_some());
    }

    /// 测试 set_notification_broker
    #[test]
    fn test_set_notification_broker() {
        let mut mgr = AccountManager::new();
        assert!(mgr.notification_broker.is_none());

        let broker = Arc::new(NotificationBroker::new());
        mgr.set_notification_broker(broker);

        assert!(mgr.notification_broker.is_some());
    }

    /// 测试 notification_broker getter
    #[test]
    fn test_notification_broker_getter() {
        let mut mgr = AccountManager::new();
        assert!(mgr.notification_broker().is_none());

        let broker = Arc::new(NotificationBroker::new());
        mgr.set_notification_broker(broker);

        assert!(mgr.notification_broker().is_some());
    }

    /// 测试 set_user_manager
    #[test]
    fn test_set_user_manager() {
        let mut mgr = AccountManager::new();
        assert!(mgr.user_manager.is_none());

        let user_mgr = Arc::new(UserManager::new());
        mgr.set_user_manager(user_mgr);

        assert!(mgr.user_manager.is_some());
    }

    // ==================== 开户测试 @yutiansut @quantaxis ====================

    #[test]
    fn test_open_account() {
        let mgr = AccountManager::new();

        let req = OpenAccountRequest {
            user_id: "user_001".to_string(),
            account_id: Some("test_account".to_string()),
            account_name: "Test Account".to_string(),
            init_cash: 100000.0,
            account_type: AccountType::Individual,
        };

        let result = mgr.open_account(req);
        assert!(result.is_ok());

        let account_id = result.unwrap();
        assert_eq!(account_id, "test_account");

        let account = mgr.get_account(&account_id);
        assert!(account.is_ok());
    }

    /// 安全回归:恶意 account_id 不得穿越写出快照目录。@yutiansut @quantaxis
    #[test]
    fn test_snapshot_rejects_path_traversal_account_id() {
        let mgr = AccountManager::new();
        // open_account 允许自定义 account_id(见 test_open_account),含穿越字符
        let req = OpenAccountRequest {
            user_id: "u".to_string(),
            account_id: Some("../qaex_traversal_probe".to_string()),
            account_name: "x".to_string(),
            init_cash: 0.0,
            account_type: AccountType::Individual,
        };
        assert!(mgr.open_account(req).is_ok());

        let base = std::env::temp_dir().join(format!("qaex_trav_{}", std::process::id()));
        let snap_dir = base.join("snapshots");
        std::fs::create_dir_all(&snap_dir).unwrap();
        let _ = mgr.save_snapshots(snap_dir.to_str().unwrap());

        // 穿越目标 = snap_dir 的上一级(../qaex_traversal_probe.json)
        let traversal_target = base.join("qaex_traversal_probe.json");
        let created = traversal_target.exists();
        let _ = std::fs::remove_file(&traversal_target);
        let _ = std::fs::remove_dir_all(&base);
        assert!(!created, "路径穿越: 恶意 account_id 写出了快照目录外的文件!");
    }

    /// 安全回归:超长 account_name 应被拒绝(防快照膨胀 DoS)。@yutiansut @quantaxis
    #[test]
    fn test_open_account_rejects_oversized_name() {
        let mgr = AccountManager::new();
        let req = OpenAccountRequest {
            user_id: "u".to_string(),
            account_id: Some("acc_bigname".to_string()),
            account_name: "x".repeat(300), // > 256 字节
            init_cash: 0.0,
            account_type: AccountType::Individual,
        };
        assert!(mgr.open_account(req).is_err(), "超长 account_name 应被拒绝");
    }

    #[test]
    fn test_duplicate_account() {
        let mgr = AccountManager::new();

        let req = OpenAccountRequest {
            user_id: "user_001".to_string(),
            account_id: Some("test_account".to_string()),
            account_name: "Test Account".to_string(),
            init_cash: 100000.0,
            account_type: AccountType::Individual,
        };

        mgr.open_account(req.clone()).unwrap();
        let result = mgr.open_account(req);
        assert!(result.is_err());
    }

    #[test]
    fn test_user_account_mapping() {
        let mgr = AccountManager::new();

        let user_id = "user_001";

        // 为同一用户创建多个账户
        let req1 = OpenAccountRequest {
            user_id: user_id.to_string(),
            account_id: Some("account_1".to_string()),
            account_name: "Account 1".to_string(),
            init_cash: 100000.0,
            account_type: AccountType::Individual,
        };

        let req2 = OpenAccountRequest {
            user_id: user_id.to_string(),
            account_id: Some("account_2".to_string()),
            account_name: "Account 2".to_string(),
            init_cash: 50000.0,
            account_type: AccountType::Institutional,
        };

        mgr.open_account(req1).unwrap();
        mgr.open_account(req2).unwrap();

        // 验证用户账户映射
        let accounts = mgr.get_accounts_by_user(user_id);
        assert_eq!(accounts.len(), 2);

        let count = mgr.get_user_account_count(user_id);
        assert_eq!(count, 2);
    }

    #[test]
    fn test_account_metadata() {
        let mgr = AccountManager::new();

        let req = OpenAccountRequest {
            user_id: "user_001".to_string(),
            account_id: Some("test_account".to_string()),
            account_name: "My Trading Account".to_string(),
            init_cash: 100000.0,
            account_type: AccountType::MarketMaker,
        };

        let account_id = mgr.open_account(req).unwrap();

        let metadata = mgr.get_account_metadata(&account_id);
        assert!(metadata.is_some());

        let (user_id, account_name, account_type, _created_at) = metadata.unwrap();
        assert_eq!(user_id, "user_001");
        assert_eq!(account_name, "My Trading Account");
        assert_eq!(account_type, AccountType::MarketMaker);
    }

    #[test]
    fn test_verify_account_ownership() {
        let mgr = AccountManager::new();

        // 创建账户
        let req = OpenAccountRequest {
            user_id: "user_alice".to_string(),
            account_id: Some("ACC_alice_001".to_string()),
            account_name: "Alice's Account".to_string(),
            init_cash: 100000.0,
            account_type: AccountType::Individual,
        };

        let account_id = mgr.open_account(req).unwrap();
        assert_eq!(account_id, "ACC_alice_001");

        // 测试1: 正确的用户验证账户所有权 - 应该成功
        let result = mgr.verify_account_ownership(&account_id, "user_alice");
        assert!(result.is_ok(), "Alice should own her account");

        // 测试2: 错误的用户验证账户所有权 - 应该失败
        let result = mgr.verify_account_ownership(&account_id, "user_bob");
        assert!(result.is_err(), "Bob should not own Alice's account");

        match result {
            Err(ExchangeError::PermissionDenied(msg)) => {
                assert!(
                    msg.contains("does not belong to"),
                    "Error message should indicate ownership mismatch"
                );
                assert!(
                    msg.contains("user_bob"),
                    "Error message should mention the requesting user"
                );
            }
            _ => panic!("Expected PermissionDenied error"),
        }

        // 测试3: 不存在的账户 - 应该失败
        let result = mgr.verify_account_ownership("ACC_nonexistent", "user_alice");
        assert!(result.is_err(), "Nonexistent account should fail");

        match result {
            Err(ExchangeError::NotFound(msg)) => {
                assert!(
                    msg.contains("not found"),
                    "Error message should indicate account not found"
                );
            }
            _ => panic!("Expected NotFound for nonexistent account"),
        }
    }

    // ==================== 开户扩展测试 @yutiansut @quantaxis ====================

    /// 测试自动生成账户ID
    #[test]
    fn test_open_account_auto_id() {
        let mgr = AccountManager::new();

        let req = OpenAccountRequest {
            user_id: "user_001".to_string(),
            account_id: None, // 不提供账户ID，自动生成
            account_name: "Auto ID Account".to_string(),
            init_cash: 100000.0,
            account_type: AccountType::Individual,
        };

        let result = mgr.open_account(req);
        assert!(result.is_ok());

        let account_id = result.unwrap();
        assert!(account_id.starts_with("ACC_"), "Auto-generated ID should start with ACC_");
    }

    /// 测试零初始资金开户
    #[test]
    fn test_open_account_zero_cash() {
        let mgr = AccountManager::new();

        let req = OpenAccountRequest {
            user_id: "user_001".to_string(),
            account_id: Some("zero_cash_account".to_string()),
            account_name: "Zero Cash".to_string(),
            init_cash: 0.0,
            account_type: AccountType::Individual,
        };

        let result = mgr.open_account(req);
        assert!(result.is_ok());
    }

    /// 测试不同账户类型
    #[test]
    fn test_open_account_different_types() {
        let mgr = AccountManager::new();

        let types = [
            AccountType::Individual,
            AccountType::Institutional,
            AccountType::MarketMaker,
        ];

        for (i, account_type) in types.iter().enumerate() {
            let req = OpenAccountRequest {
                user_id: "user_001".to_string(),
                account_id: Some(format!("account_type_{}", i)),
                account_name: format!("Account Type {:?}", account_type),
                init_cash: 100000.0,
                account_type: *account_type,
            };

            let result = mgr.open_account(req);
            assert!(result.is_ok());

            let metadata = mgr.get_account_type(&format!("account_type_{}", i));
            assert_eq!(metadata, Some(*account_type));
        }
    }

    // ==================== 销户测试 @yutiansut @quantaxis ====================

    /// 测试销户不存在的账户
    #[test]
    fn test_close_account_not_found() {
        let mgr = AccountManager::new();

        let result = mgr.close_account("non_existent_account");
        assert!(result.is_err());

        // 「账户不存在」已从 AccountError 收敛到 NotFound(该变体映射 HTTP 404)。
        // 见 lib.rs 的 NotFound 注释。@yutiansut @quantaxis
        match result {
            Err(ExchangeError::NotFound(msg)) => {
                assert!(msg.contains("not found"));
            }
            _ => panic!("Expected NotFound"),
        }
    }

    // ==================== 查询测试 @yutiansut @quantaxis ====================

    /// 测试 get_account 成功
    #[test]
    fn test_get_account_success() {
        let mgr = AccountManager::new();

        let req = OpenAccountRequest {
            user_id: "user_001".to_string(),
            account_id: Some("get_test".to_string()),
            account_name: "Get Test".to_string(),
            init_cash: 100000.0,
            account_type: AccountType::Individual,
        };

        mgr.open_account(req).unwrap();

        let account = mgr.get_account("get_test");
        assert!(account.is_ok());
    }

    /// 测试 get_account 不存在
    #[test]
    fn test_get_account_not_found() {
        let mgr = AccountManager::new();

        let account = mgr.get_account("non_existent");
        assert!(account.is_err());
    }

    /// 测试 get_accounts_by_user 使用 account_id 模式
    #[test]
    fn test_get_accounts_by_user_account_id_mode() {
        let mgr = AccountManager::new();

        let req = OpenAccountRequest {
            user_id: "user_001".to_string(),
            account_id: Some("ACC_direct_login".to_string()),
            account_name: "Direct Login".to_string(),
            init_cash: 100000.0,
            account_type: AccountType::Individual,
        };

        mgr.open_account(req).unwrap();

        // 使用 account_id 直接查询（交易所模式）
        let accounts = mgr.get_accounts_by_user("ACC_direct_login");
        assert_eq!(accounts.len(), 1);
    }

    /// 测试 get_accounts_by_user 不存在用户
    #[test]
    fn test_get_accounts_by_user_not_found() {
        let mgr = AccountManager::new();

        let accounts = mgr.get_accounts_by_user("non_existent_user");
        assert!(accounts.is_empty());
    }

    /// 测试 get_default_account 成功
    #[test]
    fn test_get_default_account_success() {
        let mgr = AccountManager::new();

        let req = OpenAccountRequest {
            user_id: "user_default".to_string(),
            account_id: Some("default_account".to_string()),
            account_name: "Default".to_string(),
            init_cash: 100000.0,
            account_type: AccountType::Individual,
        };

        mgr.open_account(req).unwrap();

        let account = mgr.get_default_account("user_default");
        assert!(account.is_ok());
    }

    /// 测试 get_default_account 用户不存在
    #[test]
    fn test_get_default_account_not_found() {
        let mgr = AccountManager::new();

        let account = mgr.get_default_account("non_existent_user");
        assert!(account.is_err());
    }

    /// 测试 get_user_account_count 空用户
    #[test]
    fn test_get_user_account_count_empty() {
        let mgr = AccountManager::new();

        let count = mgr.get_user_account_count("non_existent");
        assert_eq!(count, 0);
    }

    /// 测试 get_user_account_count 有账户
    #[test]
    fn test_get_user_account_count_with_accounts() {
        let mgr = AccountManager::new();

        for i in 0..3 {
            let req = OpenAccountRequest {
                user_id: "count_user".to_string(),
                account_id: Some(format!("count_acc_{}", i)),
                account_name: format!("Count {}", i),
                init_cash: 100000.0,
                account_type: AccountType::Individual,
            };
            mgr.open_account(req).unwrap();
        }

        let count = mgr.get_user_account_count("count_user");
        assert_eq!(count, 3);
    }

    // ==================== 所有权验证测试 @yutiansut @quantaxis ====================

    /// 测试交易所模式（空用户ID）
    #[test]
    fn test_verify_ownership_exchange_mode_empty_user() {
        let mgr = AccountManager::new();

        let req = OpenAccountRequest {
            user_id: "owner".to_string(),
            account_id: Some("ACC_exchange".to_string()),
            account_name: "Exchange Mode".to_string(),
            init_cash: 100000.0,
            account_type: AccountType::Individual,
        };

        mgr.open_account(req).unwrap();

        // 空用户ID应该通过（交易所模式）
        let result = mgr.verify_account_ownership("ACC_exchange", "");
        assert!(result.is_ok());
    }

    /// 测试交易所模式（user_id == account_id）
    #[test]
    fn test_verify_ownership_direct_login_mode() {
        let mgr = AccountManager::new();

        let req = OpenAccountRequest {
            user_id: "owner".to_string(),
            account_id: Some("ACC_direct".to_string()),
            account_name: "Direct Login".to_string(),
            init_cash: 100000.0,
            account_type: AccountType::Individual,
        };

        mgr.open_account(req).unwrap();

        // user_id == account_id 应该通过（直接登录）
        let result = mgr.verify_account_ownership("ACC_direct", "ACC_direct");
        assert!(result.is_ok());
    }

    // ==================== 持仓统计测试 @yutiansut @quantaxis ====================

    /// 测试 get_instrument_open_interest 空账户
    #[test]
    fn test_get_instrument_open_interest_empty() {
        let mgr = AccountManager::new();

        let oi = mgr.get_instrument_open_interest("cu2501");
        assert_eq!(oi, 0);
    }

    /// 测试 get_instrument_open_interest 无持仓
    #[test]
    fn test_get_instrument_open_interest_no_position() {
        let mgr = AccountManager::new();

        let req = OpenAccountRequest {
            user_id: "user_001".to_string(),
            account_id: Some("oi_test".to_string()),
            account_name: "OI Test".to_string(),
            init_cash: 100000.0,
            account_type: AccountType::Individual,
        };

        mgr.open_account(req).unwrap();

        let oi = mgr.get_instrument_open_interest("cu2501");
        assert_eq!(oi, 0);
    }

    // ==================== QIFI 查询测试 @yutiansut @quantaxis ====================

    /// 测试 get_account_qifi
    #[test]
    fn test_get_account_qifi() {
        let mgr = AccountManager::new();

        let req = OpenAccountRequest {
            user_id: "user_001".to_string(),
            account_id: Some("qifi_test".to_string()),
            account_name: "QIFI Test".to_string(),
            init_cash: 100000.0,
            account_type: AccountType::Individual,
        };

        mgr.open_account(req).unwrap();

        let qifi = mgr.get_account_qifi("qifi_test");
        assert!(qifi.is_ok());

        let account_data = qifi.unwrap();
        assert_eq!(account_data.user_id, "qifi_test");
    }

    /// 测试 get_account_qifi 不存在
    #[test]
    fn test_get_account_qifi_not_found() {
        let mgr = AccountManager::new();

        let qifi = mgr.get_account_qifi("non_existent");
        assert!(qifi.is_err());
    }

    /// 测试 get_qifi_slice
    #[test]
    fn test_get_qifi_slice() {
        let mgr = AccountManager::new();

        let req = OpenAccountRequest {
            user_id: "user_001".to_string(),
            account_id: Some("slice_test".to_string()),
            account_name: "Slice Test".to_string(),
            init_cash: 100000.0,
            account_type: AccountType::Individual,
        };

        mgr.open_account(req).unwrap();

        let slice = mgr.get_qifi_slice("slice_test");
        assert!(slice.is_ok());

        let qifi = slice.unwrap();
        assert_eq!(qifi.account_cookie, "slice_test");
    }

    /// 测试 get_mom_slice
    #[test]
    fn test_get_mom_slice() {
        let mgr = AccountManager::new();

        let req = OpenAccountRequest {
            user_id: "user_001".to_string(),
            account_id: Some("mom_test".to_string()),
            account_name: "MOM Test".to_string(),
            init_cash: 100000.0,
            account_type: AccountType::Individual,
        };

        mgr.open_account(req).unwrap();

        let mom = mgr.get_mom_slice("mom_test");
        assert!(mom.is_ok());
    }

    // ==================== 批量查询测试 @yutiansut @quantaxis ====================

    /// 测试 get_all_accounts 空
    #[test]
    fn test_get_all_accounts_empty() {
        let mgr = AccountManager::new();

        let accounts = mgr.get_all_accounts();
        assert!(accounts.is_empty());
    }

    /// 测试 get_all_accounts 有数据
    #[test]
    fn test_get_all_accounts_with_data() {
        let mgr = AccountManager::new();

        for i in 0..5 {
            let req = OpenAccountRequest {
                user_id: format!("user_{}", i),
                account_id: Some(format!("all_acc_{}", i)),
                account_name: format!("All {}", i),
                init_cash: 100000.0,
                account_type: AccountType::Individual,
            };
            mgr.open_account(req).unwrap();
        }

        let accounts = mgr.get_all_accounts();
        assert_eq!(accounts.len(), 5);
    }

    /// 测试 get_account_count
    #[test]
    fn test_get_account_count() {
        let mgr = AccountManager::new();

        assert_eq!(mgr.get_account_count(), 0);

        for i in 0..3 {
            let req = OpenAccountRequest {
                user_id: format!("user_{}", i),
                account_id: Some(format!("count_{}", i)),
                account_name: format!("Count {}", i),
                init_cash: 100000.0,
                account_type: AccountType::Individual,
            };
            mgr.open_account(req).unwrap();
        }

        assert_eq!(mgr.get_account_count(), 3);
    }

    // ==================== 时间同步测试 @yutiansut @quantaxis ====================

    /// 测试 sync_time
    #[test]
    fn test_sync_time() {
        let mgr = AccountManager::new();

        let req = OpenAccountRequest {
            user_id: "user_001".to_string(),
            account_id: Some("sync_test".to_string()),
            account_name: "Sync Test".to_string(),
            init_cash: 100000.0,
            account_type: AccountType::Individual,
        };

        mgr.open_account(req).unwrap();

        // 调用同步时间不应该panic
        mgr.sync_time();
    }

    // ==================== 元数据查询测试 @yutiansut @quantaxis ====================

    /// 测试 get_account_type 成功
    #[test]
    fn test_get_account_type() {
        let mgr = AccountManager::new();

        let req = OpenAccountRequest {
            user_id: "user_001".to_string(),
            account_id: Some("type_test".to_string()),
            account_name: "Type Test".to_string(),
            init_cash: 100000.0,
            account_type: AccountType::MarketMaker,
        };

        mgr.open_account(req).unwrap();

        let account_type = mgr.get_account_type("type_test");
        assert_eq!(account_type, Some(AccountType::MarketMaker));
    }

    /// 测试 get_account_type 不存在
    #[test]
    fn test_get_account_type_not_found() {
        let mgr = AccountManager::new();

        let account_type = mgr.get_account_type("non_existent");
        assert!(account_type.is_none());
    }

    /// 测试 get_account_owner
    #[test]
    fn test_get_account_owner() {
        let mgr = AccountManager::new();

        let req = OpenAccountRequest {
            user_id: "owner_user".to_string(),
            account_id: Some("owner_test".to_string()),
            account_name: "Owner Test".to_string(),
            init_cash: 100000.0,
            account_type: AccountType::Individual,
        };

        mgr.open_account(req).unwrap();

        let owner = mgr.get_account_owner("owner_test");
        assert_eq!(owner, Some("owner_user".to_string()));
    }

    /// 测试 get_account_owner 不存在
    #[test]
    fn test_get_account_owner_not_found() {
        let mgr = AccountManager::new();

        let owner = mgr.get_account_owner("non_existent");
        assert!(owner.is_none());
    }

    // ==================== 快照恢复测试 @yutiansut @quantaxis ====================

    /// 测试 update_balance_for_recovery
    #[test]
    fn test_update_balance_for_recovery() {
        let mgr = AccountManager::new();

        let req = OpenAccountRequest {
            user_id: "user_001".to_string(),
            account_id: Some("recovery_test".to_string()),
            account_name: "Recovery Test".to_string(),
            init_cash: 100000.0,
            account_type: AccountType::Individual,
        };

        mgr.open_account(req).unwrap();

        let result = mgr.update_balance_for_recovery(
            "recovery_test",
            200000.0, // balance
            180000.0, // available
            50000.0,  // deposit
            10000.0,  // withdraw
        );

        assert!(result.is_ok());
    }

    /// 测试 update_balance_for_recovery 账户不存在
    #[test]
    fn test_update_balance_for_recovery_not_found() {
        let mgr = AccountManager::new();

        let result = mgr.update_balance_for_recovery(
            "non_existent",
            200000.0,
            180000.0,
            50000.0,
            10000.0,
        );

        assert!(result.is_err());
    }

    /// 测试 update_metadata_for_recovery
    #[test]
    fn test_update_metadata_for_recovery() {
        let mgr = AccountManager::new();

        let req = OpenAccountRequest {
            user_id: "user_001".to_string(),
            account_id: Some("meta_recovery".to_string()),
            account_name: "Meta Recovery".to_string(),
            init_cash: 100000.0,
            account_type: AccountType::Individual,
        };

        mgr.open_account(req).unwrap();

        let result = mgr.update_metadata_for_recovery(
            "meta_recovery",
            AccountType::Institutional,
            1702800000,
        );

        assert!(result.is_ok());

        // 验证更新
        let account_type = mgr.get_account_type("meta_recovery");
        assert_eq!(account_type, Some(AccountType::Institutional));
    }

    /// 测试 update_metadata_for_recovery 账户不存在
    #[test]
    fn test_update_metadata_for_recovery_not_found() {
        let mgr = AccountManager::new();

        let result = mgr.update_metadata_for_recovery(
            "non_existent",
            AccountType::Institutional,
            1702800000,
        );

        assert!(result.is_err());
    }

    /// 测试 restore_from_snapshots 目录不存在
    #[test]
    fn test_restore_from_snapshots_dir_not_exists() {
        let mgr = AccountManager::new();

        let result = mgr.restore_from_snapshots("/non/existent/path");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
    }

    // ==================== 并发测试 @yutiansut @quantaxis ====================

    /// 测试并发开户
    #[test]
    fn test_concurrent_open_accounts() {
        use std::thread;

        let mgr = Arc::new(AccountManager::new());
        let mut handles = vec![];

        for i in 0..10 {
            let mgr_clone = mgr.clone();
            handles.push(thread::spawn(move || {
                let req = OpenAccountRequest {
                    user_id: format!("user_{}", i),
                    account_id: Some(format!("concurrent_acc_{}", i)),
                    account_name: format!("Concurrent {}", i),
                    init_cash: 100000.0,
                    account_type: AccountType::Individual,
                };
                mgr_clone.open_account(req)
            }));
        }

        for handle in handles {
            assert!(handle.join().unwrap().is_ok());
        }

        assert_eq!(mgr.get_account_count(), 10);
    }

    /// 测试并发查询
    #[test]
    fn test_concurrent_queries() {
        use std::thread;

        let mgr = Arc::new(AccountManager::new());

        // 先创建一些账户
        for i in 0..5 {
            let req = OpenAccountRequest {
                user_id: format!("user_{}", i),
                account_id: Some(format!("query_acc_{}", i)),
                account_name: format!("Query {}", i),
                init_cash: 100000.0,
                account_type: AccountType::Individual,
            };
            mgr.open_account(req).unwrap();
        }

        // 并发查询
        let mut handles = vec![];
        for i in 0..10 {
            let mgr_clone = mgr.clone();
            handles.push(thread::spawn(move || {
                let _count = mgr_clone.get_account_count();
                let _all = mgr_clone.get_all_accounts();
                let _acc = mgr_clone.get_account(&format!("query_acc_{}", i % 5));
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }
    }
}

#[cfg(test)]
mod snapshot_skip_tests {
    //! `qifi_json_eq_ignoring_updatetime` 的闸门。@yutiansut @quantaxis
    //!
    //! 这个判据决定**要不要写盘**,判错的方向不对称:
    //!   判等而实际不等 ⇒ **丢状态**(灾难)
    //!   判不等而实际相等 ⇒ 多写一次(仅浪费)
    //! 所以测试重点在「任何实质变化都必须判不等」,宁可多写。

    use super::AccountManager;

    fn snap(updatetime: &str, balance: &str, orders: &str) -> String {
        format!(
            "{{\n  \"account_cookie\": \"ACC_X\",\n  \"updatetime\": \"{}\",\n  \"accounts\": {{\n    \"balance\": {}\n  }},\n  \"orders\": {}\n}}",
            updatetime, balance, orders
        )
    }

    #[test]
    fn only_updatetime_differs_is_equal() {
        // 这正是生产实测的形态:几十 MB 的快照跨轮 diff 只有 updatetime 变
        let a = snap("2026-09-06 04:07:42", "1000.0", "{}");
        let b = snap("2026-09-06 04:08:42", "1000.0", "{}");
        assert!(
            AccountManager::qifi_json_eq_ignoring_updatetime(&a, &b),
            "只有 updatetime 不同应判等"
        );
    }

    #[test]
    fn identical_is_equal() {
        let a = snap("t1", "1000.0", "{}");
        assert!(AccountManager::qifi_json_eq_ignoring_updatetime(&a, &a));
    }

    #[test]
    fn any_substantive_change_is_not_equal() {
        let base = snap("t1", "1000.0", "{}");

        // 余额变
        assert!(
            !AccountManager::qifi_json_eq_ignoring_updatetime(&base, &snap("t1", "1001.0", "{}")),
            "余额变化必须判不等"
        );
        // 余额变 + updatetime 也变(真实场景:有成交时两者同时变)
        assert!(
            !AccountManager::qifi_json_eq_ignoring_updatetime(&base, &snap("t2", "1001.0", "{}")),
            "余额与时间同时变必须判不等"
        );
        // 订单从无到有
        assert!(
            !AccountManager::qifi_json_eq_ignoring_updatetime(
                &base,
                &snap("t1", "1000.0", "{\"O1\": 1}")
            ),
            "新增订单必须判不等"
        );
    }

    #[test]
    fn line_count_change_is_not_equal() {
        // 行数不同 ⇒ 必判不等(哪怕前缀完全相同)
        let a = "{\n  \"a\": 1\n}";
        let b = "{\n  \"a\": 1,\n  \"b\": 2\n}";
        assert!(!AccountManager::qifi_json_eq_ignoring_updatetime(a, b));
        assert!(!AccountManager::qifi_json_eq_ignoring_updatetime(b, a));
    }

    #[test]
    fn updatetime_line_must_be_recognized_on_both_sides() {
        // 一侧是 updatetime、另一侧是别的字段 ⇒ 必须判不等,
        // 否则「字段被替换成 updatetime」这种畸形会被放过
        let a = "{\n  \"updatetime\": \"t1\"\n}";
        let b = "{\n  \"balance\": 1.0\n}";
        assert!(!AccountManager::qifi_json_eq_ignoring_updatetime(a, b));
    }
}
