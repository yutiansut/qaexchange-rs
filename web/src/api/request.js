import axios from 'axios'
import { Message } from 'element-ui'

// 创建 axios 实例
// @yutiansut @quantaxis
// 使用环境变量配置 API 地址，支持开发和生产环境
const service = axios.create({
  baseURL: process.env.VUE_APP_API_BASE_URL || '/api',
  timeout: 30000
})

// 请求拦截器

// ✨ 标记「已经弹过提示」@yutiansut @quantaxis
//
// main.js 的 unhandledrejection 兜底会对未捕获的拒绝再弹一次。
// 拦截器业务错误 reject 的是 `new Error(msg)` —— **没有 `.response`**,
// 光靠「有没有 response」区分不出「已弹过」,会双弹。
// 显式打标记比猜形状可靠。
//
// 语义是「已呈现给用户」而不单是「弹过 toast」:认证接口(登录/注册)这里**不弹**,
// 但 `login.vue:243` / `register.vue:349` 各自 try/catch 后调 extractErrorMessage
// 自己呈现(已核实,非仅凭注释)—— 那条路径根本走不到 unhandledrejection,
// 标记与否都不影响,标记也不算说谎。
function rejected (err) {
  try { err.__qaToasted = true } catch (e) { /* 冻结对象 */ }
  // ✨ 顺手把**服务端消息**挂回 `err.message`。@yutiansut @quantaxis
  //
  // 原先 reject 出去的是原始 axios error,`.message` 恒为
  // `Request failed with status code 400` 这类通用串,服务端真正说的
  // (`Account error: Account not found: NOSUCH123`)只留在 `err.response` 里。
  // 上一轮我是**逐个调用点**改成 `$errMsg(...)` 解决的(42 处 / 21 个文件,
  // 已在浏览器里 15/15 页验过);但那之外**还有 7 处**在读原始 `message`,
  // 且以后新写的代码还会踩。在这里挂一次,那 7 处与将来的新代码都自动拿到原文。
  //
  // 与 `$errMsg` **不冲突**:`extractErrorMessage` 先读 `err.response.data.error.message`,
  // 拿到的是同一个串;即便回落到 `err.message`,现在也已经是原文。
  // 原始串保留在 `axiosMessage`,`err.response` 原样不动。
  // 复用同文件的 `extractErrorMessage` 而不是自己再写一遍解析 ——
  // 它还处理了「登录把业务消息放在 `data.data.message`」那种形状(见下方注释),
  // 手写只看 `data.error.message` 会漏掉。它取不到时会回落到 `err.message`,
  // 此时下面的相等判断为真、不做任何改动。
  try {
    var msg = extractErrorMessage(err, null)
    if (msg && err.message !== msg) {
      err.axiosMessage = err.message
      err.message = msg
    }
  } catch (e) { /* 冻结对象或结构异常:保持原样 */ }
  return Promise.reject(err)
}

service.interceptors.request.use(
  config => {
    // ✨ 带上 JWT @yutiansut @quantaxis
    //
    // 此前这个拦截器是空的(直接 return config),从不发 Authorization 头 ——
    // 而 token 在登录时就已写入 localStorage(store/index.js:159),从没被用过。
    //
    // 后果:后端一旦开启管理端鉴权(QAEX_REQUIRE_ADMIN_AUTH=1),
    // /api/admin/*、/api/management/*、/api/account-admin/* 全部 401,
    // 整个管理端界面不可用 —— 这就是那个开关一直不能开的真正原因。
    //
    // 加上之后:
    //   · 鉴权关闭时 —— 后端忽略这个头,行为不变
    //   · 鉴权开启时 —— 管理员登录后界面正常,非管理员收到 403(符合预期)
    const token = localStorage.getItem('token')
    if (token) {
      config.headers = config.headers || {}
      config.headers['Authorization'] = `Bearer ${token}`
    }
    return config
  },
  error => {
    console.error('Request error:', error)
    return Promise.reject(error)
  }
)

