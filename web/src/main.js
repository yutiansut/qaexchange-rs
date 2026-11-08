import Vue from 'vue'
import { extractErrorMessage } from '@/api/request'
import App from './App.vue'
import router from './router'
import store from './store'
import ElementUI from 'element-ui'
import { Message } from 'element-ui'
import 'element-ui/lib/theme-chalk/index.css'
// @yutiansut @quantaxis - 全局主题样式
import './styles/index.scss'
// vxe-table 暂时禁用（xe-utils 版本不兼容）@yutiansut @quantaxis
// import VXETable from 'vxe-table'
// import 'vxe-table/lib/style.css'
import * as echarts from 'echarts'
import dayjs from 'dayjs'

// 全局配置
Vue.config.productionTip = false

// Element UI
Vue.use(ElementUI, { size: 'small' })

// VXE Table - 暂时禁用
// Vue.use(VXETable)

// 全局挂载
Vue.prototype.$echarts = echarts
Vue.prototype.$dayjs = dayjs

// 表格空态文案 —— 把「加载失败」和「真的没数据」分开 @yutiansut @quantaxis
//
// el-table 的默认 empty-text 是「暂无数据」。请求 4xx/5xx/超时被 catch 掉后,
// 绑定的数组仍是 `[]`,表格照样渲染成「暂无数据」—— 与真的没数据**一模一样**。
// 全仓 35 张带 :data 的表里,只有 7 张自己写了三元区分,其余 28 张失败时静默留白。
// 这正是「页面无法显示」最常见的来源:后端报错了,用户看到的却是「暂无数据」。
//
// 挂在 Vue.prototype 上(与上面 $echarts / $dayjs 同一模式,不新增文件),
// 模板里统一写 `:empty-text="$emptyText(loadError, '暂无成交')"`。
// 组件仍需自己在 catch 里置 `this.loadError` —— 那是组件私有状态,无法全局化。
Vue.prototype.$emptyText = function (err, empty) {
  if (!err) return empty || '暂无数据'
  var msg = (err && err.message) || err
  return '加载失败：' + String(msg)
}

// ============ 数值显示兜底 —— 让「没有」不再被显示成「零」 @yutiansut @quantaxis ============
//
// ⚠️ 全仓有 **153 处** `(Number(x) || 0)`,其中 **97 处是直接显示**。
//    `||` 对 `null` / `undefined` / `''` / `NaN` / `0` **一视同仁**地取 0,
//    于是「这个账户没有数据」和「这个账户是 0」在界面上**完全同形**。
//    子代理实测:`fmtMoney(Infinity)` / `(NaN)` / `(null)` / `('abc')` 全都渲染成 `0.00`。
//    比长数字串更危险 —— 长串至少看得出不对劲,`0.00` 看不出。
//
// 关键性质:**对真实的 0,行为完全不变**(仍显示 `0.00`);
//    只有 null/''/NaN/Inf/超限 才变成 `--`。所以替换是低风险的。
//
// 量级上限 1e15:线上存在 `balance = 1e300` 的污染账户,而
//    **`Number.isFinite(1e300) === true`** —— 只判 isFinite 拦不住它。
//    1e300 渲染出来是 404 个字符,会把整列撑爆。
var NUM_MAX = 1e15
function numOk (v) {
  if (v === null || v === undefined || v === '') return null
  var n = Number(v)
  if (!Number.isFinite(n)) return null
  if (Math.abs(n) >= NUM_MAX) return null
  return n
}
// 定点显示:`$num(v, 2)`
Vue.prototype.$num = function (v, digits) {
  var n = numOk(v)
  return n === null ? '--' : n.toFixed(digits === undefined ? 2 : digits)
}
// 金额显示(带千分位):`$money(v)`
Vue.prototype.$money = function (v, digits) {
  var n = numOk(v)
  if (n === null) return '--'
  var d = digits === undefined ? 2 : digits
  return n.toLocaleString('zh-CN', { minimumFractionDigits: d, maximumFractionDigits: d })
}
// 错误消息提取:`$errMsg(error, '加载失败')` @yutiansut @quantaxis
//
// axios 的 `error.message` 对任何 HTTP 错误都只给通用串
// `Request failed with status code 400`,而服务端真正说的是
// `Account error: Account not found: NOSUCH123` —— 用户看到的是一句
// 与自己无关的英文,真正有用的那句被丢掉了。
//
// `extractErrorMessage`(api/request.js)早就存在,`login.vue` / `register.vue`
// 在用,但全仓还有 **42 处**直接取 `error.message`。它的最后一行就是
// `return error.message || fallback`,所以是原写法的**严格超集** ——
// 替换只会增加信息,不会丢。提成全局是为了避免 21 个文件各加一行 import。
Vue.prototype.$errMsg = function (err, fallback) {
  return extractErrorMessage(err, fallback)
}

