//! 账户、资金和风控管理 HTTP API
//!
//! 提供账户列表查询、出入金、资金流水、风险监控、全市场订单/成交查询等管理功能
//! @yutiansut @quantaxis

use actix_web::{web, HttpResponse, Result};
use crate::core::account_ext::AccountExt;   // safe_risk_ratio @yutiansut @quantaxis
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use super::models::ApiResponse;
use crate::exchange::{AccountManager, CapitalManager, FundTransaction, OrderRouter, SettlementEngine};
use crate::matching::trade_recorder::TradeRecorder;
use crate::risk::{LiquidationRecord, MarginSummary, RiskAccount, RiskLevel, RiskMonitor};

/// 管理端应用状态
/// @yutiansut @quantaxis
pub struct ManagementAppState {
    pub account_mgr: Arc<AccountManager>,
    pub capital_mgr: Arc<CapitalManager>,
    pub risk_monitor: Arc<RiskMonitor>,
    pub settlement_engine: Arc<SettlementEngine>,
    /// 订单路由器 (用于全市场订单查询)
    pub order_router: Arc<OrderRouter>,
    /// 成交记录器 (用于全市场成交查询)
    pub trade_recorder: Arc<TradeRecorder>,
}

// ============================================================================
// 账户管理 API
// ============================================================================

/// 账户列表响应
#[derive(Debug, Clone, Serialize)]
pub struct AccountListItem {
    /// 账户ID(= QIFI `account_cookie`)
    ///
    /// ⚠️ 历史遗留:下面的 `user_id` 字段装的**也是** account_cookie
    /// (见 `list_all_accounts` 的赋值),名字是错的。前端
    /// `admin/accounts.vue` 照字面绑了 `account_id` / `account_name`,
    /// 后端没有这两个字段 → 两列 20 行永久空白。
    /// 这里补上名字正确的字段;`user_id`/`user_name` 保留不动,
    /// 因为 `market-overview.vue:95` 已经按现有语义绑好了,改了会连带坏掉。
    /// @yutiansut @quantaxis
    pub account_id: String,

    /// 账户名称(与 `user_name` 同值,名字正确的那个)
    pub account_name: String,

    /// 该账户的**所属用户** —— `get_account_metadata` 一直返回它,
    /// 之前被 `let (_owner_user_id, ..)` 丢掉了。
    pub owner_user_id: String,

    /// @deprecated 实为 account_cookie(账户ID),保留仅为兼容既有前端
    pub user_id: String,
    /// @deprecated 实为账户名称,保留仅为兼容既有前端
    pub user_name: String,
    pub account_type: String,
    pub balance: f64,
    pub available: f64,
    pub margin_used: f64,
    pub risk_ratio: f64,
    pub created_at: i64,
    /// 账户状态(active / frozen / suspended / closed)
    ///
    /// ⚠️ 此前 `AccountListItem` **根本没有这个字段**,而前端
    /// `admin/accounts.vue:38` 有「账户状态」下拉并把 `status` 发过来 ——
    /// 后端既不返回状态、也不读这个参数,选「冻结」仍显示全部。
    /// @yutiansut @quantaxis
    pub status: String,
}

/// 账户详情响应
#[derive(Debug, Serialize)]
pub struct AccountDetailResponse {
    pub account_info: serde_json::Value,
    pub positions: Vec<serde_json::Value>,
    pub orders: Vec<serde_json::Value>,
    /// 订单总数（分页前，且已受 `snapshot_ts` 约束）
    pub orders_total: usize,
    /// 翻页锚点 —— 前端翻第 2 页起必须原样带回,否则会看到重复订单
    pub snapshot_ts: i64,
}

/// 服务端排序 —— **服务端分页的列表必须在分页之前按全量排序**。@yutiansut @quantaxis
///
/// 背景:前端此前用 Element UI 的默认 `sortable`,它只对 `:data` 里的行排序,
/// 而 `:data` 只有当前页。实测(82 个账户 / 每页 20):点「账户ID ↑」后
/// **第一页 20 行里有 15 行本不该出现在这里**(第1页 ∩ 全库最小20 = 5/20)。
/// 表面上排过序、实际顺序是错的 —— 比「不能排序」更坏,因为它不声张。
/// 前端已改用 `sortable="custom"`(只发事件、不就地排),由这里排全量。
///
/// ⚠️ 浮点一律走 `num` 分支(`total_cmp`),**不要用 `partial_cmp().unwrap()`**:
/// 本库确有非有限值(空账户的 risk_ratio = 0/0 = NaN,以及被污染的 1e300 余额),
/// `partial_cmp` 遇 NaN 返回 None,unwrap 会 panic 掉整个请求。
///
/// 三类字段:`str` = 实现 Ord 的(String 等);`num` = f64;`dbg` = 枚举(按 Debug 串排)。
/// 未知字段静默不排序 —— 与旧行为一致,避免前端加列时 500。
///
/// ```ignore
/// apply_sort!(list, query.sort_by, query.sort_order,
///     str { "account_id" => account_id },
///     num { "balance" => balance },
///     dbg { "status" => status });
/// ```
macro_rules! apply_sort {
    ($rows:expr, $key:expr, $order:expr
     $(, str { $($sn:literal => $sf:ident),* $(,)? })?
     $(, num { $($nn:literal => $nf:ident),* $(,)? })?
     $(, dbg { $($dn:literal => $df:ident),* $(,)? })?
     $(,)?) => {
        if let Some(__k) = $key.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            let __desc = matches!(
                $order.as_deref().unwrap_or("asc").trim(),
                "desc" | "descending" | "DESC"
            );
            match __k {
                $($( $sn => $rows.sort_by(|a, b| a.$sf.cmp(&b.$sf)), )*)?
                $($( $nn => $rows.sort_by(|a, b| a.$nf.total_cmp(&b.$nf)), )*)?
                $($( $dn => $rows.sort_by_key(|r| format!("{:?}", r.$df)), )*)?
                _ => {}
            }
            if __desc {
                $rows.reverse();
            }
        }
    };
}

/// 查询参数
#[derive(Debug, Deserialize)]
pub struct AccountListQuery {
    pub page: Option<usize>,
    pub page_size: Option<usize>,
    /// 账户状态筛选:active / frozen / suspended / closed(空串或缺省=全部)
    pub status: Option<String>,
    /// 按所属用户筛选 —— 前端 `admin/accounts.vue:290` 一直在发,
    /// 但结构体此前没有这个字段,被 serde 静默丢弃。@yutiansut @quantaxis
    pub user_id: Option<String>,
    /// 排序字段。服务端分页的表必须服务端排序 —— 见下方 sort 段的说明。
    pub sort_by: Option<String>,
    /// 排序方向:asc / desc(缺省 asc)。前端传 Element UI 的
    /// ascending/descending 也接受。
    pub sort_order: Option<String>,
}

