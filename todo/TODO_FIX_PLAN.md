# QAExchange 前后端 TODO/Mock 修复清单（更新：2025-10-08）

## 概览
- **后端待办**：2 项
- **前端待办**：7 项
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
   - 已切换到实时 API，支持账户切换、时间区间过滤与 CSV 导出。

2. **平仓表单 (`web/src/views/trade/components/CloseForm.vue:118-154`)**  
   - `availableVolume` 仍写死为 10，需要从父组件传入持仓信息或实时查询；提交前做超量校验。

3. **持仓页实时价格 (`web/src/views/positions/index.vue:234-268`)**  
   - `last_price` 使用持仓成本作为占位，需订阅行情或调用最新价 API，并根据真实价格刷新浮动盈亏。

4. **风险监控页 (`web/src/views/admin/risk.vue:233-420`)**  
   - 补齐持仓/订单明细展示区域。  
   - 在强平按钮中调用新的后端 API，并给出操作结果提示。

5. **结算管理页 (`web/src/views/admin/settlement.vue:191,374,435`)**  
   - 实现「结算趋势图表」「CSV/Excel 导入」及「历史详情弹窗」功能，替换当前的 TODO/提示文案。

6. **资金流水页导出 (`web/src/views/admin/transactions.vue:260-300`)**  
   - 完成 Excel 导出，实现列选择 & 文件命名。

7. **用户账户列表 (`web/src/views/user/my-accounts.vue`)**  
   - 调整“查看账户”跳转逻辑，联动新的账户详情页组件（当前路由指向占位页面，需要实际页面落地）。

---

## 三、API 缺口

1. ✅ `GET /api/account/{user_id}/equity-curve` —— 已返回账户权益曲线、收益率与回撤统计，前端可直接消费。  
2. ✅ `POST /api/management/force-liquidate` —— 触发强平任务，返回批次状态，用于风险页按钮。

---

## 四、执行建议
- **本周优先**：完成强平链路（后端 2 项 + 风险页改造），确保风控功能闭环。  
- **次优先**：上线资金曲线 API + 页面、平仓表单可用量，避免交易端继续依赖假数据。  
- **随后**：处理导入/导出与图表展示，提升运营工具可用性。
