"""在真实 Chromium 里验证 sanitize.js —— 17 条用例 + 阳性对照。@yutiansut @quantaxis

跑法:  cd web && python3 tests/test_sanitize.py   (需 playwright)

为什么必须带阳性对照:XSS 检测最容易出的错是「检测器本身是死的」——
载荷根本没注进去,于是每条都报 0 次触发、看起来全挡住了。
本文件开头先用**未净化的** innerHTML 直注一次,必须观测到 fired=1;
观测不到就说明下面所有的 0 都没有意义。

覆盖:排版保留(b/p/ul/table/a)、站内链接与锚点、SCRIPT/STYLE/IFRAME 整删、
未知标签保文本、javascript: 伪协议、协议相对 URL(//host)、on* 与 style 属性、
img/onerror 与其大小写变形、svg 嵌套。
"""
import io, json, re
from playwright.sync_api import sync_playwright

src = io.open(__import__('os').path.join(__import__('os').path.dirname(__import__('os').path.dirname(__import__('os').path.abspath(__file__))), 'src/utils/sanitize.js'), encoding='utf-8').read()
src = src.replace('export function', 'function')
src = re.sub(r'(?m)^export default .*$', '', src)   # 经典脚本里 export 是语法错误
assert 'export' not in src, '仍有 export 残留'

CASES = [
 ("正常排版保留", "<b>粗体</b><p>段落</p><ul><li>列表</li></ul>"),
 ("表格保留",     "<table><tr><td>单元格</td></tr></table>"),
 ("外链保留",     '<a href="https://example.com">链接</a>'),
 ("站内链接保留", '<a href="/x/y">内链</a>'),
 ("锚点保留",     '<a href="#sec">锚</a>'),
 ("SCRIPT 整删",  "<script>alert(1)</script>正文"),
 ("STYLE 整删",   '<style>body{background:url("http://e/x")}</style>正文'),
 ("IFRAME 整删",  '<iframe srcdoc="&lt;script&gt;alert(1)&lt;/script&gt;"></iframe>OK'),
 ("未知标签保文本", '<font color="red">红字</font>'),
 ("javascript: 去", '<a href="javascript:alert(1)">点我</a>'),
 ("协议相对拒绝", '<a href="//evil.example/x">看似站内</a>'),
 ("on* 属性去",   '<p onclick="alert(1)">文字</p>'),
 ("style 属性去", '<p style="color:red">文字</p>'),
 ("img 去",       "<img src=x onerror=alert(1)>正文"),
 ("大小写绕过",   "<IMG SRC=x OnErRoR=alert(1)>正文"),
 ("svg 嵌 img",   "<svg><img src=x onerror=alert(1)></svg>正文"),
 ("空输入",       ""),
]
CHECK = {
 "正常排版保留": lambda h: "<b>粗体</b>" in h and "<li>列表</li>" in h,
 "表格保留":     lambda h: "单元格" in h and re.search(r"<t[dr]", h),
 "外链保留":     lambda h: 'href="https://example.com"' in h,
 "站内链接保留": lambda h: 'href="/x/y"' in h,
 "锚点保留":     lambda h: 'href="#sec"' in h,
 "SCRIPT 整删":  lambda h: "alert" not in h and "正文" in h,
 "STYLE 整删":   lambda h: "background" not in h and "正文" in h,
 "IFRAME 整删":  lambda h: "alert" not in h and "srcdoc" not in h and "OK" in h,
 "未知标签保文本": lambda h: "红字" in h and "font" not in h.lower(),
 "javascript: 去": lambda h: "javascript" not in h.lower() and "点我" in h,
 "协议相对拒绝": lambda h: "evil.example" not in h and "看似站内" in h,
 "on* 属性去":   lambda h: "onclick" not in h.lower() and "文字" in h,
 "style 属性去": lambda h: "style=" not in h.lower(),
 "img 去":       lambda h: "onerror" not in h.lower() and "正文" in h,
 "大小写绕过":   lambda h: "onerror" not in h.lower() and "正文" in h,
 "svg 嵌 img":   lambda h: "onerror" not in h.lower(),
 "空输入":       lambda h: h == "",
}

with sync_playwright() as pw:
    b = pw.chromium.launch()
    pg = b.new_page()
    pg.goto("about:blank")
    pg.add_script_tag(content=src + "\nwindow.__S = sanitizeHtml;")

    # 阳性对照:先证明「直接 innerHTML 注入确实会执行」,否则全 0 是死仪器
    fired = pg.evaluate("""() => {
        window.__x = 0;
        const d = document.createElement('div');
        document.body.appendChild(d);
        d.innerHTML = '<img src=x onerror="window.__x=1">';
        return new Promise(r => setTimeout(() => r(window.__x), 300));
    }""")
    print(f"  阳性对照(未净化直注 img/onerror 是否触发): {fired}  {'✅ 仪器活着' if fired==1 else '❌ 仪器是死的,下面的 0 无意义'}")
    print()

    out = pg.evaluate("cs => cs.map(c => [c[0], window.__S(c[1])])", CASES)
    ok = 0
    for name, html in out:
        good = bool(CHECK[name](html))
        ok += good
        print(f"  {'✅' if good else '❌'} {name:14s} → {json.dumps(html, ensure_ascii=False)[:66]}")

    # 净化后再注入,确认不触发
    fired2 = pg.evaluate("""() => {
        window.__y = 0;
        const d = document.createElement('div');
        document.body.appendChild(d);
        d.innerHTML = window.__S('<img src=x onerror="window.__y=1">');
        return new Promise(r => setTimeout(() => r(window.__y), 300));
    }""")
    print(f"\n  净化后再注入是否触发: {fired2}  {'✅ 未触发' if fired2==0 else '❌ 仍触发'}")
    print(f"  ⇒ {ok}/{len(CASES)} 用例通过")
    b.close()