// 响应拦截器
service.interceptors.response.use(
  response => {
    const res = response.data

    // ⚠️ 顺序要紧:下面的 `res.hasOwnProperty` 对 null 会直接抛 TypeError,
    //    所以「空/非对象」的判断必须排在它**前面**。原先放在后面 ——
    //    实测裸 JSON `null` 的提示是浏览器原生的
    //    `Cannot read properties of null (reading 'hasOwnProperty')`,
    //    而不是设计好的中文;而空 body / `""` / 代理返回的 HTML 502 页面
    //    (HTTP 200)会让 res 变成**字符串**,既非 null 又非空对象,
    //    静默透传成「暂无数据」。@yutiansut @quantaxis
    if (res === null || res === undefined) {
      const msg = '服务返回空数据(接口异常)'
      Message.error(msg)
      return rejected(new Error(msg))
    }
    if (typeof res !== 'object') {
      const msg = '服务返回了非预期格式(接口异常)'
      Message.error(msg)
      return rejected(new Error(msg))
    }

    // ✨ 把 X-Total-Count 挂到返回值上 —— **必须在信封判断之外** @yutiansut @quantaxis
    //
    // 本拦截器只返回 body 的一部分,**响应头随之丢失**。而
    // `/api/management/transactions/{userId}` 的 `data` 是个数组、没有 total 字段,
    // 于是前端只能 `total = 数组长度` —— 分页条显示「总数 = 当前页条数」,
    // 把分页失效掩盖成「看起来正常」(本会话实测,后端已补分页 + 该响应头)。
    //
    // ⚠️ 我第一版把挂载写在 `res.hasOwnProperty('success')` 分支**里面**,
    //    于是**真裸数组**形状的端点会静默丢掉总数(回验实测:fetch 明明读得到
    //    `x-total-count=137`,拦截器不挂 ⇒ 分页条退回「共 3 条」)。
    //    ⚠️ 我曾在这里写「本仓当前所有端点都走信封」—— **那是错的**,
    //    与下面 :125 自己列的例外自相矛盾。2026-09-06 逐个实测 61 个 GET,
    //    有 7 个返回裸 JSON(`/api/monitoring/{system,status,accounts,orders,
    //    storage,trades}` 与 `/health`),都不带 success 字段。
    //    它们目前恰好都是**对象**而非数组,所以挂载仍然有效 —— 但正是
    //    「自洽的错误值掩盖功能失效」那一类,提到分支外一劳永逸。
    //
    // 用不可枚举属性挂载:数组/对象都能挂,不影响 JSON 序列化、
    // 不进 for...in / Object.keys,不打断任何既有调用方。
    const attachTotal = payload => {
      const tc = response.headers && (response.headers['x-total-count'] ||
                                      response.headers['X-Total-Count'])
      if (tc != null && payload && typeof payload === 'object') {
        try {
          Object.defineProperty(payload, '__total', {
            value: Number(tc) || 0, enumerable: false, configurable: true
          })
        } catch (e) { /* 冻结对象忽略 */ }
      }
      return payload
    }

    // 处理标准响应格式 { success, data, error }
    if (res.hasOwnProperty('success')) {
      if (res.success) {
        return attachTotal(res.data)
      } else {
        const errorMsg = res.error && res.error.message || '请求失败'
        Message.error(errorMsg)
        return rejected(new Error(errorMsg))
      }
    }

    // ✨ 非预期形状 —— 必须让用户看见 @yutiansut @quantaxis
    //
    // 原先「不带 success 字段的响应」一律原样透传,于是 HTTP 200 但形状不对时
    // (后端异常返回裸对象、或代理插了一层)**完全无声**,页面直接显示「暂无数据」。
    // 实测 6 个页面 × 2 种畸形响应 = 12 组,**9 组零提示**。
    //
    // 不能一刀切:确实有接口不走 {success,data,error} 信封 ——
    // `/api/monitoring/system`、`/api/monitoring/status` 返回裸对象、
    // K线接口用 {code,message,data},实测都是**非空对象**。
    // 所以只对「明显不可用」的形状报错:null / undefined / 空对象。
    if (typeof res === 'object' && !Array.isArray(res) && Object.keys(res).length === 0) {
      const msg = '服务返回空响应(接口异常)'
      Message.error(msg)
      return rejected(new Error(msg))
    }

    // 直接返回数据
    // 非信封形状(真裸数组/裸对象)也要挂总数 —— 见上面 attachTotal 的说明。
    // 回验实测:挂载只写在信封分支里时,真裸数组端点会**静默丢掉** X-Total-Count
    // (fetch 读得到 137,分页条却退回「共 3 条」)。@yutiansut @quantaxis
    return attachTotal(res)
  },
  error => {
    console.error('Response error:', error)
    const status = error.response && error.response.status

    // ✨ 401/403:清 token 并回登录页 @yutiansut @quantaxis
    //
    // 原先没有任何状态码分支。实测注入 401:页面 hash 停在 `#/admin-accounts`、
    // `localStorage.token` 仍在,只有两条 3 秒消失的红条,之后是一个空白管理页 ——
    // token 过期时用户会以为「数据没了」,而不是「需要重新登录」。
    // ⚠️ 排除认证接口本身:后端对**密码错误**也返回 401
    //    (实测 `{"success":true,"data":{"success":false,"message":"Invalid password"},"error":null}`)。
    //    若不排除,输错密码会被报成「登录已过期,请重新登录」—— 比原来更误导。
    //    这类失败交给页面自己的 extractErrorMessage 处理。@yutiansut @quantaxis
    const reqUrl = (error.config && error.config.url) || ''
    const isAuthEndpoint = /\/auth\/(login|register)/.test(reqUrl)

    // ⚠️ 403 ≠ 会话失效。403 是「这个账号没权限看这一页」(非管理员点管理菜单
    //    就会触发),会话本身仍然有效 —— 清 token 把人踢下线是过度反应。
    //    只提示,不动登录态。@yutiansut @quantaxis
    if (status === 403 && !isAuthEndpoint) {
      Message.error('没有访问权限')
      return rejected(error)
    }

    if (status === 401 && !isAuthEndpoint) {
      const already = window.location.hash.indexOf('#/login') === 0
      Message.error('登录已过期,请重新登录')
      if (!already) {
        // ⚠️ 必须走 Vuex 的 logout,不能只删 localStorage。@yutiansut @quantaxis
        //
        // `store/index.js:91` 的 `isLoggedIn` **只在模块加载时**读一次
        // localStorage,之后仅由 `SET_TOKEN`(:157)维护。只删 localStorage 会造成:
        //     store.getters.isLoggedIn === true   (陈旧)
        //     localStorage.token       === null   (已清)
        // 于是 `router/index.js:242` 的守卫看到「已登录」,把跳转到 #/login 的动作
        // 直接弹回 #/dashboard —— 实测 hash 轨迹 ["#/admin-accounts", "#/dashboard"],
        // 中间那步 #/login 连一次 hashchange 都没留下。
        // 结果比原缺陷更糟:界面显示「已登录」,但后续所有请求都不再带
        // Authorization 头(实测 monitoring/position/market 全部 authorization:false)
        // —— 僵尸会话。
        //
        // 懒 require 而非顶层 import:store → @/api → request.js 是环状依赖,
        // 顶层 import 在模块求值期会拿到 undefined;调用期图已建完,是安全的。
        try {
          const mod = require('@/store')
          const store = mod && (mod.default || mod)
          if (store && store.dispatch) {
            store.dispatch('logout')
          } else {
            throw new Error('store unavailable')
          }
        } catch (e) {
          try {
            localStorage.removeItem('token')
            localStorage.removeItem('currentUser')
          } catch (e2) { /* 隐私模式下 localStorage 可能不可用 */ }
        }
        const from = encodeURIComponent(window.location.hash.slice(1) || '/')
        window.location.hash = '#/login?redirect=' + from
      }
      return rejected(error)
    }

    // ⚠️ 用 extractErrorMessage 而不是只看 `data.error.message`。
    //    登录 401 的响应体是 `{"success":true,"data":{"success":false,
    //    "message":"Invalid password"},"error":null}` —— `error` 是 null,
    //    取值链断掉后回退成 axios 原文,用户会先看到一条英文
    //    `Request failed with status code 401` 再看到中文业务消息。
    //    extractErrorMessage 已经能处理 `data.data.message` 这一层。
    //    @yutiansut @quantaxis
    // 认证接口(登录/注册)由页面自己弹提示 —— `login.vue:244` / `register.vue:350`
    // 都已调用 extractErrorMessage。这里再弹一条就是**同文重复两条**。
    // 静默交还给页面。@yutiansut @quantaxis
    if (!isAuthEndpoint) {
      // ✨ 区分「业务拒绝」与「系统故障」—— 两者不该长得一样。@yutiansut @quantaxis
      //
      // 交易系统里 400 绝大多数是**正常的业务判定**:资金不足、订单量低于最小值、
      // 合约不存在、出金超额、撤不存在的单……实测 11 个变更场景有 7 个返回 400,
      // 其中 5 个是完全正确的拒绝。把它们和「服务挂了」用同一种红色错误条呈现,
      // 用户的体感就是「很多接口都在报错」,而系统其实工作正常。
      //
      // 错误码没有可用的语义分层(13 处裸 400 + 零散的 4001/4006/4010/4040),
      // 所以用一条可靠的结构判据:
      //   HTTP 4xx **且**响应体带结构化 `error.message` ⇒ 后端主动做出的业务判定 ⇒ warning
      //   5xx / 无结构化消息 / 网络失败                  ⇒ 系统故障            ⇒ error
      const body = (error.response && error.response.data) || null
      const hasBizMsg = !!(body && ((body.error && body.error.message) ||
                                    (body.data && body.data.message)))
      const isBizReject = status >= 400 && status < 500 && hasBizMsg
      const text = extractErrorMessage(error, '网络错误')
      if (isBizReject) {
        Message.warning(text)
      } else {
        Message.error(text)
      }
    }
    return rejected(error)
  }
)