/// 获取所有账户列表 (管理端)
pub async fn list_all_accounts(
    query: web::Query<AccountListQuery>,
    state: web::Data<ManagementAppState>,
) -> Result<HttpResponse> {
    let accounts = state.account_mgr.get_all_accounts();

    let mut account_list: Vec<AccountListItem> = accounts
        .iter()
        .filter_map(|account| {
            let mut acc = account.write();

            // 获取元数据
            let (owner_user_id, account_name, account_type, created_at) = state
                .account_mgr
                .get_account_metadata(&acc.account_cookie)?;

            // ✨ 保证金 = 持仓保证金 + 冻结保证金（待成交订单）@yutiansut @quantaxis
            let position_margin = acc.get_margin();
            let frozen_margin = acc.get_frozen_margin();
            let total_margin = position_margin + frozen_margin;

            Some(AccountListItem {
                account_id: acc.account_cookie.clone(),
                account_name: account_name.clone(),
                owner_user_id,
                user_id: acc.account_cookie.clone(),
                user_name: account_name,
                account_type: format!("{:?}", account_type),
                balance: acc.get_balance(),
                available: acc.money,
                margin_used: total_margin,
                risk_ratio: acc.safe_risk_ratio(),
                created_at,
                status: super::account_admin::account_status_str(&acc.account_cookie)
                    .to_string(),
            })
        })
        .collect();

    // ✨ 先筛选,再分页 —— 这两个参数此前都被静默丢弃 @yutiansut @quantaxis
    //
    // `status` 在 `AccountListQuery` 里声明了,但 handler 函数体从头到尾没引用过;
    // `user_id` 连声明都没有。实测(修复前):`status=''/active/frozen/BOGUS`
    // 一律返回同样的 total —— 用户在界面上选「冻结」看到的还是全部账户。
    //
    // ⚠️ 顺序不能反:先分页再筛选会让每页条数不足、total 也不对。
    if let Some(st) = query.status.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        let want = st.to_ascii_lowercase();
        account_list.retain(|a| a.status == want);
    }
    if let Some(uid) = query.user_id.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        account_list.retain(|a| a.owner_user_id == uid);
    }

    // 排序在分页之前做 —— 说明见 apply_sort! 宏的文档。@yutiansut @quantaxis
    apply_sort!(account_list, query.sort_by, query.sort_order,
        str {
            "account_id"    => account_id,
            "account_name"  => account_name,
            "owner_user_id" => owner_user_id,
            "account_type"  => account_type,
            "status"        => status,
            "created_at"    => created_at,
        },
        num {
            "balance"     => balance,
            "available"   => available,
            "margin_used" => margin_used,
            "risk_ratio"  => risk_ratio,
        });

    // ✨ 全量合计必须在**分页之前**算,并且要跳过非有限值。@yutiansut @quantaxis
    //
    // 前端 `views/admin/accounts.vue:290` 的「总资金 / 可用资金」原先是对
    // **当前页**的 20 行求和,却与旁边取自 `pagination.total` 的「总账户数」并排显示。
    // 子代理翻页实测:第 1 页 4,232,981,133.15、第 2 页 7,365,634,616.77,
    // 而「总账户数」始终是 82 —— 用户会当成 82 个账户的总额。
    //
    // 跳过非有限值的原因:线上存在 `balance = 1e300` 的账户(测试污染),
    // float64 相加会**吸收**掉其余 81 个账户的钱(`1e300 + 4.2e9 == 1e300`),
    // 于是聚合值恒为 1e300。这里显式排除并回报排除条数,
    // 让前端能说明「合计已排除 N 个异常账户」而不是默默给出一个假数。
    // ⚠️ **`is_finite` 拦不住 1e300** —— 它是有限数。@yutiansut @quantaxis
    //
    // 我第一版只判 `is_finite`,子代理实测指出漏洞:线上那个污染账户的
    // `balance = 1e300` 会照样进合计,而 float64 相加会**吸收**掉其余 81 个账户
    // (`1e300 + 4.2e9 == 1e300`),聚合值恒为 1e300 —— 与不加闸门等价。
    // 必须同时判**量级**。阈值 1e15(`f64` 可精确表示整数的上限量级):
    // 本系统单账户余额不可能到这个量级,超过即判为坏数据。
    const MONEY_MAX: f64 = 1e15;
    let mut sum_balance = 0.0_f64;
    let mut sum_available = 0.0_f64;
    let mut excluded = 0_usize;
    for a in &account_list {
        let ok = a.balance.is_finite()
            && a.available.is_finite()
            && a.balance.abs() < MONEY_MAX
            && a.available.abs() < MONEY_MAX;
        if ok {
            sum_balance += a.balance;
            sum_available += a.available;
        } else {
            excluded += 1;
        }
    }

    // 分页处理
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(20).max(1);
    let total = account_list.len();

    let start = (page - 1) * page_size;
    let end = std::cmp::min(start + page_size, total);

    if start < total {
        account_list = account_list[start..end].to_vec();
    } else {
        account_list = vec![];
    }

    Ok(
        HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
            "total": total,
            "page": page,
            "page_size": page_size,
            "accounts": account_list,
            // 全量合计(已按当前 status/user_id 筛选,未受分页影响)
            "total_balance": sum_balance,
            "total_available": sum_available,
            "excluded_non_finite": excluded,
        }))),
    )
}

/// 获取账户详情
/// 未指定 page_size(或给了 0)时的默认每页条数 —— 该接口恒定分页,无全量分支
const DEFAULT_ORDER_PAGE_SIZE: usize = 100;

/// 账户详情的分页参数
///
/// 比 `handlers::OrderPageQuery` 多一个 `snapshot_ts` 锚点 —— 见下方 `paged_orders`
/// 处的说明。@yutiansut @quantaxis
#[derive(Debug, Deserialize)]
pub struct AccountDetailQuery {
    pub page: Option<usize>,
    pub page_size: Option<usize>,
    /// 翻页锚点:首页响应里的 `snapshot_ts`,后续页原样带回
    pub snapshot_ts: Option<i64>,
}