// ECharts 空态/加载态标题:`$chartTitle('暂无结算记录', '系统尚未执行过日终结算')`
// @yutiansut @quantaxis
//
// 「只有两条轴线、无刻度、无数据、无文字的空框」是本仓库里「无法显示」最常见的形态:
// 它既不报错也不解释,用户分不清「还没加载」「加载失败」「真的没数据」。
// dashboard 的资金分布图已经用四态文案修过一轮,`/admin-settlement`「结算统计」
// 还是空框 —— 提到这里让两处**共用一份**样式,不再各写各的。
//
// ⚠️ 传 null 时返回 `{ text: '', subtext: '' }` 而**不是** `{}` ——
//    `setOption` 是合并语义,省略键等于沿用上一次的文案(见 lessons L153)。
Vue.prototype.$chartTitle = function (text, subtext) {
  if (!text) return { text: '', subtext: '' }
  return {
    text: text,
    subtext: subtext || '',
    left: 'center',
    top: 'center',
    textStyle: { fontSize: 14, color: '#909399' },
    subtextStyle: { fontSize: 12, color: '#c0c4cc' }
  }
}
// 百分比显示:`$pct(v, 2)` —— 入参是**比率**(0.0153),输出 `1.53%`
Vue.prototype.$pct = function (v, digits) {
  var n = numOk(v)
  return n === null ? '--' : (n * 100).toFixed(digits === undefined ? 2 : digits) + '%'
}

// ============ 全局错误兜底 —— 让「静默空白」不再可能 @yutiansut @quantaxis ============
//
// ⚠️ 此前全仓**没有任何**全局错误处理:无 `Vue.config.errorHandler`、
//    无 `window.onerror`、无 `unhandledrejection`。后果是渲染期异常完全静默:
//    模板里对 null 数值调 `.toFixed()` 抛 TypeError ⇒ **整张表空白**,
//    无 toast、无 el-alert、**连 `pageerror` 都没有**,只有 console 里一条 Vue warn。
//
//    这条路径**绕过了** `$emptyText` 那套「加载失败 vs 真没数据」的区分 ——
//    渲染崩溃时请求是**成功**的、`loadError` 为 null,表格走「暂无数据」分支。
//    用户看到「没有数据」,真相是「有数据但渲染崩了」。
//
//    逐字段加 `|| 0` 是打地鼠(已补 30 处模板调用 + 10 处 reduce),
//    这里做系统性兜底:任何漏网的渲染异常至少**会被看见**。
// 去重 + 限流 —— 渲染错误会随每次重渲染反复触发(滚动、排序、轮询刷新),
// 不限流会造 toast 风暴,比静默更糟。同一消息 10s 内只弹一次,总量 5 条封顶。
var _errSeen = Object.create(null)
var _errCount = 0
function reportErr (prefix, msg) {
  var key = prefix + '|' + msg
  var now = Date.now()
  if (_errSeen[key] && now - _errSeen[key] < 10000) return
  _errSeen[key] = now
  if (++_errCount > 5) {
    if (_errCount === 6) {
      try { Message.error('界面持续报错,已停止提示,请查看控制台') } catch (e) {}
    }
    return
  }
  try { Message.error(prefix + msg) } catch (e) { /* Message 本身不可用时不要再抛 */ }
}

