#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
双边交易测试脚本
@yutiansut @quantaxis

测试场景：
1. 创建账户 A 和 B
2. A 提交买入开仓订单
3. B 提交卖出开仓订单（相同价格，触发撮合）
4. 验证 A 和 B 的账户都正确更新了持仓

HTTP API:
- POST /api/account/open - 开户
- GET /api/account/{account_id} - 查询账户
- POST /api/order/submit - 提交订单
- GET /api/position/{account_id} - 查询持仓
"""

import requests
import json
import time
import uuid
from typing import Optional, Dict, Any

BASE_URL = "http://127.0.0.1:8094"

class ExchangeClient:
    """交易所 HTTP 客户端"""

    def __init__(self, base_url: str = BASE_URL):
        self.base_url = base_url
        self.session = requests.Session()

    def health_check(self) -> bool:
        """健康检查"""
        try:
            resp = self.session.get(f"{self.base_url}/health", timeout=5)
            return resp.status_code == 200
        except Exception as e:
            print(f"健康检查失败: {e}")
            return False

    def register_user(self, username: str, password: str = "test123") -> Optional[str]:
        """注册用户"""
        payload = {
            "username": username,
            "password": password
        }
        try:
            resp = self.session.post(f"{self.base_url}/api/auth/register", json=payload)
            data = resp.json()
            if data.get("success"):
                user_id = data["data"].get("user_id")
                print(f"[+] 用户注册成功: username={username}, user_id={user_id}")
                return user_id
            else:
                error = data.get("error", {})
                print(f"[-] 用户注册失败: {error.get('message', error)}")
                return None
        except Exception as e:
            print(f"[-] 用户注册异常: {e}")
            return None

    def open_account(self, user_id: str, user_name: str, init_cash: float = 1000000.0) -> Optional[str]:
        """开户"""
        payload = {
            "user_id": user_id,
            "user_name": user_name,
            "init_cash": init_cash,
            "account_type": "individual",
            "password": "test123"
        }
        resp = self.session.post(f"{self.base_url}/api/account/open", json=payload)
        data = resp.json()
        if data.get("success"):
            account_id = data["data"].get("account_id")
            print(f"[+] 开户成功: user_id={user_id}, account_id={account_id}")
            return account_id
        else:
            print(f"[-] 开户失败: {data.get('error')}")
            return None

    def get_account(self, account_id: str) -> Optional[Dict[str, Any]]:
        """查询账户"""
        resp = self.session.get(f"{self.base_url}/api/account/{account_id}")
        data = resp.json()
        if data.get("success"):
            return data["data"]
        else:
            print(f"[-] 查询账户失败: {data.get('error')}")
            return None

    def submit_order(
        self,
        user_id: str,
        account_id: str,
        instrument_id: str,
        direction: str,  # BUY / SELL
        offset: str,     # OPEN / CLOSE
        volume: float,
        price: float,
        order_type: str = "LIMIT"
    ) -> Optional[str]:
        """提交订单"""
        payload = {
            "user_id": user_id,
            "account_id": account_id,
            "instrument_id": instrument_id,
            "direction": direction,
            "offset": offset,
            "volume": volume,
            "price": price,
            "order_type": order_type
        }
        resp = self.session.post(f"{self.base_url}/api/order/submit", json=payload)
        data = resp.json()
        if data.get("success"):
            order_id = data["data"].get("order_id")
            print(f"[+] 订单提交成功: order_id={order_id}, {direction} {offset} {volume}@{price}")
            return order_id
        else:
            print(f"[-] 订单提交失败: {data.get('error')}")
            return None

    def get_positions(self, account_id: str) -> Optional[list]:
        """查询持仓"""
        resp = self.session.get(f"{self.base_url}/api/position/account/{account_id}")
        data = resp.json()
        if data.get("success"):
            return data["data"]
        else:
            print(f"[-] 查询持仓失败: {data.get('error')}")
            return None

    def get_order(self, order_id: str) -> Optional[Dict[str, Any]]:
        """查询订单"""
        resp = self.session.get(f"{self.base_url}/api/order/{order_id}")
        data = resp.json()
        if data.get("success"):
            return data["data"]
        else:
            return None


def print_separator(title: str):
    """打印分隔线"""
    print(f"\n{'='*60}")
    print(f"  {title}")
    print(f"{'='*60}\n")


def test_bidirectional_trade():
    """测试双边交易：A买入开仓，B卖出开仓，验证双方账户都更新"""

    client = ExchangeClient()

    # 1. 健康检查
    print_separator("步骤 1: 健康检查")
    if not client.health_check():
        print("[-] 服务器未启动，请先启动 qaexchange-server")
        return False
    print("[+] 服务器正常运行")

    # 2. 注册测试用户
    print_separator("步骤 2: 注册测试用户")

    # 生成唯一的用户名避免冲突
    test_id = str(uuid.uuid4())[:8]
    username_a = f"test_user_A_{test_id}"
    username_b = f"test_user_B_{test_id}"

    user_id_a = client.register_user(username_a)
    user_id_b = client.register_user(username_b)

    if not user_id_a or not user_id_b:
        print("[-] 注册用户失败")
        return False

    # 3. 创建交易账户
    print_separator("步骤 3: 创建交易账户")

    account_a = client.open_account(user_id_a, f"测试账户A_{test_id}", init_cash=1000000.0)
    account_b = client.open_account(user_id_b, f"测试账户B_{test_id}", init_cash=1000000.0)

    if not account_a or not account_b:
        print("[-] 创建账户失败")
        return False

    # 4. 查询初始账户状态
    print_separator("步骤 4: 查询初始账户状态")

    acc_a_before = client.get_account(account_a)
    acc_b_before = client.get_account(account_b)

    if acc_a_before:
        print(f"账户A初始: balance={acc_a_before['balance']}, available={acc_a_before['available']}")
    if acc_b_before:
        print(f"账户B初始: balance={acc_b_before['balance']}, available={acc_b_before['available']}")

    # 5. 提交订单：A买入开仓，B卖出开仓
    print_separator("步骤 5: 提交对冲订单")

    instrument = "IF2501"  # 股指期货
    price = 3800.0   # 价格
    volume = 1.0     # 1手

    # A: 买入开仓
    print(f"\n账户A ({account_a}): 买入开仓 {instrument}")
    order_a = client.submit_order(
        user_id=user_id_a,
        account_id=account_a,
        instrument_id=instrument,
        direction="BUY",
        offset="OPEN",
        volume=volume,
        price=price
    )

    # 稍等一下让订单进入订单簿
    time.sleep(0.5)

    # B: 卖出开仓（相同价格，应该被撮合）
    print(f"\n账户B ({account_b}): 卖出开仓 {instrument}")
    order_b = client.submit_order(
        user_id=user_id_b,
        account_id=account_b,
        instrument_id=instrument,
        direction="SELL",
        offset="OPEN",
        volume=volume,
        price=price
    )

    if not order_a or not order_b:
        print("[-] 订单提交失败")
        return False

    # 6. 等待撮合完成
    print_separator("步骤 6: 等待撮合完成")
    time.sleep(1.0)  # 给撮合引擎一点时间

    # 7. 检查订单状态
    print_separator("步骤 7: 检查订单状态")

    order_a_status = client.get_order(order_a)
    order_b_status = client.get_order(order_b)

    if order_a_status:
        print(f"订单A状态: {order_a_status.get('status')}, 成交量: {order_a_status.get('filled_volume')}")
    else:
        print("订单A状态: 未找到")

    if order_b_status:
        print(f"订单B状态: {order_b_status.get('status')}, 成交量: {order_b_status.get('filled_volume')}")
    else:
        print("订单B状态: 未找到")

    # 8. 验证双方持仓
    print_separator("步骤 8: 验证双方持仓 (关键测试)")

    positions_a = client.get_positions(account_a)
    positions_b = client.get_positions(account_b)

    print(f"\n账户A ({account_a}) 持仓:")
    pos_a_ok = False
    if positions_a:
        for pos in positions_a:
            print(f"  - {pos['instrument_id']}: 多={pos['volume_long']}, 空={pos['volume_short']}")
            if pos['instrument_id'] == instrument and pos['volume_long'] == volume:
                pos_a_ok = True
                print(f"    [OK] 账户A 多头持仓正确！")
    else:
        print("  (无持仓)")

    print(f"\n账户B ({account_b}) 持仓:")
    pos_b_ok = False
    if positions_b:
        for pos in positions_b:
            print(f"  - {pos['instrument_id']}: 多={pos['volume_long']}, 空={pos['volume_short']}")
            if pos['instrument_id'] == instrument and pos['volume_short'] == volume:
                pos_b_ok = True
                print(f"    [OK] 账户B 空头持仓正确！")
    else:
        print("  (无持仓)")

    # 9. 验证账户资金变化
    print_separator("步骤 9: 验证账户资金变化")

    acc_a_after = client.get_account(account_a)
    acc_b_after = client.get_account(account_b)

    if acc_a_after and acc_a_before:
        # ✨ 使用 frozen 字段检查保证金（API 返回 frozen 而非 margin）@yutiansut @quantaxis
        frozen_a = acc_a_after.get('frozen', 0)
        available_change_a = acc_a_after['available'] - acc_a_before['available']
        print(f"账户A: frozen={frozen_a:.2f}, available变化={available_change_a:.2f}")
        if frozen_a > 0:
            print(f"    [OK] 账户A 保证金已冻结！")

    if acc_b_after and acc_b_before:
        frozen_b = acc_b_after.get('frozen', 0)
        available_change_b = acc_b_after['available'] - acc_b_before['available']
        print(f"账户B: frozen={frozen_b:.2f}, available变化={available_change_b:.2f}")
        if frozen_b > 0:
            print(f"    [OK] 账户B 保证金已冻结！")

    # 10. 汇总测试结果
    print_separator("测试结果汇总")

    results = {
        "账户A持仓正确": pos_a_ok,
        "账户B持仓正确": pos_b_ok,
        # ✨ 改用 frozen 字段检查 @yutiansut @quantaxis
        "账户A保证金冻结": acc_a_after.get('frozen', 0) > 0 if acc_a_after else False,
        "账户B保证金冻结": acc_b_after.get('frozen', 0) > 0 if acc_b_after else False,
    }

    all_passed = all(results.values())

    for test_name, passed in results.items():
        status = "PASS" if passed else "FAIL"
        print(f"  [{status}] {test_name}")

    print()
    if all_passed:
        print("=" * 60)
        print("  双边交易测试通过！A/B 账户都正确更新了")
        print("=" * 60)
        return True
    else:
        print("=" * 60)
        print("  双边交易测试失败！请检查撮合逻辑")
        print("=" * 60)
        return False


if __name__ == "__main__":
    import sys

    print("""
    ╔═══════════════════════════════════════════════════════════╗
    ║        QAExchange 双边交易测试脚本                          ║
    ║        @yutiansut @quantaxis                              ║
    ╚═══════════════════════════════════════════════════════════╝
    """)

    success = test_bidirectional_trade()
    sys.exit(0 if success else 1)