pub async fn get_account_detail(
    user_id: web::Path<String>,
    query: web::Query<AccountDetailQuery>,
    state: web::Data<ManagementAppState>,
) -> Result<HttpResponse> {
    match state.account_mgr.get_qifi_slice(&user_id) {
        Ok(qifi) => {
            // margin 口径对齐 @yutiansut @quantaxis
            //
            // qars `get_accountmessage()`(account.rs:328/421)填的是
            // `margin: self.get_margin()` —— **只有持仓保证金,不含冻结**。
            // 而 `/api/account/{id}`(handlers.rs)与 `/api/management/accounts`
            // 都用 `position_margin + frozen_margin`。三个接口同一账户报三个值:
            //   实测 MM_BETA_BID  前两者 180,470,303.65 / 本接口 79,061,560.45
            // 差额 101.4M 全是冻结部分。这里补上,与另两个接口一致;
            // 不去改 qars(共享库,其他项目依赖该口径)。
            let mut account_info = serde_json::to_value(&qifi.accounts).unwrap();
            if let Some(obj) = account_info.as_object_mut() {
                if let Ok(mut acc) = state.account_mgr.get_account(&user_id) {
                    let mut a = acc.write();
                    let m = a.get_margin() + a.get_frozen_margin();
                    obj.insert("margin".into(), serde_json::json!(m));
                    // ⚠️ 改了 margin 就必须同步 risk_ratio,否则同一份响应里
                    //    `risk_ratio != margin / balance`。@yutiansut @quantaxis
                    //
                    // `get_qifi_slice` 里的 `conform_qifi_static_balance` 已按
                    // 规范重算过 balance 与 risk_ratio,但那用的是 QIFI 自带的
                    // margin(不含冻结);这里把 margin 换成含冻结的口径后,
                    // risk_ratio 也要跟着换,三个字段才自洽。
                    if let Some(bal) = obj.get("balance").and_then(|v| v.as_f64()) {
                        let rr = if bal > 0.0 {
                            let r = m / bal;
                            if r.is_finite() { r } else { 0.0 }
                        } else {
                            0.0
                        };
                        obj.insert("risk_ratio".into(), serde_json::json!(rr));
                    }
                }
            }

            // ✨ 排序 + 分页 —— 这个接口原先把整个 `qifi.orders` 吐出来 @yutiansut @quantaxis
            //
            // 实测最大账户 ACC_496f8587… 返回 **65,460,145 字节 / 157,861 单**;
            // 前端 Element UI 2.15 的 el-table 不虚拟化,实测「点击 → 渲染完成
            // 12,509 ms / DOM 382,568 个节点」。同样的坑在 `/api/order/user/{id}`
            // 上踩过并已修(见 web/src/api/index.js:164 的注释),这个接口漏掉了,
            // 而且量大 26 倍 —— 典型的「做对了 N-1 处」(lessons.md L46)。
            //
            // ⚠️ 必须**先排序再分页**:`qifi.orders` 是 HashMap,迭代顺序不定,
            //    直接切片会导致翻页时内容随机跳动、且可能重复或漏掉订单。
            //    按 insert_date_time 倒序(新→旧),与前端展示习惯一致。
            // ⚡ 对**引用**排序,只把当前页转 JSON @yutiansut @quantaxis
            //
            // 原写法先把全部订单 `to_value` 再切片 —— 16.9 万单的账户每翻一页都要做
            // 16.9 万次 serde_json 转换(每次分配一个 Map),实测单页请求 ~1.14s,
            // 而返回的只有 100 条。这里改成排序 `&Order` 引用(只比 i64),
            // 分页之后才转换,转换量从 O(全部) 降到 O(每页)。
            //
            // 注:`get_qifi_slice()` 本身仍会克隆全部订单与成交(qars 的 `&mut self` API,
            // 且调用点持账户写锁)。这是**另一个尚未解决的问题** ——
            // 原注释写的「见下方 TODO」指向一个并不存在的 TODO,这里补上实测数据。
            //
            // 实测(2026-09-06,按快照体积分档):
            //   0 MB 账户   → 本端点 0.001s
            //   39.2 MB     → 0.040s
            //   92.4 MB     → **0.109s**   ← 与体积近似线性
            //
            // 更值得注意的是**尾部的全局影响** —— 「按账户的写锁只阻塞该账户」
            // 这个直觉是**错的**,别照它做容量规划。三步受控对照:
            //
            // ① 3 线程反复拉 92MB 账户的本端点,对 `/api/account/{id}` 的影响
            //    (各 200 次采样,ms):
            //              静默 p90 / p99      加压 p90 / p99
            //    同账户      0.53 / 0.73   →   52.68 / 140.90
            //    **别的账户**  0.36 / 0.50   →    1.72 / **201.69**   ← 无关账户也被拖到 200ms
            //    ⚠️ 只看**中位数**看不出来(同账户 ×1.1、对照 ×1.3),效应全在尾部。
            //
            // ② 分离「体积」与「并发」:同样 3 线程改压 0MB 账户 ——
            //    完成 8,942 次请求,第三方 p99 仅 2.04ms;
            //    压 92MB 账户只完成 67 次,第三方 p99 却是 206.76ms。
            //    **请求数少 133 倍,尾延迟差 100 倍 ⇒ 是克隆体积,不是并发本身。**
            //
            // ③ 机制:压 0MB 账户 45,052 次请求 RSS **+0.0 MB**;
            //    压 92MB 账户 187 次请求 RSS **+776.5 MB**(3 并发 × 92MB × JSON 展开)。
            //    即**大块瞬时分配**,不是锁竞争、也不是 worker 排队
            //    (worker 有 16 个、机器 96 核,3 个压力线程占不满)。
            //
            // (3 线程是人为构造的负载;但多个管理端同时查看同一个大账户就能达到。)
            let mut order_refs: Vec<_> = qifi.orders.values().collect();
            order_refs.sort_by(|a, b| b.insert_date_time.cmp(&a.insert_date_time));

            // ✨ 快照锚定 —— 纯 offset 分页在这里是错的 @yutiansut @quantaxis
            //
            // 排序是**倒序**(新→旧),新订单插在列表最前面,于是后续所有页整体右移。
            // 实测(受控对照):背靠背取 page1/page2 交集为 0;间隔 8 秒再取,
            // 交集恰好等于这 8 秒内新增的订单数(5 条)。做市商账户每秒新增 10+ 单,
            // 用户翻页**必然**看到重复订单,且 total 一路在涨。
            //
            // 解法:首次请求(不带 snapshot_ts)返回当前最新订单的时间戳作为锚点;
            // 后续页原样带回,服务端只在 `insert_date_time <= snapshot_ts` 的集合里分页。
            // 这样翻页看到的是一份**固定快照**,不受期间新增影响。
            let snapshot_ts = query.snapshot_ts.unwrap_or_else(|| {
                order_refs
                    .first()
                    .map(|o| o.insert_date_time)
                    .unwrap_or(i64::MAX)
            });
            if query.snapshot_ts.is_some() {
                order_refs.retain(|o| o.insert_date_time <= snapshot_ts);
            }

            let orders_total = order_refs.len();
            // ⚠️ 恒定分页 —— 不存在「不传参数就给全量」这条路 @yutiansut @quantaxis
            //
            // 上一版留了「两个参数都不传 = 全量」作为向后兼容。实测这条路是有害的:
            //   GET /api/management/account/{id}/detail            → 8,278,703 bytes(19,982 单)
            //   GET .../detail?page=1&page_size=20                 →    13,252 bytes
            // 单账户 8.28 MB,而账户侧订单是**只增不减**的当日全量,还会继续涨。
            //
            // 「兼容」的对象并不存在:仓内两个调用方
            // (web/src/views/admin/risk.vue:496、web/src/views/user/account-detail.vue:142)
            // 都恒定传 `{ page, page_size }`,从不走全量分支。
            // 真正会撞上的是 curl / 脚本 / 监控这类不传参的调用方 —— 它们恰恰最不该
            // 拿到 8 MB。总量由 `orders_total` 如实给出,要更多就显式加大 page_size。
            let size = query.page_size.filter(|s| *s > 0).unwrap_or(DEFAULT_ORDER_PAGE_SIZE);
            let page = query.page.unwrap_or(1).max(1);
            let start = (page - 1).saturating_mul(size);
            let page_slice: Vec<_> = order_refs.into_iter().skip(start).take(size).collect();
            // 只在这里转 JSON —— 分页时是 O(每页),不分页才退回 O(全部)
            let paged_orders: Vec<serde_json::Value> = page_slice
                .into_iter()
                .map(|o| serde_json::to_value(o).unwrap())
                .collect();

            let detail = AccountDetailResponse {
                account_info,
                // ⚠️ `qifi.positions` / `qifi.orders` 是 HashMap,`.iter()` 产出的是
                // `(&String, &V)` **元组** —— `to_value(元组)` 序列化成二元数组
                // `["IF2502", {…}]` 而不是对象。前端在数组元素上绑
                // `prop="instrument_id"` 之类,于是每一格都是空的
                // (实测 4 行持仓 × 5 列、6054 行订单 × 7 列全空)。
                // `serde_json::to_value` 接受任何 Serialize,类型系统拦不住这个。
                // 取 value 即可 —— QIFI 的 instrument_id / order_id 本来就在值里。
                // @yutiansut @quantaxis
                positions: qifi
                    .positions
                    .values()
                    .map(|p| serde_json::to_value(p).unwrap())
                    .collect(),
                orders: paged_orders,
                orders_total,
                snapshot_ts,
            };

            Ok(HttpResponse::Ok().json(ApiResponse::success(detail)))
        }
        Err(e) => Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(400, e.to_string()))),
    }
}

