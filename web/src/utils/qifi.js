/**
 * QIFI 数据处理工具类
 * 基于 /home/quantaxis/qapro/qaotcweb/src/components/qifi/libs/js/qifi.js
 * 适配 qaexchange-rs 的数据结构
 */

import dayjs from 'dayjs'

/**
 * QIFI 账户数据处理类
 */
export class QifiAccount {
  constructor(data) {
    // 兼容两种格式：直接 QIFI 对象 或 { data: qifi } 包装
    const type = Object.prototype.toString.call(data.data)
    let result = type === '[object Array]' ? data.data : [data.data || data]
    this.res = result[0]
  }

  /**
   * 获取账户基本信息
   * @returns {Object} 账户信息对象
   */
  getAccountInfo() {
    if (!this.res) return null

    const accounts = this.res.accounts || {}

    // 计算总盈亏 = 平仓盈亏 + 浮动盈亏 - 手续费
    accounts.profit = (accounts.close_profit || 0) +
                     (accounts.float_profit || 0) -
                     (accounts.commission || 0)

    // 格式化更新时间
    if (this.res.updatetime) {
      accounts.updatetime = this.res.updatetime.split('.')[0]
    }

    // 添加投资者姓名
    if (this.res.investor_name) {
      accounts.investor_name = this.res.investor_name
    }

    // 移除不需要的字段
    const excludeFields = [
      'frozen_margin',
      'frozen_commission',
      'frozen_premium',
      'deposit',
      'withdraw',
      'WithdrawQuota',
      'premium',
      'currency'
    ]
    excludeFields.forEach(field => {
      delete accounts[field]
    })

    return accounts
  }

  /**
   * 获取成交记录列表
   * @returns {Array} 成交记录数组
   */
  getTrades() {
    if (!this.res || !this.res.trades) return []

    let trades = Object.values(this.res.trades)

    // 格式化时间和排序
    trades = trades.map(item => {
      if (item.trade_date_time) {
        // 处理微秒时间戳 (10e5 = 1000000)
        item.trade_date_time = dayjs(item.trade_date_time / 10e5).format('YYYY-MM-DD HH:mm:ss')
      }
      return item
    })

    // 按时间倒序排序
    trades.sort((a, b) => {
      return dayjs(b.trade_date_time).valueOf() - dayjs(a.trade_date_time).valueOf()
    })

    return trades
  }

  /**
   * 获取持仓列表
   * @returns {Array} 持仓数组
   */
  getPositions() {
    if (!this.res || !this.res.positions) return []

    let positions = Object.values(this.res.positions)

    // 格式化时间
    positions = positions.map(item => {
      if (item.trade_date_time) {
        item.trade_date_time = dayjs(item.trade_date_time / 10e5).format('YYYY-MM-DD HH:mm:ss')
      }
      return item
    })

    // 按合约代码排序
    positions.sort((a, b) => {
      return (a.instrument_id + '').localeCompare(b.instrument_id + '')
    })

    return positions
  }

  /**
   * 获取订单列表
   * @returns {Array} 订单数组
   */
  getOrders() {
    if (!this.res || !this.res.orders) return []

    let orders = Object.values(this.res.orders)

    // 格式化时间
    orders = orders.map(item => {
      if (item.insert_date_time) {
        item.insert_date_time = dayjs(item.insert_date_time / 10e5).format('YYYY-MM-DD HH:mm:ss')
      }
      return item
    })

    // 按插入时间倒序排序
    orders.sort((a, b) => {
      return dayjs(b.insert_date_time).valueOf() - dayjs(a.insert_date_time).valueOf()
    })

    return orders
  }

  /**
   * 为K线图生成交易标记点
   * @param {Array} trades - 成交记录数组
   * @returns {Array} 标记点数组
   */
  getChartDots(trades) {
    const allDots = []

    const buyColorConfig = {
      color: 'rgb(255,255,255)',
      backgroundColor: 'rgba(233,30,99,0.5)' // 红色 - 买入
    }

    const sellColorConfig = {
      color: 'rgb(255,255,255)',
      backgroundColor: 'rgba(0,150,136,0.5)' // 绿色 - 卖出
    }

    trades.forEach(trade => {
      const obj = {}
      let config = {}

      // 提取日期（YYYYMMDD 格式）
      const date = Number(dayjs(trade.trade_date_time).format('YYYYMMDD'))
      obj.symbol = trade.instrument_id

      // 提取时间（HHMM 格式）
      let dealTime = trade.trade_date_time.split(' ')[1]
      dealTime = dealTime.split(':')
      const time = Number(dealTime[0] + '' + dealTime[1])

      // 处理夜盘（21:00-23:59）
      if (time >= 2100 && time <= 2359) {
        obj.flag = true
      } else {
        obj.flag = false
      }

      // 开平标记
      const offset = trade.offset && trade.offset.includes('OPEN') ? 'O' : 'C'
      const offsetCN = trade.offset && trade.offset.includes('OPEN') ? '开仓' : '平仓'

      // 方向标记
      const direction = trade.direction === 'BUY' ? 'B' : 'S'
      const directionCN = trade.direction === 'BUY' ? '买入' : '卖出'

      // 设置颜色
      config = direction === 'B' ? buyColorConfig : sellColorConfig

      // 格式化价格
      const price = Number.isInteger(trade.price)
        ? trade.price
        : trade.price.toFixed(2)

      obj.date = date
      const TIME = date.toString().slice(4, 8)

      obj.data = {
        Date: date,
        Time: time,
        Title: direction + ' ' + offset + ' ' + price,
        Color: config.color,
        BGColor: config.backgroundColor,
        Price: price,
        Content: TIME + '=>' + directionCN + offsetCN + trade.volume + '手'
      }

      allDots.push(obj)
    })

    return allDots
  }
}

