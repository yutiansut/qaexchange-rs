Differential Information Flow for Finance (DIFF) 协议
目前市面上的接口通常以事件回调的方式进行信息交互，导致业务层对当前状况的全景缺乏了解，不便于编写复杂业务逻辑。而 DIFF 协议将异步的事件回调转为同步的数据访问，使得业务层能简单同步的访问业务数据，简化了编码复杂度。

DIFF 协议分为两部分: 数据访问 和 数据传输

数据访问
DIFF 协议要求服务端维护一个业务信息截面，例如：

{
  "account_id": "41007684", # 账号
  "static_balance": 9954306.319000003, # 静态权益
  "balance": 9963216.550000003, # 账户资金
  "available": 9480176.150000002, # 可用资金
  "float_profit": 8910.231, # 浮动盈亏
  "risk_ratio": 0.048482375, # 风险度
  "using": 11232.23, # 占用资金
  "position_volume": 12, # 持仓总手数
  "ins_list": "SHFE.cu1609,...." # 行情订阅的合约列表
  "quotes":{ # 所有订阅的实时行情
    "SHFE.cu1612": {
      "instrument_id": "SHFE.cu1612",
      "datetime": "2016-12-30 13:21:32.500000",
      "ask_priceN": 36590.0, #卖N价
      "ask_volumeN": 121, #卖N量
      "bid_priceN": 36580.0, #买N价
      "bid_volumeN": 3, #买N量
      "last_price": 36580.0, # 最新价
      "highest": 36580.0, # 最高价
      "lowest": 36580.0, # 最低价
      "amount": 213445312.5, # 成交额
      "volume": 23344, # 成交量
      "open_interest": 23344, # 持仓量
      "pre_open_interest": 23344, # 昨持
      "pre_close": 36170.0, # 昨收
      "open": 36270.0, # 今开
      "close" : "-", # 收盘
      "lower_limit": 34160.0, #跌停
      "upper_limit": 38530.0, #涨停
      "average": 36270.1 #均价
      "pre_settlement": 36270.0, # 昨结
      "settlement": "-", # 结算价
    },
    ...
  }
}
对应的客户端也维护了一个该截面的镜像，因此业务层可以简单同步的访问到全部业务数据。

业务截面的内容由各业务模块定义，例如 ref:quote ref:trade ref:mdhis
除非由业务模块的文档另行说明，否则业务截面中的数据应是自恰的。例如：任何时刻的业务截面都应包含 balance, static_balance 和 float_profit 字段，并且满足 balance = static_balance + float_profit
业务截面可能会包含协议中未写出的额外字段，使用方应忽略这些字段的信息，因此在扩展或新增业务模块时可以保持向后兼容性
部分字段可能会有多种数据类型，例如上述例子中的收盘和结算价在收盘前是字符串，在收盘后会更新为对应的数值
数据传输
DIFF 协议使用 json 编码通过 websocket 传输，因此可以使用 ssl 实现传输层安全加密，permessage-deflate 实现数据压缩。协议通讯为全双工模式，任何一方都可以随时向对方发送数据包，也应随时准备接收对方发来的数据包，发出的包之间无需等待对方回应。每个数据包中均有一个 aid 字段，此字段值即为数据包类型。