// ============================================================================
// 资金管理 API
// ============================================================================

/// 入金请求
#[derive(Debug, Deserialize)]
pub struct DepositRequest {
    pub user_id: String,
    pub amount: f64,
    pub method: Option<String>,
    pub remark: Option<String>,
}

/// 出金请求
#[derive(Debug, Deserialize)]
pub struct WithdrawRequest {
    pub user_id: String,
    pub amount: f64,
    pub method: Option<String>,
    pub bank_account: Option<String>,
}

/// 资金流水查询参数
#[derive(Debug, Deserialize)]
pub struct TransactionQuery {
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub limit: Option<usize>,
    // ✨ 补分页 —— 此前**没有**这两个字段 @yutiansut @quantaxis
    //
    // 前端 `admin/transactions.vue` 一直在传 `page`/`page_size`,
    // 而 actix 的 Query 提取器**静默丢弃未知字段** ⇒ 按用户查时分页完全失效,
    // 每次都返回全量。前端 `:279` 又把 `total` 设成当前数组长度,
    // 于是分页条显示的总数恰好等于已拿到的条数,**掩盖了失效**。
    pub page: Option<u32>,
    pub page_size: Option<u32>,
    /// 排序字段 / 方向 —— 服务端分页的表必须服务端排序,见 apply_sort! 宏
    pub sort_by: Option<String>,
    pub sort_order: Option<String>,
}

/// 入金
pub async fn deposit(
    req: web::Json<DepositRequest>,
    state: web::Data<ManagementAppState>,
) -> Result<HttpResponse> {
    match state.capital_mgr.deposit_with_record(
        req.user_id.clone(),
        req.amount,
        req.method.clone(),
        req.remark.clone(),
    ) {
        Ok(transaction) => Ok(HttpResponse::Ok().json(ApiResponse::success(transaction))),
        Err(e) => Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(400, e.to_string()))),
    }
}

/// 出金
pub async fn withdraw(
    req: web::Json<WithdrawRequest>,
    state: web::Data<ManagementAppState>,
) -> Result<HttpResponse> {
    let remark = req
        .bank_account
        .as_ref()
        .map(|acc| format!("提现至银行账户: {}", acc));

    match state.capital_mgr.withdraw_with_record(
        req.user_id.clone(),
        req.amount,
        req.method.clone(),
        remark,
    ) {
        Ok(transaction) => Ok(HttpResponse::Ok().json(ApiResponse::success(transaction))),
        Err(e) => Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(400, e.to_string()))),
    }
}

