#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""部署后验证:逐条确认「部署缺口」那批修复在**新运行的二进制**里真的生效了。
用法: python3 tools/verify_post_deploy.py [BACKEND_URL]  (默认 http://127.0.0.1:8094)
在**重编+重启之后**运行。每条给 PASS/FAIL/SKIP + 依据。@yutiansut @quantaxis
"""
import sys, json, subprocess, urllib.request, urllib.error, os, glob

BASE = sys.argv[1] if len(sys.argv) > 1 else "http://127.0.0.1:8094"
PID_HINT = None  # 自动找运行 pid

def get(path, timeout=30):
    try:
        with urllib.request.urlopen(BASE + path, timeout=timeout) as r:
            return r.status, r.read().decode("utf-8", "replace")
    except urllib.error.HTTPError as e:
        return e.code, e.read().decode("utf-8", "replace")
    except Exception as e:
        return None, str(e)

def find_backend_pid():
    try:
        out = subprocess.check_output(["bash","-c",
            "ss -ltnp 2>/dev/null | grep ':8094 ' | grep -oE 'pid=[0-9]+' | head -1 | cut -d= -f2"],
            text=True).strip()
        return out or None
    except Exception:
        return None

results = []
def rec(name, ok, detail):
    results.append((name, ok, detail))
    tag = {True:"PASS", False:"FAIL", None:"SKIP"}[ok]
    print(f"[{tag}] {name}\n       {detail}")

print(f"== 部署后验证 @ {BASE} ==\n")

# 0. 进程是不是新起的(对照 build 后)
pid = find_backend_pid()
if pid:
    try:
        etime = subprocess.check_output(["ps","-o","etime=","-p",pid], text=True).strip()
        rec("后端进程存活", True, f"pid={pid} 运行时长={etime}(若刚重启应很短)")
    except Exception as e:
        rec("后端进程存活", None, str(e))
else:
    rec("后端进程存活", False, "8094 无监听进程")

# 1. 符号探针:from_qifi_trade 应出现在新运行二进制(旧版=0)
if pid:
    try:
        n = subprocess.run(["bash","-c",f"grep -a -c from_qifi_trade /proc/{pid}/exe 2>/dev/null || echo 0"],
                           capture_output=True, text=True).stdout.strip().split("\n")[0]
        ctrl = subprocess.run(["bash","-c",f"grep -a -c get_trades_by_user /proc/{pid}/exe 2>/dev/null || echo 0"],
                           capture_output=True, text=True).stdout.strip().split("\n")[0]
        ok = int(n) >= 1 and int(ctrl) >= 1
        rec("QIFI成交读取修复已部署", ok,
            f"from_qifi_trade 运行二进制={n}(应≥1), 正对照 get_trades_by_user={ctrl}(应≥1)")
    except Exception as e:
        rec("QIFI成交读取修复已部署", None, str(e))
else:
    rec("QIFI成交读取修复已部署", None, "无 pid")

# 2. 成交列表:对有历史的账户应返回全量(旧版只返执行态少量)
#    先从快照找一个成交多的账户
snaps = sorted(glob.glob("output/qaexchange/storage/snapshots/*.json"),
               key=lambda f: os.path.getsize(f), reverse=True)
if snaps:
    big = snaps[0]
    try:
        j = json.load(open(big))
        acc = j.get("account_cookie") or os.path.basename(big).replace(".json","")
        snap_trades = len(j.get("trades",{})) if isinstance(j.get("trades"),dict) else 0
        code, body = get(f"/api/trades/account/{acc}?page=1&page_size=5")
        api_total = "?"
        try:
            d = json.loads(body).get("data",{})
            api_total = d.get("total", d.get("total_count","?")) if isinstance(d,dict) else "?"
        except Exception: pass
        # 修复生效判据:api_total 接近 snap_trades(而非旧版的个位数)
        ok = isinstance(api_total,int) and snap_trades>0 and api_total >= snap_trades*0.5
        rec("成交列表读QIFI全量", ok,
            f"账户 {acc[:16]}: 快照 {snap_trades} 笔, 接口 total={api_total}(修复后应接近快照, 旧版仅个位数)")
    except Exception as e:
        rec("成交列表读QIFI全量", None, str(e))
else:
    rec("成交列表读QIFI全量", None, "无快照文件")

# 3. 市场接口:不存在合约应 404 而非 500
code, body = get("/api/market/tick/__NOSUCH_INSTRUMENT__")
if code is not None:
    ok = code == 404
    rec("市场接口不存在合约→404", ok, f"/api/market/tick/__NOSUCH__ → HTTP {code}(修复后应 404, 旧版 500)")
else:
    rec("市场接口不存在合约→404", None, body)

# 4. 聚合非有限防护:dashboard total_balance 不应是 1e300(即便存在 T1e300)
code, body = get("/api/monitoring/system")
try:
    parsed = json.loads(body)
    d = parsed  # total_balance 在顶层 accounts.total_balance, data 可能为 None
    tb = None
    def find_tb(o):
        if isinstance(o,dict):
            for k,v in o.items():
                if k=="total_balance" and isinstance(v,(int,float)): return v
                r=find_tb(v)
                if r is not None: return r
        return None
    tb = find_tb(d)
    if tb is None:
        rec("聚合非有限防护", None, "monitoring/system 无 total_balance 字段")
    else:
        ok = abs(tb) < 1e15
        rec("聚合非有限防护", ok, f"total_balance={tb}(修复后应<1e15, 未修则被 T1e300 吸成 1e300)")
except Exception as e:
    rec("聚合非有限防护", None, str(e))

# 5b. 账户列表排序:desc 首行余额应 >= asc 首行(部署后 apply_sort! 生效)
def first_balance(order):
    code, body = get(f"/api/management/accounts?page=1&page_size=5&sort_by=balance&sort_order={order}")
    try:
        rows = json.loads(body).get("data",{}).get("accounts",[])
        return rows[0].get("balance") if rows else None
    except Exception:
        return None
bd, ba = first_balance("desc"), first_balance("asc")
if isinstance(bd,(int,float)) and isinstance(ba,(int,float)):
    ok = bd >= ba and bd != ba  # desc 首行应 >= asc 首行, 且两者不同(证明真排了)
    rec("账户列表排序生效", ok,
        f"desc首行余额={bd}, asc首行={ba}(修复后 desc>=asc 且不等; 旧版忽略 sort → 两者相同)")
else:
    rec("账户列表排序生效", None, f"取值失败 desc={bd} asc={ba}")

# 6. compaction 错误应停止(重启后 scheduler 修复生效, 不再刷 Failed to open 0000000001.sst)
#    只能提示:重启后观察日志 60s 内有没有新的 compaction 错误
rec("compaction 错误停止", None,
    "需人工:重启后 tail -f 日志 60s, 确认不再出现 'Failed to open ...0000000001.sst'(每10秒一次那种)")

# 汇总
p = sum(1 for _,ok,_ in results if ok is True)
f = sum(1 for _,ok,_ in results if ok is False)
s = sum(1 for _,ok,_ in results if ok is None)
print(f"\n== 汇总: {p} PASS / {f} FAIL / {s} SKIP ==")
print("FAIL = 该修复没生效(可能没重编/没重启, 或需 rebuild); SKIP = 需人工或缺前提")
sys.exit(1 if f>0 else 0)