数据包类型
业务信息截面更新:
DIFF 协议要求服务端将业务信息的变化以 JSON Merge Patch (https://tools.ietf.org/html/rfc7386) 的格式推送给客户端，例如：

{
  "aid": "rtn_data", # 业务信息截面更新
  "data": [ # 数据更新数组
    {
      "balance": 10237421.1, # 账户资金
    },
    {
      "float_profit": 283114.780999997, # 浮动盈亏
    },
    {
      "quotes":{
        "SHFE.cu1612": {
          "datetime": "2016-12-30 14:31:02.000000",
          "last_price": 36605.0, # 最新价
          "volume": 25431, # 成交量
          "pre_close": 36170.0, # 昨收
        }
      }
    }
  ]
}
以上数据包中的 "aid": "rtn_data" 表示该包的类型为业务信息截面更新包
整个 data 数组相当于一个事务，其中的每一个元素都是一个 JSON Merge Patch，处理完整个数组后业务截面即完成了从上一个时间截面推进到下一个时间截面。
处理过程中业务截面可能处于内部不一致的状态，例如上述例子中的 balance 更新后，float_profit 更新前，并不满足 balance = static_balance + float_profit，因此除非有特殊需求，否则业务层应等整个 data 数组都处理完成后再从业务截面中提取所需的数据
没有变化的字段服务端也可能发送,例如上述例子中的 pre_close。
服务端可以自行决定 data 数组的元素个数及每个元素中包含哪些更新，例如客户端不能假定更新行情最新价和成交量一定是在一个 JSON Merge Patch 中。
如果在处理完一个 JSON Merge Patch 后，某个 object 下的所有字段都被删除则也应将该 object 删除
业务信息截面更新请求:
DIFF 协议要求客户端发送 peek_message 数据包以获得业务信息截面更新

{
  "aid": "peek_message"
}
服务端在收到 peek_message 数据包后应检查是否有数据更新，如果有则应将更新内容立即发送给客户端，如果没有则应等到有更新发生时再回应客户端。
服务端发送 rtn_data 数据包后可以等收到下一个 peek_message 后再发送下一个 rtn_data 数据包。
一个简单的客户端实现可以在连接成功后及每收到一个 rtn_data 数据包后发送一个 peek_message 数据包，这样当客户端带宽不足时会自动降低业务信息截面的更新频率以适应低带宽
指令包:
当数据包中的 aid 字段不是 rtn_data 或 peek_message 则表示该包为一个指令包，具体指令由各业务模块定义，例如 subscribe_quote 表示订阅行情，insert_order 表示下单

由于客户端和服务端存在网络通讯延迟，客户端的指令需要过一段时间才会影响到截面中的业务数据，为了使客户端能分辨出服务端是否处理了该指令，通常服务端会将客户端的请求以某种方式体现在截面中（具体方式由各业务模块定义）。例如 subscribe_quote 订阅行情时服务端会将业务截面中的 ins_list 字段更新为客户端订阅的合约列表，这样当客户端检查业务截面时如果 ins_list 包含了客户端订阅的某个合约，但是 quotes 没有该合约则说明该合约不存在



行情报价数据
请求订阅行情报价
终端通过发送 subscribe_quote 包实现订阅行情报价

{
  "aid": "subscribe_quote",       //必填, 请求订阅实时报价数据
  "ins_list": "SHFE.cu1612,CFFEX.IF1701",    //必填, 需要订阅的合约列表，以逗号分隔
}
需要注意几点:

合约代码必须带交易所代码, 例如cu1801应该写作 SHFE.cu1801. 目前支持的交易所为 CFFEX, SHFE, DCE, CZCE, INE
用户自定义的组合, 交易所代码都为 USER
合约代码及交易所代码都是大小写敏感的
每次发送 subscribe_quote 时，应在 ins_list 中列出所有需要订阅的合约代码。多次发送 subscribe_quote，后一次的订阅列表会覆盖前一次的
行情报价数据同步
行情报价数据通过 rtn_data 包的 quotes 字段进行差分发送, 如下所示:

{
  "aid": "rtn_data",                                        //数据推送
  "data": [                                                 //diff数据数组, 一次推送中可能含有多个数据包
    {
      "quotes": {                                           //实时行情数据
        "SHFE.cu1612": {
          "instrument_id": "SHFE.cu1612",                   //合约代码
          "volume_multiple": 300,                           //合约乘数
          "price_tick": 0.2,                                //合约价格单位
          "price_decs": 1,                                  //合约价格小数位数
          "max_market_order_volume": 1000,                  //市价单最大下单手数
          "min_market_order_volume": 1,                     //市价单最小下单手数
          "max_limit_order_volume": 1000,                   //限价单最大下单手数
          "min_limit_order_volume": 1,                      //限价单最小下单手数
          "margin": 4480.0,                                 //每手保证金
          "commission": 2.5,                                //每手手续费
          "datetime": "2016-12-30 13:21:32.500000",         //时间
          "ask_price1": 36590.0,                            //卖价
          "ask_volume1": 121,                               //卖量
          "bid_price1": 36580.0,                            //买价
          "bid_volume1": 3,                                 //买量
          "last_price": 36580.0,                            //最新价
          "highest": 36580.0,                               //最高价
          "lowest": 36580.0,                                //最低价
          "amount": 213445312.5,                            //成交额
          "volume": 23344,                                  //成交量
          "open_interest": 23344,                           //持仓量
          "pre_open_interest": 23344,                       //昨持
          "pre_close": 36170.0,                             //昨收
          "open": 36270.0,                                  //今开
          "close": 36270.0,                                 //收盘
          "lower_limit": 34160.0,                           //跌停
          "upper_limit": 38530.0,                           //涨停
          "average": 36270.1,                               //均价
          "pre_settlement": 36270.0,                        //昨结
          "settlement": 36270.0,                            //结算价
        },
      },
    ]
  }
}


交易及银期转账
交易账户结构
用户(USER)
一个用户由一个唯一的 USER_ID 标识. 每个用户的账户信息互相独立.

在任一时刻, 一个用户的交易账户可以由以下信息完整描述

1-N个资金账户(ACCOUNT)
0-N个持仓记录(POSITION)
0-N个委托单(ORDER)
这些信息完整的描述了用户交易账户的[当前状态]. 需要注意的是, 用户过往的交易记录, 转账记录等并不在其中, 那些信息对于用户的交易动作没有任何影响.

资金账户(ACCOUNT)
每个资金账户由一个 ACCOUNT_ID 标识. 一个USER可以同时拥有多个ACCOUNT. 每个 ACCOUNT 中的各字段都使用同一币种.

下面是一个ACCOUNT的内容示例:

"CNY": {
  //账号及币种
  "user_id": "423423",                      //用户ID
  "currency": "CNY",                        //币种

  //本交易日开盘前状态
  "pre_balance": 12345,                     //上一交易日结算时的账户权益

  //本交易日内出入金事件的影响
  "deposit": 42344,                         //本交易日内的入金金额
  "withdraw": 42344,                        //本交易日内的出金金额
  "static_balance": 124895,                 //静态权益 = pre_balance + deposit - withdraw

  //本交易日内已完成交易的影响
  "close_profit": 12345,                    //本交易日内的平仓盈亏
  "commission": 123,                        //本交易日内交纳的手续费
  "premium": 123,                           //本交易日内交纳的期权权利金

  //当前持仓盈亏
  "position_profit": 12345,                 //当前持仓盈亏
  "float_profit": 8910.2,                   //当前浮动盈亏

  //当前权益
  "balance": 9963216.55,                    //账户权益 = static_balance + close_profit - commission - premium + position_profit

  //保证金占用, 冻结及风险度
  "margin": 11232.23,                       //持仓占用保证金
  "frozen_margin": 12345,                   //挂单冻结保证金
  "frozen_commission": 123,                 //挂单冻结手续费
  "frozen_premium": 123,                    //挂单冻结权利金
  "available": 9480176.150000002,           //可用资金 = balance - margin - frozen_margin - frozen_commission - frozen_premium
  "risk_ratio": 0.048482375,                //风险度 = 1 - available / balance
}
持仓(POSITION)
每个持仓项描述一个合约的当前持仓情况. 通常以相应的合约代码(SYMBOL)作为KEY

下面是一个 POSITION 的内容示例:

"SHFE.cu1801":{                             //position_key=symbol
  //交易所和合约代码
  "user_id": "423423",                      //用户ID
  "exchange_id": "SHFE",                    //交易所
  "instrument_id": "cu1801",                //合约在交易所内的代码

  //持仓手数与冻结手数
  "volume_long_today": 5,                   //多头今仓持仓手数
  "volume_long_his": 5,                     //多头老仓持仓手数
  "volume_long": 10,                        //多头持仓手数
  "volume_long_frozen_today": 1,            //多头今仓冻结手数
  "volume_long_frozen_his": 2,              //多头老仓冻结手数
  "volume_short_today": 5,                  //空头今仓持仓手数
  "volume_short_his": 5,                    //空头老仓持仓手数
  "volume_short": 10,                       //空头持仓手数
  "volume_short_frozen_today": 1,           //空头今仓冻结手数
  "volume_short_frozen_his": 2,             //空头老仓冻结手数

  //成本, 现价与盈亏
  "open_price_long": 3203.5,                //多头开仓均价
  "open_price_short": 3100.5,               //空头开仓均价
  "open_cost_long": 3203.5,                 //多头开仓成本
  "open_cost_short": 3100.5,                //空头开仓成本
  "position_price_long": 32324.4,           //多头持仓均价
  "position_price_short": 32324.4,          //空头持仓均价
  "position_cost_long": 32324.4,            //多头持仓成本
  "position_cost_short": 32324.4,           //空头持仓成本
  "last_price": 12345.6,                    //最新价
  "float_profit_long": 32324.4,             //多头浮动盈亏
  "float_profit_short": 32324.4,            //空头浮动盈亏
  "float_profit": 12345.6,                  //浮动盈亏 = float_profit_long + float_profit_short
  "position_profit_long": 32324.4,          //多头持仓盈亏
  "position_profit_short": 32324.4,         //空头持仓盈亏
  "position_profit": 12345.6,               //持仓盈亏 = position_profit_long + position_profit_short

  //保证金占用
  "margin_long": 32324.4,                   //多头持仓占用保证金
  "margin_short": 32324.4,                  //空头持仓占用保证金
  "margin": 32123.5,                        //持仓占用保证金 = margin_long + margin_short
}
委托单(ORDER)
委托单的单号:

每个委托单都必须有一个单号, 单号可以是不超过128个字节长的任意中英文字符和数字组合.
单号由发出下单指令的终端负责设定. 它必须保证, 对于同一个USER, 每个单号都是不重复的.
委托单状态:

任何一个委托单的状态只会是这两种之一: FINISHED 或 ALIVE
FINISHED: 已经可以确定, 这个委托单以后不会再产生任何新的成交
ALIVE: 除上一种情况外的其它任何情况, 委托单状态都标记为 ALIVE, 即这个委托单还有可能产生新的成交
下面是一个 ORDER 的内容示例:

"123": {                                    //order_id, 用于唯一标识一个委托单. 对于一个USER, order_id 是永远不重复的

  //委托单初始属性(由下单者在下单前确定, 不再改变)
  "user_id": "423423",                      //用户ID
  "order_id": "123",                        //委托单ID, 对于一个USER, order_id 是永远不重复的
  "exchange_id": "SHFE",                    //交易所
  "instrument_id": "cu1801",                //在交易所中的合约代码
  "direction": "BUY",                       //下单方向
  "offset": "OPEN",                         //开平标志
  "volume_orign": 6,                        //总报单手数
  "price_type": "LIMIT",                    //指令类型
  "limit_price": 45000,                     //委托价格, 仅当 price_type = LIMIT 时有效
  "time_condition":   "GTD",                  //时间条件
  "volume_condition": "ANY",                //数量条件

  //下单后获得的信息(由期货公司返回, 不会改变)
  "insert_date_time": 1517544321432,        //下单时间, epoch nano
  "exchange_order_id": "434214",            //交易所单号

  //委托单当前状态
  "status": "ALIVE",                        //委托单状态, ALIVE=有效, FINISHED=已完
  "volume_left": 3,                         //未成交手数
  "frozen_margin": 343234,                  //冻结保证金
  "last_msg": "",                           //提示信息

  //内部序号
  "seqno": 4324,
}
成交记录(TRADE)
下面是一个 TRADE 的内容示例:

"123": {                                    //trade_key, 用于唯一标识一条成交记录. 对于一个USER, trade_key 是永远不重复的

  "user_id": "423423",                      //用户ID
  "order_id": "434214",                     //交易所单号
  "trade_id": "123",                        //委托单ID, 对于一个USER, trade_id 是永远不重复的
  "exchange_id": "SHFE",                    //交易所
  "instrument_id": "cu1801",                //在交易所中的合约代码
  "exchange_trade_id": "434214",            //交易所单号
  "direction": "BUY",                       //下单方向
  "offset": "OPEN",                         //开平标志
  "volume": 6,                              //成交手数
  "price": 45000,                           //成交价格
  "trade_date_time":  15175442131,          //成交时间, epoch nano
  "commission": "434214",                   //成交手续费
  "seqno": 4324,
}
交易账户信息同步
交易账户信息通过 rtn_data 包的 trade 字段进行差分发送, 如下所示:

{
  "aid": "rtn_data",                                      //数据推送
  "data": [                                               //diff数据数组, 一次推送中可能含有多个数据包
  {
    "trade": {                                            //交易相关数据
      "user1": {                                          //登录用户名
        "user_id": "user1",                               //登录用户名
        "accounts": {                                     //账户资金信息
          "CNY": {                                        //account_key, 通常为币种代码
            //核心字段
            "account_id": "423423",                       //账号
            "currency": "CNY",                            //币种
            "balance": 9963216.550000003,                 //账户权益
            "available": 9480176.150000002,               //可用资金
            //参考字段
            "pre_balance": 12345,                         //上一交易日结算时的账户权益
            "deposit": 42344,                             //本交易日内的入金金额
            "withdraw": 42344,                            //本交易日内的出金金额
            "commission": 123,                            //本交易日内交纳的手续费
            "preminum": 123,                              //本交易日内交纳的权利金
            "static_balance": 124895,                     //静态权益
            "position_profit": 12345,                     //持仓盈亏
            "float_profit": 8910.231,                     //浮动盈亏
            "risk_ratio": 0.048482375,                    //风险度
            "margin": 11232.23,                           //占用资金
            "frozen_margin": 12345,                       //冻结保证金
            "frozen_commission": 123,                     //冻结手续费
            "frozen_premium": 123,                        //冻结权利金
            "close_profit": 12345,                        //本交易日内平仓盈亏
            "position_profit": 12345,                     //当前持仓盈亏
          }
        },
        "positions": {                                    //持仓
          "SHFE.cu1801": {                                //合约代码
            //核心字段
            "exchange_id": "SHFE",                        //交易所
            "instrument_id": "cu1801",                    //合约代码
            //参考字段
            "hedge_flag": "SPEC",                         //套保标记
            "open_price_long": 3203.5,                    //多头开仓均价
            "open_price_short": 3100.5,                   //空头开仓均价
            "open_cost_long": 3203.5,                     //多头开仓成本
            "open_cost_short": 3100.5,                    //空头开仓成本
            "float_profit_long": 32324.4,                 //多头浮动盈亏
            "float_profit_short": 32324.4,                //空头浮动盈亏
            "position_cost_long": 32324.4,                //多头持仓成本
            "position_cost_short": 32324.4,               //空头持仓成本
            "position_profit_long": 32324.4,              //多头浮动盈亏
            "position_profit_long": 32324.4,              //空头浮动盈亏
            "volume_long_today": 5,                       //多头今仓持仓手数
            "volume_long_his": 5,                         //多头老仓持仓手数
            "volume_short_today": 5,                      //空头今仓持仓手数
            "volume_short_his": 5,                        //空头老仓持仓手数
            "margin_long": 32324.4,                       //多头持仓占用保证金
            "margin_short": 32324.4,                      //空头持仓占用保证金
            "order_volume_buy_open": 1,                   //买开仓挂单手数
            "order_volume_buy_close": 1,                  //买平仓挂单手数
            "order_volume_sell_open": 1,                  //卖开仓挂单手数
            "order_volume_sell_close": 1,                 //卖平仓挂单手数
          }
        },
        "orders": {                                       //委托单
          "123": {                                        //order_id, 用于唯一标识一个委托单. 对于一个USER, order_id 是永远不重复的
            //核心字段
            "order_id": "123",                            //委托单ID, 对于一个USER, order_id 是永远不重复的
            "order_type": "TRADE",                        //指令类型
            "exchange_id": "SHFE",                        //交易所
            "instrument_id": "cu1801",                    //在交易所中的合约代码
            "direction": "BUY",                           //下单方向, BUY=
            "offset": "OPEN",                             //开平标志
            "volume_orign": 6,                            //总报单手数
            "volume_left": 3,                             //未成交手数
            "trade_type": "TAKEPROFIT",                   //指令类型
            "price_type": "LIMIT",                        //指令类型
            "limit_price": 45000,                         //委托价格, 仅当 price_type = LIMIT 时有效
            "time_condition": "GTD",                      //时间条件
            "volume_condition": "ANY",                    //数量条件
            "min_volume": 0,
            "hedge_flag": "SPECULATION",                  //保值标志
            "status": "ALIVE",                            //委托单状态, ALIVE=有效, FINISHED=已完
            //参考字段
            "last_msg":       "",                               //最后操作信息
            "insert_date_time":       1928374000000000,         //下单时间
            "exchange_order_id": "434214",                //交易所单号
          }
        },
        "trades": {                                       //成交记录
          "123|1": {                                      //trade_key, 用于唯一标识一个成交项
            "order_id": "123",
            "exchange_id": "SHFE",                        //交易所
            "instrument_id": "cu1801",                    //交易所内的合约代码
            "exchange_trade_id": "1243",                  //交易所成交号
            "direction": "BUY",                           //成交方向
            "offset": "OPEN",                             //开平标志
            "volume": 6,                                  //成交手数
            "price": 1234.5,                              //成交价格
            "trade_date_time": 1928374000000000           //成交时间
          }
        },
      },
    },
    ]
  }
}
终端登录鉴权
我们使用 aid = “req_login” 的包作为登录请求包. 此包的结构由具体的实现定义. 以 Open Trade Gateway 项目为例, req_login 包结构如下:

{
  "aid": "req_login",
  "bid": "aaa",
  "user_name": "43214",
  "password": "abcd123",
}
登录成功或失败的信息, 通过 notify 发送

交易指令
下单
终端通过发送 insert_order 包实现下单

{
  "aid": "insert_order",                    //必填, 下单请求
  "user_id": "user1",                       //必填, 需要与登录用户名一致, 或为登录用户的子账户(例如登录用户为user1, 则报单 user_id 应当为 user1 或 user1.some_unit)
  "order_id": "SomeStrategy.Instance1.001", //必填, 委托单号, 需确保在一个账号中不重复, 限长512字节
  "exchange_id": "SHFE",                    //必填, 下单到哪个交易所
  "instrument_id": "cu1803",                //必填, 下单合约代码
  "direction": "BUY",                       //必填, 下单买卖方向
  "offset": "OPEN",                         //必填, 下单开平方向, 仅当指令相关对象不支持开平机制(例如股票)时可不填写此字段
  "volume": 1,                              //必填, 下单手数
  "price_type": "LIMIT",                    //必填, 报单价格类型
  "limit_price": 30502,                     //当 price_type == LIMIT 时需要填写此字段, 报单价格
  "volume_condition": "ANY",
  "time_condition": "GFD",
}
撤单
终端通过发送 cancel_order 包实现撤单

{
  "aid": "cancel_order",                    //必填, 撤单请求
  "user_id": "abcd"                         //必填, 下单时的 user_id
  "order_id": "0001",                       //必填, 委托单的 order_id
}
银期转账
签约银行和转账记录
签约银行和转账记录信息由 rtn_data 包中 trade 部分的 banks 和 transfers 发送, 如下所示

{
  "aid": "rtn_data",                                        //数据推送
  "data": [                                                 //diff数据数组, 一次推送中可能含有多个数据包
    {
      "trade": {                                            //交易相关数据
        "user1": {                                          //登录用户名
          "banks": {                                        //用户相关银行
            "4324": {
              "id": "4324",
              "name": "工行",
            }
          },
          "transfers": {                                    //账户转账记录
            "0001": {
              "datetime": 433241234123                      //转账时间, epoch nano
              "currency": "CNY",                            //币种
              "amount": 3243,                               //涉及金额
              "error_id": 0,                                //转账结果代码
              "error_msg": "成功",                          //转账结果代码
            }
          },
        },
      },
    ]
  }
}
请求银期转账
{
  "aid": "req_transfer",                                    //必填, 转账请求
  "future_account": "0001",                                 //必填, 期货账户
  "future_password": "0001",                                //必填, 期货账户密码
  "bank_id": "0001",                                        //必填, 银行ID
  "bank_password": "0001",                                  //必填, 银行账户密码
  "currency": "CNY",                                        //必填, 币种代码
  "amount": 135.4                                           //必填, 转账金额, >0 表示转入期货账户, <0 表示转出期货账户
}
转账操作的结果, 将由转账记录同步的方式提供给终端

字段常量表
order_type
Name	Value/Description
TRADE	交易指令
SWAP	互换交易指令
EXECUTE	期权行权指令
QUOTE	期权询价指令
trade_type
Name	Value/Description
STOPLOSS	止损
TAKEPROFIT	止盈
price_type
Name	Value/Description
ANY	任意价
LIMIT	限价
BEST	最优价
FIVELEVEL	五档价
volume_condition
Name	Value/Description
ANY	任何数量
MIN	最小数量
ALL	全部数量
time_condition
Name	Value/Description
IOC	立即完成，否则撤销
GFS	本节有效
GFD	当日有效
GTD	指定日期前有效
GTC	撤销前有效
GFA	集合竞价有效
force_close
Name	Value/Description
NOT	非强平
LACK_DEPOSIT	资金不足
CLIENT_POSITION_LIMIT	客户超仓
MEMBER_POSITION_LIMIT	会员超仓
POSITION_MULTIPLE	持仓非整数倍
VIOLATION	违规
OTHER	其他
PERSONAL_DELIV	自然人临近交割
HEDGE_POSITION_LIMIT	客户套保超仓


通知
通知通过 rtn_data 包中的 notify 字段发送, 如下所示

{
  "aid": "rtn_data",                                        //数据推送
  "data": [                                                 //diff数据数组, 一次推送中可能含有多个数据包
    {
      "notify": {                                           //通知信息
        "2010": {
          "type": "MESSAGE",                                //消息类型
          "level": "INFO",                                  //消息等级
          "code": 1000,                                     //消息代码
          "content": "abcd",                                //消息正文
        }
      },
    }
  ]
}
type	说明
MESSAGE	content是一个短字符串，通常只有1行
TEXT	content是一个长文本，通常有多行内容
HTML	content为HTML格式
level	信息等级
INFO	普通消息
WARNING	警告
ERROR	错误


图表数据（K线数据及历史tick数据）
请求订阅图表数据
终端通过发送 set_chart 包实现订阅图表数据

{
  "aid": "set_chart",         // 必填, 请求图表数据
  "chart_id": "abcd123",      // 必填, 图表id, 服务器只会维护每个id收到的最后一个请求的数据
  "ins_list": "SHFE.cu1701",  // 必填, 填空表示删除该图表，多个合约以逗号分割，第一个合约是主合约，所有id都是以主合约为准
  "duration": 180000000000,   // 必填, 周期，单位ns, tick:0, 日线: 3600 * 24 * 1000 * 1000 * 1000
  "view_width": 500,          // 必填, 图表宽度, 请求最新N个数据，并保持滚动(新K线生成会移动图表)
}
需要注意几点:

chart_id 为一个任意字符串，当多次发送的set_chart包中的chart_id重复时，后一次的请求将覆盖前一次。chart_id不相同则视为不同的订阅
历史数据服务是订阅式而非查询式的。只要发送过一次 set_chart 请求，每当行情变化时，都会通过 rtn_data 包推送新的K线，不需要多次发送 set_chart 包
图表数据同步
图表数据通过 rtn_data 包的 klines 字段和 ticks 字段进行差分发送, 如下所示:

{
  "aid": "rtn_data",                                        //数据推送
  "data": [                                                 //diff数据数组, 一次推送中可能含有多个数据包
    {
      "klines": {                                           //K线数据
        "SHFE.cu1601": {                                    //合约代码
          180000000000: {                                   //K线周期, 单位为纳秒, 180000000000纳秒 = 3分钟
            "last_id": 3435,                                //整个序列最后一个记录的序号
            "data": {
              3384: {
                "datetime": 192837400000000,                //UnixNano 北京时间，如果是日线，则是交易日的 UnixNano
                "open": 3432.33,                            //开
                "high": 3432.33,                            //高
                "low": 3432.33,                             //低
                "close": 3432.33,                           //收
                "volume": 2,                                //成交量
                "open_oi": 1632,                            //起始持仓量
                "close_oi": 1621,                           //结束持仓量
              },
              3385: {
                ...
              },
              ...
            },
            "binding": {
              "SHFE.cu1709": {
                3384: 2900,                                 //本合约K线所对应的另一个合约的K线号
                3385: 2950,
                ...
              }
            }
          },
          ...
        },
        ...
      },
      "ticks": {
        "SHFE.cu1601": {
          "last_id": 3550,                                  //整个序列最后一个元素的编号
          "data": {
            3384: {
              "datetime": 1928374000000000,                 //UnixNano 北京时间
              "last_price": 3432.33,                        //最新价
              "average": 3420.11,                           //当日均价
              "highest": 3452.33,                           //最高价
              "lowest": 3402.33,                            //最低价
              "bid_price1": 3432.2,                         //买一价
              "ask_price1": 3432.4,                         //卖一价
              "bid_volume1": 1,                             //买一量
              "ask_volume1": 2,                             //卖一量
              "volume": 200,                                //成交量
              "amount": 120023,                             //成交额
              "open_interest": 1621,                        //持仓量
            },
            3385: {
              ...
            },
            ...
          }
        },
        ...
      },
    ]
  }
}