/// 查询资金流水
///
/// 合并查询 CapitalManager 和 TransferStore 的记录 @yutiansut @quantaxis
pub async fn get_transactions(
    user_id: web::Path<String>,
    query: web::Query<TransactionQuery>,
    state: web::Data<ManagementAppState>,
) -> Result<HttpResponse> {
    use crate::exchange::{TransactionStatus, TransactionType};
    use crate::service::http::transfer::TRANSFER_STORE;

    // 1. 从 CapitalManager 获取记录
    let mut transactions = if let (Some(start), Some(end)) = (&query.start_date, &query.end_date) {
        state
            .capital_mgr
            .get_transactions_by_date_range(&user_id, start, end)
    } else if let Some(limit) = query.limit {
        state.capital_mgr.get_recent_transactions(&user_id, limit)
    } else {
        state.capital_mgr.get_transactions(&user_id)
    };

    // 2. 从 TransferStore 获取记录（通过 user 的所有账户）
    let accounts = state.account_mgr.get_accounts_by_user(&user_id);
    for account in accounts {
        let account_id = account.read().account_cookie.clone();
        let (transfer_records, _) = TRANSFER_STORE.get_records(&account_id, None, None, None, None);

        // 转换 TransferRecord 为 FundTransaction
        for record in transfer_records {
            let transaction = FundTransaction {
                transaction_id: record.id,
                user_id: user_id.to_string(),
                transaction_type: if record.amount >= 0.0 {
                    TransactionType::Deposit
                } else {
                    TransactionType::Withdrawal
                },
                amount: record.amount.abs(),
                balance_before: record.balance_before,
                balance_after: record.balance_after,
                status: if record.error_id == 0 {
                    TransactionStatus::Completed
                } else {
                    TransactionStatus::Failed
                },
                method: Some(record.bank_name),
                remark: Some(record.error_msg),
                created_at: chrono::DateTime::from_timestamp_millis(record.datetime)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                    .unwrap_or_default(),
                updated_at: chrono::DateTime::from_timestamp_millis(record.datetime)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                    .unwrap_or_default(),
            };
            // ⚠️ 转账记录必须走**同一套日期筛选** @yutiansut @quantaxis
            //
            // 上面 :418 的 `TRANSFER_STORE.get_records(&account_id, None, None, None, None)`
            // **不带任何日期条件**,于是这些记录被无条件 push,
            // 与前面按 start_date/end_date 过滤过的资金流水**混在一起** ——
            // 用户选了日期范围,结果里却混着范围外的转账。
            //
            // 这个缺陷此前休眠(前端日期选择器没接上),本会话接上后才激活。
            // 口径与 `capital_mgr.rs:281` 一致:只比日期部分(created_at 是
            // "%Y-%m-%d %H:%M:%S",查询参数是 "%Y-%m-%d")。
            let in_range = {
                let day = if transaction.created_at.len() >= 10 {
                    &transaction.created_at[..10]
                } else {
                    transaction.created_at.as_str()
                };
                let ge = query.start_date.as_deref().map_or(true, |s| day >= s);
                let le = query.end_date.as_deref().map_or(true, |e| day <= e);
                ge && le
            };
            if in_range {
                transactions.push(transaction);
            }
        }
    }

    // 3. 按时间排序（最新的在前）
    transactions.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    // 用户指定排序时覆盖默认时序 —— 必须在 limit / 分页之前。@yutiansut @quantaxis
    apply_sort!(transactions, query.sort_by, query.sort_order,
        str {
            "transaction_id" => transaction_id,
            "user_id"        => user_id,
            "created_at"     => created_at,
            "updated_at"     => updated_at,
        },
        num {
            "amount"         => amount,
            "balance_before" => balance_before,
            "balance_after"  => balance_after,
        },
        dbg {
            "transaction_type" => transaction_type,
            "status"           => status,
        });

    // 4. 如果有 limit，截取
    if let Some(limit) = query.limit {
        transactions.truncate(limit);
    }

    // 5. 分页 @yutiansut @quantaxis
    //
    // ⚠️ 响应形状:此前本端点返回**裸数组**,而全局端点 `get_all_transactions`
    //    返回 `{records, total, page, page_size}` —— 同一个页面的两条路径形状不同,
    //    前端 `:278` 用 `Array.isArray(result) ? result : result.records` 兼容两者。
    //    这里**保持返回裸数组**以免打断该兼容逻辑,只在服务端做切片;
    //    total 由前端按已有逻辑取数组长度(在只有一页时是对的)。
    //    要彻底统一形状是独立改动,需同时改前端 —— 不在本次范围。
    let total = transactions.len();
    if let (Some(page), Some(size)) = (query.page, query.page_size) {
        let size = size.max(1) as usize;
        let start = (page.max(1) as usize - 1) * size;
        if start >= total {
            transactions.clear();
        } else {
            let end = (start + size).min(total);
            transactions = transactions[start..end].to_vec();
        }
    }

    // 用响应头带总数 —— 既补上分页所需的信息,又不改动裸数组的响应体
    // @yutiansut @quantaxis
    //
    // 本端点返回裸数组(见上面的说明),**没有 total 字段**,
    // 于是前端 `admin/transactions.vue:279` 只能 `total = 数组长度` ——
    // 分页条永远显示「总数 = 当前页条数」,把分页失效掩盖成「看起来正常」。
    // 改响应体形状会打断前端的 `Array.isArray(...)` 兼容分支,
    // 用响应头是最小改动:老消费者不受影响,新消费者读 `X-Total-Count`。
    Ok(HttpResponse::Ok()
        .insert_header(("X-Total-Count", total.to_string()))
        .json(ApiResponse::success(transactions)))
}

/// 查询所有账户的资金流水（不需要指定用户）
///
/// @yutiansut @quantaxis
/// 管理端默认加载全部流水，支持分页和筛选
pub async fn get_all_transactions(
    query: web::Query<AllTransactionsQuery>,
    state: web::Data<ManagementAppState>,
) -> Result<HttpResponse> {
    use crate::exchange::{TransactionStatus, TransactionType};
    use crate::service::http::transfer::TRANSFER_STORE;
    use rayon::prelude::*;

    // 1. 并行遍历所有账户，从 TransferStore 获取流水
    let all_accounts = state.account_mgr.get_all_accounts();

    // 先收集账户信息（需要释放锁）
    let account_infos: Vec<(String, String)> = all_accounts
        .iter()
        .map(|account| {
            let acc = account.read();
            (acc.account_cookie.clone(), acc.portfolio_cookie.clone())
        })
        .collect();

    // 使用 rayon 并行处理每个账户的 TransferStore 记录
    let transfer_transactions: Vec<FundTransaction> = account_infos
        .par_iter()
        .flat_map(|(account_id, user_id)| {
            // 全所聚合要的是**全部**记录 —— 不能用 get_records(.., None, ..),
            // 它的 page_size 默认 20 且封顶 100(见 transfer.rs 的说明)。
            // @yutiansut @quantaxis
            let transfer_records = TRANSFER_STORE.get_all_records(account_id);

            transfer_records.into_iter().map(|record| {
                FundTransaction {
                    transaction_id: record.id,
                    user_id: user_id.clone(),
                    transaction_type: if record.amount >= 0.0 {
                        TransactionType::Deposit
                    } else {
                        TransactionType::Withdrawal
                    },
                    amount: record.amount.abs(),
                    balance_before: record.balance_before,
                    balance_after: record.balance_after,
                    status: if record.error_id == 0 {
                        TransactionStatus::Completed
                    } else {
                        TransactionStatus::Failed
                    },
                    method: Some(record.bank_name),
                    remark: Some(format!("{} (账户: {})", record.error_msg, account_id)),
                    created_at: chrono::DateTime::from_timestamp_millis(record.datetime)
                        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                        .unwrap_or_default(),
                    updated_at: chrono::DateTime::from_timestamp_millis(record.datetime)
                        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                        .unwrap_or_default(),
                }
            }).collect::<Vec<_>>()
        })
        .collect();

    // 2. 从 CapitalManager 获取所有用户的流水（已经使用rayon）
    let capital_transactions = state.capital_mgr.get_all_transactions();

    // 3. 合并流水
    let mut all_transactions = transfer_transactions;
    all_transactions.extend(capital_transactions);

    // 4. 使用 rayon 并行排序（最新的在前）
    all_transactions.par_sort_by(|a, b| b.created_at.cmp(&a.created_at));

    // 用户指定排序时覆盖默认时序 —— 必须在筛选与分页之前。@yutiansut @quantaxis
    apply_sort!(all_transactions, query.sort_by, query.sort_order,
        str {
            "transaction_id" => transaction_id,
            "user_id"        => user_id,
            "created_at"     => created_at,
            "updated_at"     => updated_at,
        },
        num {
            "amount"         => amount,
            "balance_before" => balance_before,
            "balance_after"  => balance_after,
        },
        dbg {
            "transaction_type" => transaction_type,
            "status"           => status,
        });

    // 5. 筛选类型
    if let Some(ref tx_type) = query.transaction_type {
        let tx_type_clone = tx_type.clone();
        all_transactions = all_transactions
            .into_par_iter()
            .filter(|t| {
                match tx_type_clone.as_str() {
                    "deposit" => matches!(t.transaction_type, TransactionType::Deposit),
                    "withdrawal" => matches!(t.transaction_type, TransactionType::Withdrawal),
                    _ => true,
                }
            })
            .collect();
    }

    // 6. 分页
    let page = query.page.unwrap_or(1).max(1) as usize;
    let page_size = query.page_size.unwrap_or(20).min(100) as usize;
    let total = all_transactions.len();
    let start = (page - 1) * page_size;

    let paged: Vec<_> = all_transactions
        .into_iter()
        .skip(start)
        .take(page_size)
        .collect();

    Ok(HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
        "records": paged,
        "total": total,
        "page": page,
        "page_size": page_size
    }))))
}

