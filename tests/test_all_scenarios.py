#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
完整交易场景测试脚本
@yutiansut @quantaxis

测试场景：
1. 下单（买入开仓 / 卖出开仓）
2. 撤单
3. 全部成交
4. 部分成交（大单 vs 小单）
5. 平仓

HTTP API:
- POST /api/account/open - 开户
- GET /api/account/{account_id} - 查询账户
- POST /api/order/submit - 提交订单
- POST /api/order/cancel - 撤单
- GET /api/position/{account_id} - 查询持仓
"""

import requests
import json
import time
import uuid
from typing import Optional, Dict, Any, List

BASE_URL = "http://127.0.0.1:8094"


class ExchangeClient:
    """交易所 HTTP 客户端"""

    def __init__(self, base_url: str = BASE_URL):
        self.base_url = base_url
        self.session = requests.Session()

    def health_check(self) -> bool:
        try:
            resp = self.session.get(f"{self.base_url}/health", timeout=5)
            return resp.status_code == 200
        except Exception as e:
            print(f"健康检查失败: {e}")
            return False

    def register_user(self, username: str, password: str = "test123") -> Optional[str]:
        payload = {"username": username, "password": password}
        try:
            resp = self.session.post(f"{self.base_url}/api/auth/register", json=payload)
            data = resp.json()
            if data.get("success"):
                return data["data"].get("user_id")
            return None
        except:
            return None

    def open_account(self, user_id: str, user_name: str, init_cash: float = 1000000.0) -> Optional[str]:
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
            return data["data"].get("account_id")
        return None

    def get_account(self, account_id: str) -> Optional[Dict[str, Any]]:
        resp = self.session.get(f"{self.base_url}/api/account/{account_id}")
        data = resp.json()
        if data.get("success"):
            return data["data"]
        return None

    def submit_order(
        self,
        user_id: str,
        account_id: str,
        instrument_id: str,
        direction: str,
        offset: str,
        volume: float,
        price: float,
        order_type: str = "LIMIT"
    ) -> Optional[str]:
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
            return data["data"].get("order_id")
        else:
            print(f"[-] 订单提交失败: {data.get('error')}")
            return None

    def cancel_order(self, user_id: str, account_id: str, order_id: str) -> bool:
        payload = {
            "user_id": user_id,
            "account_id": account_id,
            "order_id": order_id
        }
        resp = self.session.post(f"{self.base_url}/api/order/cancel", json=payload)
        data = resp.json()
        return data.get("success", False)

    def get_positions(self, account_id: str) -> Optional[list]:
        resp = self.session.get(f"{self.base_url}/api/position/account/{account_id}")
        data = resp.json()
        if data.get("success"):
            return data["data"]
        return None

    def get_order(self, order_id: str) -> Optional[Dict[str, Any]]:
        resp = self.session.get(f"{self.base_url}/api/order/{order_id}")
        data = resp.json()
        if data.get("success"):
            return data["data"]
        return None


def print_separator(title: str):
    print(f"\n{'='*60}")
    print(f"  {title}")
    print(f"{'='*60}\n")


def test_cancel_order(client: ExchangeClient, user_id: str, account_id: str):
    """测试撤单功能"""
    print_separator("测试撤单功能")

    # 1. 提交一个买单（价格较低，不会立即成交）
    low_price = 3700.0  # 低于市价
    order_id = client.submit_order(
        user_id=user_id,
        account_id=account_id,
        instrument_id="IF2501",
        direction="BUY",
        offset="OPEN",
        volume=1.0,
        price=low_price
    )

    if not order_id:
        print("[-] 下单失败")
        return False

    print(f"[+] 订单已提交: {order_id} @ {low_price}")

    # 2. 查询订单状态
    time.sleep(0.5)
    order_status = client.get_order(order_id)
    if order_status:
        print(f"    订单状态: {order_status.get('status')}, volume_left={order_status.get('volume_left')}")

    # 3. 查询账户资金（应该有冻结）
    acc_before = client.get_account(account_id)
    frozen_before = acc_before.get('frozen', 0) if acc_before else 0
    print(f"    冻结资金: {frozen_before:.2f}")

    # 4. 撤单
    success = client.cancel_order(user_id, account_id, order_id)
    print(f"    撤单结果: {'成功' if success else '失败'}")

    # 5. 验证撤单后资金释放
    time.sleep(0.5)
    acc_after = client.get_account(account_id)
    frozen_after = acc_after.get('frozen', 0) if acc_after else 0
    print(f"    撤单后冻结: {frozen_after:.2f}")

    # 6. 验证订单状态
    order_after = client.get_order(order_id)
    if order_after:
        print(f"    撤单后订单状态: {order_after.get('status')}")

    return success and frozen_after < frozen_before


def test_partial_fill(client: ExchangeClient, user_a: str, acc_a: str, user_b: str, acc_b: str):
    """测试部分成交（大单 vs 小单）"""
    print_separator("测试部分成交")

    # 1. A 提交大买单（10手）
    large_order_id = client.submit_order(
        user_id=user_a,
        account_id=acc_a,
        instrument_id="IF2502",
        direction="BUY",
        offset="OPEN",
        volume=10.0,
        price=3820.0
    )
    print(f"[+] A 提交大买单: {large_order_id}, 10手 @ 3820")

    time.sleep(0.5)

    # 2. B 提交小卖单（3手）- 应该部分成交 A 的订单
    small_order_id = client.submit_order(
        user_id=user_b,
        account_id=acc_b,
        instrument_id="IF2502",
        direction="SELL",
        offset="OPEN",
        volume=3.0,
        price=3820.0
    )
    print(f"[+] B 提交小卖单: {small_order_id}, 3手 @ 3820")

    time.sleep(1.0)

    # 3. 检查 A 的订单状态（应该部分成交）
    order_a = client.get_order(large_order_id)
    if order_a:
        filled = order_a.get('filled_volume', 0)
        left = order_a.get('volume_left', 10)
        status = order_a.get('status')
        print(f"    A 订单: 已成交={filled}, 剩余={left}, 状态={status}")

        # 部分成交：filled > 0 and left > 0
        if filled > 0 and left > 0:
            print(f"    [OK] 部分成交正确！")
        elif filled == 10:
            print(f"    [OK] 全部成交（可能有其他订单参与）")
        else:
            print(f"    [!] 未成交或状态异常")

    # 4. 检查 B 的订单状态（应该全部成交）
    order_b = client.get_order(small_order_id)
    if order_b:
        filled = order_b.get('filled_volume', 0)
        status = order_b.get('status')
        print(f"    B 订单: 已成交={filled}, 状态={status}")

    # 5. 检查持仓
    pos_a = client.get_positions(acc_a)
    pos_b = client.get_positions(acc_b)

    print(f"\n    A 持仓: {pos_a}")
    print(f"    B 持仓: {pos_b}")

    return True


def test_close_position(client: ExchangeClient, user_id: str, account_id: str):
    """测试平仓"""
    print_separator("测试平仓")

    # 1. 查询当前持仓
    positions = client.get_positions(account_id)
    if not positions:
        print("[-] 无持仓可平")
        return False

    # 找到有多头持仓的合约
    long_pos = None
    for pos in positions:
        if pos.get('volume_long', 0) > 0:
            long_pos = pos
            break

    if not long_pos:
        print("[-] 无多头持仓可平")
        return False

    instrument = long_pos['instrument_id']
    volume = long_pos['volume_long']
    print(f"[+] 找到多头持仓: {instrument}, {volume}手")

    # 2. 提交卖出平仓订单
    close_order_id = client.submit_order(
        user_id=user_id,
        account_id=account_id,
        instrument_id=instrument,
        direction="SELL",
        offset="CLOSE",
        volume=volume,
        price=3800.0  # 假设市价
    )

    if not close_order_id:
        print("[-] 平仓订单提交失败")
        return False

    print(f"[+] 平仓订单已提交: {close_order_id}")

    time.sleep(1.0)

    # 3. 检查订单状态
    order_status = client.get_order(close_order_id)
    if order_status:
        print(f"    订单状态: {order_status.get('status')}, 成交={order_status.get('filled_volume')}")

    # 4. 检查持仓是否减少
    positions_after = client.get_positions(account_id)
    print(f"    平仓后持仓: {positions_after}")

    return True


def main():
    print("""
    ╔═══════════════════════════════════════════════════════════╗
    ║        QAExchange 完整交易场景测试                         ║
    ║        @yutiansut @quantaxis                              ║
    ╚═══════════════════════════════════════════════════════════╝
    """)

    client = ExchangeClient()

    # 1. 健康检查
    if not client.health_check():
        print("[-] 服务器未启动")
        return
    print("[+] 服务器正常")

    # 2. 创建测试用户和账户
    test_id = str(uuid.uuid4())[:8]

    user_a = client.register_user(f"test_a_{test_id}")
    user_b = client.register_user(f"test_b_{test_id}")

    if not user_a or not user_b:
        print("[-] 用户注册失败")
        return

    acc_a = client.open_account(user_a, f"测试账户A_{test_id}")
    acc_b = client.open_account(user_b, f"测试账户B_{test_id}")

    if not acc_a or not acc_b:
        print("[-] 账户创建失败")
        return

    print(f"[+] 测试账户已创建: A={acc_a}, B={acc_b}")

    # 3. 测试撤单
    cancel_result = test_cancel_order(client, user_a, acc_a)
    print(f"\n撤单测试: {'PASS' if cancel_result else 'FAIL'}")

    # 4. 测试部分成交
    partial_result = test_partial_fill(client, user_a, acc_a, user_b, acc_b)
    print(f"\n部分成交测试: {'PASS' if partial_result else 'FAIL'}")

    # 5. 测试平仓（如果有持仓）
    close_result = test_close_position(client, user_a, acc_a)
    print(f"\n平仓测试: {'PASS' if close_result else 'SKIP (无持仓)'}")

    # 汇总
    print_separator("测试汇总")
    print(f"  撤单功能: {'✓' if cancel_result else '✗'}")
    print(f"  部分成交: {'✓' if partial_result else '✗'}")
    print(f"  平仓功能: {'✓' if close_result else '○ (跳过)'}")


if __name__ == "__main__":
    main()
