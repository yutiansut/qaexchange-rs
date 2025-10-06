<template>
  <div class="chart-container">
    <div class="chart-header">
      <h2>K线图表</h2>
      <div class="chart-controls">
        <el-select v-model="selectedSymbol" placeholder="选择交易对" style="width: 200px;">
          <el-option label="BTC/USDT" value="BTCUSDT"></el-option>
          <el-option label="ETH/USDT" value="ETHUSDT"></el-option>
        </el-select>
        <el-select v-model="selectedInterval" placeholder="时间周期" style="width: 120px;">
          <el-option label="1分钟" value="1m"></el-option>
          <el-option label="5分钟" value="5m"></el-option>
          <el-option label="15分钟" value="15m"></el-option>
          <el-option label="1小时" value="1h"></el-option>
          <el-option label="4小时" value="4h"></el-option>
          <el-option label="1天" value="1d"></el-option>
        </el-select>
      </div>
    </div>
    <div class="chart-content">
      <div id="kline-chart" style="height: 600px;"></div>
    </div>
  </div>
</template>

<script>
export default {
  name: 'Chart',
  data() {
    return {
      selectedSymbol: 'BTCUSDT',
      selectedInterval: '1h'
    }
  },
  mounted() {
    this.initChart()
  },
  methods: {
    initChart() {
      // TODO: 初始化K线图表
      console.log('初始化K线图表')
    }
  }
}
</script>

<style scoped>
.chart-container {
  padding: 20px;
}

.chart-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 20px;
}

.chart-header h2 {
  margin: 0;
  color: #303133;
}

.chart-controls {
  display: flex;
  gap: 10px;
}

.chart-content {
  background: #fff;
  border-radius: 4px;
  box-shadow: 0 2px 4px rgba(0, 0, 0, 0.1);
  padding: 20px;
}
</style>
