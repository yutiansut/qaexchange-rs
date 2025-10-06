import axios from 'axios'
import { Message } from 'element-ui'

// 创建 axios 实例
const service = axios.create({
  baseURL: '/api',
  timeout: 30000
})

// 请求拦截器
service.interceptors.request.use(
  config => {
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

    // 处理标准响应格式 { success, data, error }
    if (res.hasOwnProperty('success')) {
      if (res.success) {
        return res.data
      } else {
        Message.error(res.error && res.error.message || '请求失败')
        return Promise.reject(new Error(res.error && res.error.message || '请求失败'))
      }
    }

    // 直接返回数据
    return res
  },
  error => {
    console.error('Response error:', error)
    Message.error(error.message || '网络错误')
    return Promise.reject(error)
  }
)

export default service