/// 全部流水查询参数
#[derive(Debug, Deserialize)]
pub struct AllTransactionsQuery {
    pub transaction_type: Option<String>,  // deposit / withdrawal
    pub page: Option<u32>,
    pub page_size: Option<u32>,
    /// 排序字段 / 方向 —— 服务端分页的表必须服务端排序,见 apply_sort! 宏
    pub sort_by: Option<String>,
    pub sort_order: Option<String>,
}

// ============================================================================
// 风控监控 API
// ============================================================================

/// 风险监控查询参数
#[derive(Debug, Deserialize)]
pub struct RiskQuery {
    pub risk_level: Option<String>,
}

/// 强平记录查询参数
#[derive(Debug, Deserialize)]
pub struct LiquidationQuery {
    pub start_date: Option<String>,
    pub end_date: Option<String>,
}

/// 强平请求
#[derive(Debug, Deserialize)]
pub struct ForceLiquidateRequest {
    pub account_id: String,
    pub reason: Option<String>,
}

/// 获取风险账户列表
pub async fn get_risk_accounts(
    query: web::Query<RiskQuery>,
    state: web::Data<ManagementAppState>,
) -> Result<HttpResponse> {
    let risk_level_filter = query
        .risk_level
        .as_ref()
        .and_then(|level| match level.as_str() {
            "low" => Some(RiskLevel::Low),
            "medium" => Some(RiskLevel::Medium),
            "high" => Some(RiskLevel::High),
            "critical" => Some(RiskLevel::Critical),
            _ => None,
        });

    let accounts = state.risk_monitor.get_risk_accounts(risk_level_filter);
    Ok(HttpResponse::Ok().json(ApiResponse::success(accounts)))
}

/// 获取保证金监控汇总
pub async fn get_margin_summary(state: web::Data<ManagementAppState>) -> Result<HttpResponse> {
    let summary = state.risk_monitor.get_margin_summary();
    Ok(HttpResponse::Ok().json(ApiResponse::success(summary)))
}

/// 获取强平记录
pub async fn get_liquidation_records(
    query: web::Query<LiquidationQuery>,
    state: web::Data<ManagementAppState>,
) -> Result<HttpResponse> {
    let records = if let (Some(start), Some(end)) = (&query.start_date, &query.end_date) {
        state
            .risk_monitor
            .get_liquidation_records_by_date_range(start, end)
    } else {
        state.risk_monitor.get_all_liquidation_records()
    };

    Ok(HttpResponse::Ok().json(ApiResponse::success(records)))
}

/// 触发强平
pub async fn force_liquidate_account(
    req: web::Json<ForceLiquidateRequest>,
    state: web::Data<ManagementAppState>,
) -> Result<HttpResponse> {
    match state
        .settlement_engine
        .force_liquidate_account(&req.account_id, req.reason.clone())
    {
        Ok(result) => Ok(HttpResponse::Ok().json(ApiResponse::success(result))),
        Err(e) => Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(
            400,
            format!("Force liquidation failed: {}", e),
        ))),
    }
}

// ============================================================================
// 全市场订单/成交查询 API (管理端)
// ============================================================================

/// 全市场订单查询参数
/// @yutiansut @quantaxis
#[derive(Debug, Deserialize)]
pub struct AllOrdersQuery {
    pub page: Option<usize>,
    pub page_size: Option<usize>,
    pub status: Option<String>,
    pub instrument_id: Option<String>,
}

/// 全市场成交查询参数
/// @yutiansut @quantaxis
#[derive(Debug, Deserialize)]
pub struct AllTradesQuery {
    pub page: Option<usize>,
    pub page_size: Option<usize>,
    pub instrument_id: Option<String>,
}

/// 订单列表项 (管理端)
/// @yutiansut @quantaxis
#[derive(Debug, Clone, Serialize)]
pub struct OrderListItem {
    pub order_id: String,
    pub user_id: String,
    pub account_id: String,
    pub instrument_id: String,
    pub direction: String,
    pub offset: String,
    pub price: f64,
    pub volume: f64,
    pub filled_volume: f64,
    pub status: String,
    pub submit_time: i64,
    pub update_time: i64,
}

/// 成交列表项 (管理端)
/// @yutiansut @quantaxis
#[derive(Debug, Clone, Serialize)]
pub struct TradeListItem {
    pub trade_id: String,
    pub instrument_id: String,
    /// 买方账户。**`Option` 而不是 `String`** —— 从账户 QIFI 的 `dailytrades`
    /// 补齐的成交是**单边**的(QIFI Trade 只记本方 `user_id` 与 `direction`,
    /// 规范里没有对手方字段),此时另一侧**不存在**。
    /// 用空串填会让前端显示成一个没有名字的账户,那是**编造** ——
    /// 与 `handlers.rs` 的 `is_taker` 是同一条先例:前端须按三态渲染,null → `--`。
    /// @yutiansut @quantaxis
    pub buy_user_id: Option<String>,
    pub sell_user_id: Option<String>,
    pub buy_order_id: Option<String>,
    pub sell_order_id: Option<String>,
    pub price: f64,
    pub volume: f64,
    pub timestamp: i64,
    pub trading_day: String,
}

/// 获取全市场所有订单 (管理端)
/// @yutiansut @quantaxis
pub async fn list_all_orders(
    query: web::Query<AllOrdersQuery>,
    state: web::Data<ManagementAppState>,
) -> Result<HttpResponse> {
    let all_orders = state.order_router.get_all_orders();

    // 转换为列表项
    // QIFI Order 字段: user_id = account_cookie, limit_price, volume_orign
    // @yutiansut @quantaxis
    let mut order_list: Vec<OrderListItem> = all_orders
        .into_iter()
        .map(|(order_id, order, status, submit_time, update_time, filled_volume)| {
            OrderListItem {
                order_id,
                user_id: order.user_id.clone(),     // QIFI: user_id 即账户ID
                account_id: order.user_id.clone(),  // QIFI: 同 user_id
                instrument_id: order.instrument_id.clone(),
                direction: order.direction.clone(),  // QIFI: 已是 String
                offset: order.offset.clone(),        // QIFI: 已是 String
                price: order.limit_price,            // QIFI: limit_price
                volume: order.volume_orign,          // QIFI: volume_orign
                filled_volume,
                status: format!("{:?}", status),
                submit_time,
                update_time,
            }
        })
        .collect();

    // 过滤: 状态
    if let Some(ref status_filter) = query.status {
        order_list.retain(|o| o.status.to_lowercase().contains(&status_filter.to_lowercase()));
    }

    // 过滤: 合约
    if let Some(ref inst_filter) = query.instrument_id {
        order_list.retain(|o| o.instrument_id.contains(inst_filter));
    }

    // 按更新时间降序排序
    order_list.sort_by(|a, b| b.update_time.cmp(&a.update_time));

    // 分页处理
    // ⚠️ `.max(1)` 不能省 —— `page=0` 会让下面的 `(page - 1) * page_size`
    // 在 usize 上**下溢**成天文数字,`.skip()` 直接跳过全部数据,
    // 返回 `total=677` 但 **0 条记录**。@yutiansut @quantaxis
    //
    // 实测(2026-09-05,回验代理发现):
    //   /api/management/orders?page=0  → total=677 / 返回 0 条  ❌
    //   /api/management/trades?page=0  → total=5   / 返回 0 条  ❌
    //   /api/management/accounts?page=0 → 正常     ← 那处 :157 有 .max(1)
    // 同文件 :157 / :291 / :614 三处都做了 clamp,只有这里和下面漏了。
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(50).max(1).min(1000); // ✨ 封顶 1000 防大数据集 DoS(前端最大只请求 200)。@yutiansut @quantaxis
    let total = order_list.len();

    let start = (page - 1) * page_size;
    let end = std::cmp::min(start + page_size, total);

    let paged_list = if start < total {
        order_list[start..end].to_vec()
    } else {
        vec![]
    };

    Ok(
        HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
            "total": total,
            "page": page,
            "page_size": page_size,
            "orders": paged_list,
        }))),
    )
}