Vue.config.errorHandler = function (err, vm, info) {
  // 仍然打到 console,保留原有排查手段
  console.error('[Vue errorHandler]', info, err, vm && vm.$options && vm.$options.name)
  var where = (vm && vm.$options && (vm.$options.name || vm.$options._componentTag)) || '页面'
  reportErr('界面渲染出错(' + where + ')：', (err && err.message) || String(err))
}

// 未捕获的 Promise 拒绝 —— 大多来自忘记 catch 的异步加载。
// `__qaToasted` 由 request.js 的 rejected() 打上:拦截器已经弹过的不再重复弹。
window.addEventListener('unhandledrejection', function (ev) {
  var r = ev && ev.reason
  if (r && (r.__qaToasted || r.response)) return
  console.error('[unhandledrejection]', r)
  reportErr('未处理的异步错误：', (r && r.message) || String(r))
})

// ============ 跨标签登出同步 @yutiansut @quantaxis ============
//
// ⚠️ 此前:在一个标签点「退出登录」后,**其它标签仍是完整可用的交易界面**。
//    子代理用真实 UI 实测(两标签同 context,标签 A 走真实「退出登录」→确定):
//      标签 B:token = "",localStorage 键数 = 0   ← 凭证已没了
//              hash = #/positions,loginForm = false ← 界面仍是持仓页
//              仍显示持仓行、WebSocket closed=false、消息数 9→14→15→56 不断增长
//              点「刷新」正常刷新不跳登录;SPA 内跳 /trades 正常渲染,零 token
//    根因两条:
//      ① `logout()` 调 `localStorage.clear()`,而 localStorage 是**全标签共享**的,
//         但没有任何 `storage` 事件监听 —— 其它标签不知道自己被登出了。
//      ② `router.beforeEach` 判的是 `store.getters.isLoggedIn`,读**内存里的 Vuex**,
//         不是 localStorage。内存态没人改,守卫就一路放行。
//    (标签 B 硬刷新后才恢复,即不刷新就永久停在「看似已登录」。)
//
// 放在 main.js 而不是 store/index.js:store 不能 import router(router 已 import store,
// 会成环),而这里 router 与 store 都在手边 —— 与上面的 unhandledrejection 同一模式,
// 不新增文件。
window.addEventListener('storage', function (ev) {
  // localStorage.clear() 触发 key === null;removeItem('token') 触发 key === 'token'。
  // 其它键的变动(userInfo/roles/…)不代表登出,忽略。
  if (ev.key !== null && ev.key !== 'token') return
  // 另一标签是**登录**或续期(token 还在)—— 不是登出,不要把自己踢掉。
  if (localStorage.getItem('token')) return
  // 本标签已经是登出态:直接返回,避免多标签间来回级联。
  if (!store.getters.isLoggedIn) return

  // 只清内存态 —— **不要**再调 `store.dispatch('logout')`,那会又执行一次
  // `localStorage.clear()`,向其余标签再播一轮 storage 事件。
  // 但 WebSocket 必须断:否则本标签的连接会继续推送已登出用户的数据。
  store.commit('SET_TOKEN', '')
  store.commit('SET_USER_INFO', null)
  store.commit('SET_CURRENT_USER', '')
  store.commit('SET_ROLES', [])
  store.commit('SET_PERMISSIONS', [])
  try { store.dispatch('websocket/destroyWebSocket') } catch (e) { /* 未初始化时忽略 */ }

  var cur = router.currentRoute
  if (cur && (cur.path === '/login' || cur.path === '/register')) return
  try { Message.warning('您已在其它标签页退出登录') } catch (e) {}
  // 带 redirect,与 router/index.js 里守卫的跳转形状保持一致。
  router.push({ path: '/login', query: { redirect: cur ? cur.fullPath : '/' } })
    .catch(function () { /* NavigationDuplicated:已在登录页,忽略 */ })
})

// NProgress 配置
import NProgress from 'nprogress'
import 'nprogress/nprogress.css'
NProgress.configure({ showSpinner: false })

// 路由拦截
router.beforeEach((to, from, next) => {
  NProgress.start()
  next()
})

router.afterEach(() => {
  NProgress.done()
})

new Vue({
  router,
  store,
  render: h => h(App)
}).$mount('#app')