/**
 * 行情五档数据处理类
 */
export class QifiQuotation {
  constructor(data) {
    this.data = data
    // 区分期货和股票（通过 gateway_name 字段）
    this.type = data.gateway_name ? 'future' : 'stock'
  }

  /**
   * 解析五档行情数据
   * @returns {Object} { buy: [], sell: [] }
   */
  parse() {
    if (this.type === 'stock') {
      return this.parseStock()
    } else {
      return this.parseFuture()
    }
  }

  /**
   * 解析股票五档行情
   */
  parseStock() {
    const res = { buy: [], sell: [] }
    const { BuyPrices, BuyVols, SellPrices, SellVols } = this.data

    for (let i = 0; i < 5; i++) {
      res.buy.push({
        index: i + 1,
        price: BuyPrices[i],
        volume: BuyVols[i]
      })

      res.sell.unshift({
        index: i + 1,
        price: SellPrices[i],
        volume: SellVols[i]
      })
    }

    return res
  }

  /**
   * 解析期货五档行情
   */
  parseFuture() {
    const res = { buy: [], sell: [] }

    for (let i = 1; i <= 5; i++) {
      res.buy.push({
        index: i,
        price: this.data['bid_price_' + i],
        volume: this.data['bid_volume_' + i]
      })

      res.sell.unshift({
        index: i,
        price: this.data['ask_price_' + i],
        volume: this.data['ask_volume_' + i]
      })
    }

    return res
  }
}

/**
 * 将 qaexchange-rs 的账户数据转换为 QIFI 格式
 * @param {Object} account - QA_Account 对象
 * @param {Array} positions - 持仓数组
 * @param {Array} orders - 订单数组
 * @param {Array} trades - 成交数组
 * @returns {Object} QIFI 格式对象
 */
export function convertToQifi(account, positions = [], orders = [], trades = []) {
  const qifi = {
    account_cookie: account.user_id,
    user_id: account.user_id,
    investor_name: account.user_name || account.user_id,
    updatetime: new Date().toISOString(),
    accounts: {
      user_id: account.user_id,
      currency: 'CNY',
      pre_balance: account.balance - account.deposit + account.withdraw,
      deposit: account.deposit || 0,
      withdraw: account.withdraw || 0,
      close_profit: account.close_profit || 0,
      commission: account.commission || 0,
      position_profit: 0, // 从持仓计算
      float_profit: 0,    // 从持仓计算
      balance: account.balance,
      margin: account.margin,
      available: account.available,
      risk_ratio: account.risk_ratio || 0,
      static_balance: account.balance - account.float_profit
    },
    positions: {},
    orders: {},
    trades: {}
  }

  // 转换持仓
  positions.forEach(pos => {
    const key = `${pos.instrument_id}_${pos.direction}`
    qifi.positions[key] = {
      instrument_id: pos.instrument_id,
      exchange_id: pos.exchange_id || 'UNKNOWN',
      volume_long: pos.direction === 'LONG' ? pos.volume : 0,
      volume_short: pos.direction === 'SHORT' ? pos.volume : 0,
      open_price_long: pos.direction === 'LONG' ? pos.open_price : 0,
      open_price_short: pos.direction === 'SHORT' ? pos.open_price : 0,
      position_profit: pos.position_profit || 0,
      float_profit: pos.float_profit || 0,
      last_price: pos.last_price || pos.open_price,
      margin: pos.margin || 0,
      trade_date_time: new Date(pos.open_time || Date.now()).getTime() * 10e5
    }

    // 累计持仓盈亏
    qifi.accounts.position_profit += pos.position_profit || 0
    qifi.accounts.float_profit += pos.float_profit || 0
  })

  // 转换订单
  orders.forEach(order => {
    qifi.orders[order.order_id] = {
      order_id: order.order_id,
      instrument_id: order.instrument_id,
      exchange_id: order.exchange_id || 'UNKNOWN',
      direction: order.direction,
      offset: order.offset,
      price: order.price,
      volume: order.volume,
      volume_left: order.volume - order.volume_traded,
      status: order.status,
      insert_date_time: new Date(order.insert_time || Date.now()).getTime() * 10e5
    }
  })

  // 转换成交
  trades.forEach(trade => {
    qifi.trades[trade.trade_id] = {
      trade_id: trade.trade_id,
      order_id: trade.order_id,
      instrument_id: trade.instrument_id,
      exchange_id: trade.exchange_id || 'UNKNOWN',
      direction: trade.direction,
      offset: trade.offset,
      price: trade.price,
      volume: trade.volume,
      commission: trade.commission || 0,
      trade_date_time: new Date(trade.trade_time || Date.now()).getTime() * 10e5
    }
  })

  return qifi
}

/**
 * 数字格式化过滤器（保留2位小数）
 * @param {Number} val - 数值
 * @returns {String|Number} 格式化后的值
 */
export function toFixed(val) {
  if (typeof val !== 'number') return val
  return Number.isInteger(val) ? val : val.toFixed(2)
}

/**
 * 百分比格式化
 * @param {Number} val - 数值（0-1）
 * @returns {String} 百分比字符串
 */
export function toPercent(val) {
  if (typeof val !== 'number') return '0.00%'
  return (val * 100).toFixed(2) + '%'
}

export default {
  QifiAccount,
  QifiQuotation,
  convertToQifi,
  toFixed,
  toPercent
}
