# HQChart K线图集成文档

## 概述

本项目已成功集成 HQChart 专业K线图表库到 QAExchange 前端界面中。

## 文件结构

```
web/
├── src/
│   ├── components/
│   │   └── KLineChart.vue           # K线图组件
│   └── views/
│       └── WebSocketTest.vue        # 主界面（已集成K线图）
└── package.json                     # 已包含 hqchart 依赖
```

## 核心组件

### KLineChart.vue

封装了 HQChart 的 K线图功能，提供简洁的 Vue 组件接口。

**Props:**
- `symbol` (String): 合约代码，如 'IF2501'
- `period` (Number): K线周期
  - `0`: 日线
  - `4`: 1分钟
  - `5`: 5分钟
  - `6`: 15分钟
  - `7`: 30分钟
  - `8`: 60分钟
- `right` (Number): 复权方式
  - `0`: 不复权
  - `1`: 前复权
  - `2`: 后复权
- `klineData` (Array): K线数据（可选，用于自定义数据源）

**使用示例:**
```vue
<template>
  <div class="chart-container" style="height: 500px">
    <KLineChart
      ref="klineChart"
      :symbol="selectedSymbol"
      :period="5"
      :kline-data="klineDataList"
    />
  </div>
</template>

<script>
import KLineChart from '@/components/KLineChart.vue'

export default {
  components: {
    KLineChart
  },

  data() {
    return {
      selectedSymbol: 'IF2501',
      klineDataList: []
    }
  }
}
</script>
```

## 功能特性

### ✅ 已实现功能

1. **K线图显示**
   - 主图：K线 + 均线（MA）
   - 副图1：成交量（VOL）
   - 副图2：MACD 指标

2. **交互功能**
   - 鼠标拖拽移动
   - 滚轮缩放
   - 十字光标
   - 右键菜单

3. **动态切换**
   - 切换合约代码
   - 切换K线周期（1分钟/5分钟/15分钟/30分钟/60分钟/日线）

4. **响应式布局**
   - 自动适应容器大小

### 🚧 待实现功能

1. **数据对接**
   - 当前使用模拟数据
   - 需要后端实现 K线数据 API
   - 需要 WebSocket 实时K线推送

2. **指标扩展**
   - 添加更多技术指标（KDJ、BOLL、RSI等）
   - 支持自定义指标

3. **画图工具**
   - 趋势线
   - 水平线
   - 斐波那契回调线等

## 在 WebSocketTest.vue 中的集成

K线图已集成到主交易界面中，位于行情面板下方：

```vue
<!-- K线图面板 -->
<el-card class="panel kline-panel">
  <template #header>
    <div class="panel-header">
      <span>K线图</span>
      <el-select v-model="klinePeriod" size="small">
        <el-option label="1分钟" :value="4" />
        <el-option label="5分钟" :value="5" />
        <el-option label="15分钟" :value="6" />
        <el-option label="30分钟" :value="7" />
        <el-option label="60分钟" :value="8" />
        <el-option label="日线" :value="0" />
      </el-select>
    </div>
  </template>

  <div class="kline-container">
    <KLineChart
      ref="klineChart"
      :symbol="selectedInstrument"
      :period="klinePeriod"
      :kline-data="klineDataList"
    />
  </div>
</el-card>
```

## 后端数据对接方案

### 方案一：HTTP API（推荐）

后端提供 K线数据查询接口：

```
GET /api/market/kline/{instrument_id}?period={period}&count={count}&end_time={end_time}
```

**参数：**
- `instrument_id`: 合约代码（如 IF2501）
- `period`: 周期（0=日线, 4=1分钟, 5=5分钟等）
- `count`: 数据条数（默认500）
- `end_time`: 结束时间（可选）

**响应示例：**
```json
{
  "code": 0,
  "message": "success",
  "data": {
    "symbol": "IF2501",
    "period": 5,
    "klines": [
      {
        "datetime": 1696723200000,
        "open": 3800.5,
        "high": 3820.0,
        "low": 3795.0,
        "close": 3810.0,
        "volume": 12345,
        "amount": 47123456.78
      }
    ]
  }
}
```

### 方案二：WebSocket 推送

通过 DIFF 协议推送实时K线数据：

```json
{
  "aid": "rtn_data",
  "data": [
    {
      "klines": {
        "IF2501": {
          "60": {
            "data": {
              "202510071400": {
                "datetime": 1696723200000,
                "open": 3800.5,
                "high": 3820.0,
                "low": 3795.0,
                "close": 3810.0,
                "volume": 12345,
                "amount": 47123456.78
              }
            }
          }
        }
      }
    }
  ]
}
```

## 前端数据处理

在 `WebSocketTest.vue` 中的 `fetchKLineData()` 方法里：

```javascript
async fetchKLineData() {
  try {
    const response = await this.$axios.get(
      `/api/market/kline/${this.selectedInstrument}`,
      {
        params: {
          period: this.klinePeriod,
          count: 500
        }
      }
    )

    this.klineDataList = response.data.data.klines
  } catch (error) {
    console.error('获取K线数据失败:', error)
    this.$message.error('获取K线数据失败')
  }
}
```

## HQChart 配置说明

### 主要配置项

```javascript
{
  Type: '历史K线图',

  // 窗口指标配置
  Windows: [
    { Index: 'MA', Modify: false, Change: false },    // 主图均线
    { Index: 'VOL', Modify: false, Change: false },   // 成交量
    { Index: 'MACD', Modify: false, Change: false }   // MACD
  ],

  // K线图配置
  KLine: {
    DragMode: 1,              // 拖拽模式
    Right: 0,                 // 复权方式
    Period: 5,                // K线周期
    PageSize: 100,            // 一屏显示K线数
    IsShowTooltip: true       // 显示提示信息
  },

  // 边框间距
  Border: {
    Left: 60,
    Right: 80,
    Top: 25,
    Bottom: 20
  },

  // 子框架高度比例
  Frame: [
    { SplitCount: 5, StringFormat: 0, Height: 13 },  // 主图
    { SplitCount: 3, StringFormat: 0, Height: 4 },   // 副图1
    { SplitCount: 2, StringFormat: 0, Height: 3 }    // 副图2
  ]
}
```

## 常见问题

### Q: K线图不显示？

A: 检查以下几点：
1. 容器是否有高度（必须明确指定高度，如 `height: 500px`）
2. HQChart 依赖是否正确安装（`npm install hqchart`）
3. 浏览器控制台是否有错误信息

### Q: 如何添加更多技术指标？

A: 修改 `KLineChart.vue` 中的 `Windows` 配置：

```javascript
Windows: [
  { Index: 'MA', Modify: false, Change: false },
  { Index: 'VOL', Modify: false, Change: false },
  { Index: 'MACD', Modify: false, Change: false },
  { Index: 'KDJ', Modify: false, Change: false },   // 新增 KDJ
  { Index: 'BOLL', Modify: false, Change: false }   // 新增布林带
]
```

### Q: 如何切换K线样式（美国线/蜡烛图）？

A: 在配置中添加：

```javascript
KLine: {
  DrawType: 0,  // 0=实心K线, 1=空心K线, 2=美国线, 3=收盘价线
  // ... 其他配置
}
```

## 参考资料

- [HQChart GitHub](https://github.com/jones2000/HQChart)
- [HQChart 文档](https://github.com/jones2000/HQChart/tree/master/document)
- [Vue 示例](https://github.com/jones2000/HQChart/tree/master/vue.demo)

## 作者

@yutiansut @quantaxis

## 更新日期

2025-10-07
