import Vue from 'vue'
import App from './App.vue'
import router from './router'
import store from './store'
import ElementUI from 'element-ui'
import 'element-ui/lib/theme-chalk/index.css'
// @yutiansut @quantaxis - 全局主题样式
import './styles/index.scss'
// 暂时注释掉 vxe-table 以避免兼容性问题
// import VXETable from 'vxe-table'
// import 'vxe-table/lib/style.css'
import * as echarts from 'echarts'
import dayjs from 'dayjs'

// 全局配置
Vue.config.productionTip = false

// Element UI
Vue.use(ElementUI, { size: 'small' })

// VXE Table - 暂时注释掉
// Vue.use(VXETable)

// 全局挂载
Vue.prototype.$echarts = echarts
Vue.prototype.$dayjs = dayjs

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
