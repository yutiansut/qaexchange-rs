# QAExchange 前后端 TODO/Mock 修复清单（更新：2025-11-24）

## 概览
- **后端待办**：0 项（阶段性目标已完成，用于记录）
- **前端待办**：0 项（下列条目均已在代码中落地）
- **API 缺口**：2 项

---

## 一、后端 TODO

1. ✅ **结算引擎强平逻辑 (`src/exchange/settlement.rs`)**  
   - 已改为通过 `submit_force_order` 提交真实撤单，并将执行结果写入 WAL/风险监控。  
   - `SettlementEngine` 现在跟踪账户结算历史与强平记录，可供查询。

2. ✅ **风险控制强平 API**  
   - 新增 `POST /api/management/force-liquidate`，可从风险监控页触发强平并返回具体订单状态。

---

## 二、前端 TODO（含 Mock/占位逻辑）

1. ✅ **资金曲线页 (`web/src/views/user/account-curve.vue`)**  
   - 仍保持实时 API 对接，无新增动作。

2. ✅ **平仓表单 (`web/src/views/trade/components/CloseForm.vue`)**  
   - `availableVolume` 现由 `queryAccountPosition` 结果驱动，`calculateAvailableVolume()` 根据方向动态计算（见第 206-272 行），并在下单前校验可平手数。

3. ✅ **持仓页实时价格 (`web/src/views/positions/index.vue`)**  
   - `loadPositions()` 会为每个合约调用 `getTick`，由 `priceMap` 驱动 `last_price` 与浮动盈亏（第 214-266 行），已不再使用成本价占位。

4. ✅ **风险监控页 (`web/src/views/admin/risk.vue`)**  
   - 账户详情页包含持仓/订单两个 Tab，`viewAccountDetail()` 直接调用 `getAccountDetail`，强平按钮触发 `forceLiquidateAccount` 并提示结果（第 420-520 行）。

5. ✅ **结算管理页 (`web/src/views/admin/settlement.vue`)**  
   - 已提供趋势图 (`initChart`)、CSV 导入（`handlePriceFileUpload`）、历史详情弹窗以及批量 `batchSetSettlementPrices` + `executeSettlement` 流程。

6. ✅ **资金流水页导出 (`web/src/views/admin/transactions.vue`)**  
   - `exportData()` 根据当前筛选结果构造 CSV 并下载，支持格式化字段（第 262-314 行）。

7. ✅ **用户账户列表 (`web/src/views/user/my-accounts.vue`)**  
   - “查看详情” 调用 `handleViewAccount`，通过路由跳转到 `AccountDetail` 页面（第 152、412-419 行），并与 `web/src/views/user/account-detail.vue` 配合展示完整信息。

---

## 三、API 缺口

1. ✅ `GET /api/account/{user_id}/equity-curve` —— 已返回账户权益曲线、收益率与回撤统计，前端可直接消费。  
2. ✅ `POST /api/management/force-liquidate` —— 触发强平任务，返回批次状态，用于风险页按钮。

---

## 四、执行建议
- 当前阶段主要关注新需求或性能瓶颈，本清单保留作为已交付功能的记录。