/// 获取全市场所有成交 (管理端)
/// @yutiansut @quantaxis
pub async fn list_all_trades(
    query: web::Query<AllTradesQuery>,
    state: web::Data<ManagementAppState>,
) -> Result<HttpResponse> {
    // ✨ 数据源:账户 QIFI 的 `dailytrades` **为主**,TradeRecorder 为补充。
    // @yutiansut @quantaxis
    //
    // 原实现只读 `trade_recorder` —— 那是**执行态**组件,`new()` 建空表、
    // 全仓无 restore 入口,**每次重启归零**。
    // 实测(2026-09-06,跨页面对拍):
    //   `/dashboard` 「总成交数」151,699  ← `/api/monitoring/system`,遍历账户 dailytrades
    //   `/market-overview` 成交记录   5   ← 本接口,只读 trade_recorder
    // 两者都是**全系统口径的「所有成交」**,相差 **30,340 倍**,同屏自相矛盾。
    //
    // `handlers.rs:1446` 的 `/api/trades/account/{id}` 早就改成了「QIFI 为主 +
    // TradeRecorder 补充 + 按 trade_id 去重」,**本函数当时漏改了** ——
    // 这里补上同一套做法。
    //
    // ⚠️ QIFI Trade 是**单边**的(只有本方 `user_id` 与 `direction`),
    // 所以对手方字段留 `None`,由前端渲染成 `--`,**不编造**。
    let recorder_trades = state.trade_recorder.get_all_trades();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut trade_list: Vec<TradeListItem> = Vec::new();

    // ⚠️ **读锁 + 直接读 `dailytrades`,不要走 `get_qifi_slice()`**。@yutiansut @quantaxis
    //
    // 第一版我在这里对**每个账户**调 `get_qifi_slice()`,那是个严重的性能错误:
    // 它取**写锁**、且克隆整份 QIFI(订单 + 成交 + 持仓)。实测单个 92MB 账户
    // 一次调用 109ms、3 并发瞬时 RSS +776MB;全库 82 个账户合计 319MB 快照,
    // 每次请求都要克隆一遍 —— 这条接口会变成整个进程的尾延迟源
    // (受控对照见 `management.rs` 上方 `get_account_detail` 的实测数据)。
    //
    // `dailytrades` 是 qars `QA_Account` 的**公开字段**,`monitoring.rs:405` 早就是
    // 「`account.read()` + 直接遍历」——照抄它:只拿读锁、只读成交、不克隆其余。
    //
    // 交易日用 `TradingDayResolver`(handlers.rs)按**每笔成交自己的时间戳**解析,
    // 而不是取账户当前交易日 —— 后者会把全部历史成交都标成今天(该缺陷已在
    // `/api/trades/account/{id}` 上修过,这里不重蹈)。
    let mut resolver = super::handlers::TradingDayResolver::new();
    for acc in state.account_mgr.get_all_accounts() {
        let a = acc.read();
        let cookie = a.account_cookie.clone();
        for t in a.dailytrades.values() {
            if !seen.insert(t.trade_id.clone()) {
                continue;
            }
            let is_buy = t.direction.eq_ignore_ascii_case("BUY");
            let day = resolver.resolve(t.trade_date_time).unwrap_or_default();
            trade_list.push(TradeListItem {
                trade_id: t.trade_id.clone(),
                instrument_id: t.instrument_id.clone(),
                buy_user_id: if is_buy { Some(cookie.clone()) } else { None },
                sell_user_id: if is_buy { None } else { Some(cookie.clone()) },
                buy_order_id: if is_buy { Some(t.order_id.clone()) } else { None },
                sell_order_id: if is_buy { None } else { Some(t.order_id.clone()) },
                price: t.price,
                volume: t.volume,
                timestamp: t.trade_date_time,
                trading_day: day,
            });
        }
    }

    // TradeRecorder 里有**本次运行期**的最新成交(可能还没进 60 秒快照),
    // 且它有对手方信息 —— 已存在的 trade_id 不覆盖(QIFI 是权威的持久化来源)。
    for t in recorder_trades {
        if !seen.insert(t.trade_id.clone()) {
            continue;
        }
        trade_list.push(TradeListItem {
            trade_id: t.trade_id,
            instrument_id: t.instrument_id,
            buy_user_id: Some(t.buy_user_id),
            sell_user_id: Some(t.sell_user_id),
            buy_order_id: Some(t.buy_order_id),
            sell_order_id: Some(t.sell_order_id),
            price: t.price,
            volume: t.volume,
            timestamp: t.timestamp,
            trading_day: t.trading_day,
        });
    }

    // 过滤: 合约
    if let Some(ref inst_filter) = query.instrument_id {
        trade_list.retain(|t| t.instrument_id.contains(inst_filter));
    }

    // 按时间降序排序
    trade_list.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    // 分页处理
    // ⚠️ `.max(1)` 不能省 —— `page=0` 会让下面的 `(page - 1) * page_size`
    // 在 usize 上**下溢**成天文数字,`.skip()` 直接跳过全部数据,
    // 返回 `total=677` 但 **0 条记录**。@yutiansut @quantaxis
    //
    // 实测(2026-09-05,回验代理发现):
    //   /api/management/orders?page=0  → total=677 / 返回 0 条  ❌
    //   /api/management/trades?page=0  → total=5   / 返回 0 条  ❌
    //   /api/management/accounts?page=0 → 正常     ← 那处 :157 有 .max(1)
    // 同文件 :157 / :291 / :614 三处都做了 clamp,只有这里和下面漏了。
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(50).max(1).min(1000); // ✨ 封顶 1000 防大数据集 DoS(前端最大只请求 200)。@yutiansut @quantaxis
    let total = trade_list.len();

    let start = (page - 1) * page_size;
    let end = std::cmp::min(start + page_size, total);

    let paged_list = if start < total {
        trade_list[start..end].to_vec()
    } else {
        vec![]
    };

    Ok(
        HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
            "total": total,
            "page": page,
            "page_size": page_size,
            "trades": paged_list,
        }))),
    )
}

