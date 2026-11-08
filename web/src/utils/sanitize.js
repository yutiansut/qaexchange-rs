/**
 * 富文本净化 —— 给 `v-html` 用。@yutiansut @quantaxis
 *
 * ⚠️ 为什么必须有这个文件(2026-09-05 回验代理实测):
 *
 * 全仓有两处 `v-html`:
 *   · `views/login.vue:183`            登录后的公告弹窗
 *   · `views/announcements/index.vue:89` 公告详情正文
 * 而 `grep -rniE 'dompurify|sanitize|xss'` 在 `web/src` 与 `package.json` 上
 * **零命中** —— 没有任何净化。
 *
 * 代理用桩把公告 content 换成
 *   `<b>BOLD</b><img src=x onerror="window.__l30xss=1">`
 * 实测:`<img>` 节点被创建,**onerror 真的执行了**(`xssFired=1`);
 * 纯文本对照腿 `xssFired=0`,证明读数器能区分。
 * ⇒ **这是可利用的存储型 XSS**:公告由管理端写入、对所有登录用户渲染。
 *
 * 加重情节:创建公告的管理口令是字面量 `qaexchange_admin_2024`,
 * 且**已被打进发布 bundle**(代理在 `app.js`/`10.js`/`12.js` 里 grep 到);
 * 后端 `account_admin.rs:771` 对内容不做任何长度或内容校验就广播。
 *
 * 为什么不引 DOMPurify:本项目是 Vue 2 + 较老构建链(连 `??` 都不支持,
 * 见 lessons L105),加依赖需要评估兼容性与体积。公告是**受控内容**
 * (只有管理员能发),这里用**白名单**策略即可,且实现不足 60 行、无依赖。
 * 若将来公告开放给非管理员,应换成 DOMPurify。
 *
 * 策略:**白名单标签 + 白名单属性**,其余一律剥离。
 * 不是黑名单(黑名单永远漏,如 `<svg/onload>`、`javascript:` 伪协议、
 * 大小写与实体编码绕过)。
 */

// 允许的标签 —— 只保留排版所需,不含任何可执行或可加载外部资源的标签
const ALLOWED_TAGS = new Set([
  'P', 'BR', 'DIV', 'SPAN',
  'B', 'STRONG', 'I', 'EM', 'U', 'S',
  'UL', 'OL', 'LI',
  'H1', 'H2', 'H3', 'H4', 'H5', 'H6',
  'BLOCKQUOTE', 'PRE', 'CODE',
  'TABLE', 'THEAD', 'TBODY', 'TR', 'TH', 'TD',
  'HR', 'A'
])

// 这些标签**连文本内容一起丢弃** —— 它们的 textContent 是代码不是正文。
//
// ⚠️ 其余非白名单标签走「保留 textContent、删元素」是对的(不能因为出现
// 一个 `<font>` 就把整段正文吃掉),但对 SCRIPT / STYLE 这条规则会把
// **源码当正文显示**:实测 `<script>alert(1)</script>正文` 渲染成
// `alert(1)正文`、`<style>body{background:url("http://evil.example/x")}</style>正文`
// 把整条 CSS 打在页面上。脚本没执行(安全上无洞),但显示是错的。
// @yutiansut @quantaxis
const DROP_WITH_CONTENT = new Set([
  'SCRIPT', 'STYLE', 'NOSCRIPT', 'TEMPLATE',
  'IFRAME', 'OBJECT', 'EMBED', 'APPLET', 'FRAME', 'FRAMESET'
])

// 允许的属性 —— 注意**不含**任何 on* 事件属性与 style
const ALLOWED_ATTRS = {
  A: ['href', 'title', 'target', 'rel'],
  TD: ['colspan', 'rowspan'],
  TH: ['colspan', 'rowspan']
}

// href 只允许这些协议;`javascript:` / `data:` 一律拒绝。
//
// ⚠️ `\/` 那一支必须**排除协议相对 URL**(`//evil.example/x`):
// 它以 `/` 开头,看起来像站内路径,浏览器却会当作 `https://evil.example/x` 去取。
// 公告里挂一条这样的链接,读者从 URL 上分辨不出是外链。
// 所以用 `\/(?!\/)` —— 单个 `/` 开头(站内绝对路径)放行,`//` 开头拒绝。
const SAFE_HREF = /^(https?:|mailto:|tel:|#|\/(?!\/))/i

/**
 * 净化 HTML 片段,返回可安全交给 v-html 的字符串。
 * @param {string} html 原始内容
 * @returns {string} 净化后的 HTML;输入非字符串时返回空串
 */
export function sanitizeHtml (html) {
  if (typeof html !== 'string' || html === '') return ''

  // 用 DOMParser 解析到**游离文档**,不接入当前页面 —— 解析阶段不会执行脚本,
  // 也不会因 <img src=x> 发起请求(游离文档的图片不加载)。
  let doc
  try {
    doc = new DOMParser().parseFromString(`<body>${html}</body>`, 'text/html')
  } catch (e) {
    // 解析失败时**不要**回退到原文 —— 那等于没净化。降级为纯文本。
    return escapeText(html)
  }

  const walk = node => {
    // 倒序遍历:移除子节点时不会打乱索引
    const children = Array.prototype.slice.call(node.childNodes)
    for (let i = children.length - 1; i >= 0; i--) {
      const child = children[i]
      if (child.nodeType === 3) continue          // 文本节点,保留
      if (child.nodeType !== 1) {                  // 注释等,直接删
        node.removeChild(child); continue
      }
      const tag = child.tagName
      if (DROP_WITH_CONTENT.has(tag)) {
        // 整节点删除,textContent 也不留 —— 见 DROP_WITH_CONTENT 的说明
        node.removeChild(child)
        continue
      }
      if (!ALLOWED_TAGS.has(tag)) {
        // 其余不在白名单的:**保留其文本内容**再删元素,避免正文丢失
        const text = doc.createTextNode(child.textContent || '')
        node.replaceChild(text, child)
        continue
      }
      // 剥属性:先收集再删,避免遍历时修改集合
      const attrs = Array.prototype.slice.call(child.attributes)
      const allowed = ALLOWED_ATTRS[tag] || []
      attrs.forEach(a => {
        const name = a.name.toLowerCase()
        if (allowed.indexOf(name) === -1) {
          child.removeAttribute(a.name)
          return
        }
        if (name === 'href' && !SAFE_HREF.test((a.value || '').trim())) {
          child.removeAttribute(a.name)
        }
      })
      // 外链统一加 noopener,防 window.opener 劫持
      if (tag === 'A' && child.getAttribute('target')) {
        child.setAttribute('rel', 'noopener noreferrer')
      }
      walk(child)
    }
  }
  walk(doc.body)
  return doc.body.innerHTML
}

/** HTML 实体转义 —— 净化失败时的降级路径 */
function escapeText (s) {
  return String(s)
    .replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;').replace(/'/g, '&#39;')
}

export default sanitizeHtml