/**
 * 从 axios 错误里取出**人能看懂**的消息 @yutiansut @quantaxis
 *
 * ⚠️ 后端的错误体是 `error: { code, message }` —— 是个**对象**。
 * 全仓有 7 处直接写 `'xxx失败: ' + error.response.data.error`,
 * 字符串拼接会把它变成 **`[object Object]`**。
 *
 * 实测(下单接口):
 *   HTTP 200 + {success:false, error:{message:"保证金不足,可用资金 1234.00 元"}}
 *     → 「订单提交失败: 保证金不足,可用资金 1234.00 元」  ✓(这条路径没走拼接)
 *   HTTP 400 + 同一个错误体
 *     → 「订单提交失败: [object Object]」                 ✗
 *
 * 另有登录场景:后端对错误密码返回 HTTP 401 +
 * `{"success":true,"data":{"success":false,"message":"Invalid password"},"error":null}`
 * —— `error` 是 null,取不到,于是回退成 axios 原始串
 * 「Request failed with status code 401」,而后端给的 "Invalid password" 全程没被用上。
 * 这里把 `data.data.message` 也纳入回退链。
 */
export function extractErrorMessage(error, fallback) {
  const data = error && error.response && error.response.data
  if (data) {
    const e = data.error
    if (e) {
      if (typeof e === 'string') return e
      if (typeof e === 'object' && e.message) return e.message
    }
    // 后端有的路径把业务失败放在 data.data.message(如登录密码错误)
    if (data.data && typeof data.data === 'object' && data.data.message) {
      return data.data.message
    }
    if (typeof data.message === 'string') return data.message
  }
  return (error && error.message) || fallback || '请求失败'
}

export default service