#[cfg(test)]
mod sort_tests {
    //! `apply_sort!` 的正确性闸门。@yutiansut @quantaxis
    //!
    //! 这个宏排的是**金融列表**且要在分页之前跑全量,一旦 panic 会打掉整个请求。
    //! 本库确有非有限值:空账户的 `risk_ratio = margin/balance = 0/0 = NaN`,
    //! 以及探针留下的 `1e300` 余额。所以 NaN 分支必须有测试锁死 ——
    //! 用 `partial_cmp().unwrap()` 的写法能编译、能过 code review、
    //! 在**没有 NaN 的测试数据上也能通过**,只在生产炸。

    #[derive(Debug, Clone, PartialEq)]
    struct Row {
        id: String,
        amount: f64,
    }

    fn rows() -> Vec<Row> {
        vec![
            Row { id: "c".into(), amount: 3.0 },
            Row { id: "a".into(), amount: f64::NAN },
            Row { id: "d".into(), amount: 1e300 },
            Row { id: "b".into(), amount: -5.0 },
        ]
    }

    fn sort(key: &str, order: &str) -> Vec<Row> {
        let mut v = rows();
        let k = Some(key.to_string());
        let o = Some(order.to_string());
        apply_sort!(v, k, o, str { "id" => id }, num { "amount" => amount });
        v
    }

    #[test]
    fn string_asc_and_desc() {
        assert_eq!(
            sort("id", "asc").iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            vec!["a", "b", "c", "d"]
        );
        assert_eq!(
            sort("id", "desc").iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            vec!["d", "c", "b", "a"]
        );
        // 前端传的是 Element UI 的 ascending/descending,也要认
        assert_eq!(
            sort("id", "descending").iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            vec!["d", "c", "b", "a"]
        );
    }

    #[test]
    fn float_sort_with_nan_does_not_panic() {
        // total_cmp 把 NaN 排在正数之后(正 NaN 视为最大),关键是**不 panic、且确定性**
        let asc = sort("amount", "asc");
        assert_eq!(asc.len(), 4, "不能丢行");
        assert_eq!(asc[0].amount, -5.0, "最小的必须在最前");
        assert_eq!(asc[1].amount, 3.0);
        assert_eq!(asc[2].amount, 1e300);
        assert!(asc[3].amount.is_nan(), "NaN 落在末尾");

        // 反向必须严格是正向的逆序 —— 这是 reverse() 实现的不变量
        let desc = sort("amount", "desc");
        let mut expect: Vec<f64> = asc.iter().map(|r| r.amount).collect();
        expect.reverse();
        for (got, want) in desc.iter().map(|r| r.amount).zip(expect) {
            assert_eq!(got.is_nan(), want.is_nan());
            if !got.is_nan() {
                assert_eq!(got, want);
            }
        }
    }

    /// 按 id 取序列 —— **不能直接 `assert_eq!` 整行**:`Row` 含 `f64::NAN`,
    /// 而 `NaN != NaN`,整行比较恒为 false。第一版测试就栽在这里 ——
    /// 与生产代码要防的是同一个坑,顺手当反面教材留着。@yutiansut @quantaxis
    fn ids(v: &[Row]) -> Vec<&str> {
        v.iter().map(|r| r.id.as_str()).collect()
    }

    #[test]
    fn unknown_key_and_empty_key_are_noops() {
        let orig = rows();
        // 负向对照:未知字段不能报错、不能改变顺序(前端加列时不该 500)
        assert_eq!(ids(&sort("NOSUCH", "asc")), ids(&orig));
        assert_eq!(ids(&sort("", "asc")), ids(&orig));
        assert_eq!(ids(&sort("   ", "desc")), ids(&orig), "全空白等同于没传");

        // 负向对照:不传 sort_by 时整段不执行
        let mut v = rows();
        let k: Option<String> = None;
        let o: Option<String> = None;
        apply_sort!(v, k, o, str { "id" => id }, num { "amount" => amount });
        assert_eq!(ids(&v), ids(&orig), "不传排序参数必须保持原顺序");
    }

    #[test]
    fn sort_is_stable_across_repeated_calls() {
        // 同样输入排两次必须一致 —— NaN 参与比较时最容易在这里露馅
        assert_eq!(
            sort("amount", "asc").iter().map(|r| r.id.clone()).collect::<Vec<_>>(),
            sort("amount", "asc").iter().map(|r| r.id.clone()).collect::<Vec<_>>()
        );
    }
}

#[cfg(test)]
mod money_guard_tests {
    /// 合计的排除判据:非有限**或**量级超限。@yutiansut @quantaxis
    ///
    /// 这个测试存在的理由:我第一版只判 `is_finite`,而
    /// **`(1e300_f64).is_finite() == true`** —— 闸门形同虚设。
    fn accepted(balance: f64, available: f64) -> bool {
        const MONEY_MAX: f64 = 1e15;
        balance.is_finite()
            && available.is_finite()
            && balance.abs() < MONEY_MAX
            && available.abs() < MONEY_MAX
    }

    #[test]
    fn rejects_huge_but_finite() {
        assert!(1e300_f64.is_finite(), "前提:1e300 是有限数,所以 is_finite 挡不住它");
        assert!(!accepted(1e300, 1.0), "1e300 必须被排除");
        assert!(!accepted(1.0, -1e300), "负的超限值同样排除");
    }

    #[test]
    fn rejects_non_finite() {
        assert!(!accepted(f64::NAN, 1.0));
        assert!(!accepted(f64::INFINITY, 1.0));
        assert!(!accepted(1.0, f64::NEG_INFINITY));
    }

    /// **负对照**:正常量级必须被接受 —— 否则一个「全部拒绝」的实现也能通过上面两个。
    #[test]
    fn accepts_normal_magnitudes() {
        assert!(accepted(2_132_919_328.77, 2_092_538_943.63), "20 亿级是正常业务值");
        assert!(accepted(0.0, 0.0), "空账户");
        assert!(accepted(-1000.0, -1000.0), "负余额是坏数据但不是坏数值,仍应计入");
        assert!(accepted(9.99e14, 9.99e14), "刚好低于阈值应接受");
    }

    /// float64 吸收:证明「不排除」等价于「合计被单个值吞掉」。
    #[test]
    fn demonstrates_float_absorption() {
        let polluted = 1e300_f64;
        let real_total = 4_232_981_133.15_f64;
        assert_eq!(polluted + real_total, polluted, "其余账户的钱被完全吸收");
    }
}
