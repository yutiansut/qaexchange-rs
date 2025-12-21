<template>
  <div class="settlement-container">
    <!-- 页面标题 -->
    <div class="page-header">
      <h2>结算管理</h2>
    </div>

    <!-- 标签页 -->
    <el-tabs v-model="activeTab" class="tabs-container">
      <!-- 日终结算操作 -->
      <el-tab-pane label="日终结算" name="daily">
        <el-card class="settlement-card">
          <div slot="header">
            <span>日终结算操作</span>
          </div>

          <el-form :model="settlementForm" label-width="120px">
            <el-form-item label="结算日期">
              <el-date-picker
                v-model="settlementForm.settlementDate"
                type="date"
                placeholder="选择结算日期"
                value-format="yyyy-MM-dd"
                :picker-options="datePickerOptions"
              ></el-date-picker>
            </el-form-item>

            <el-form-item label="结算价设置">
              <el-button type="primary" size="small" @click="showSetPriceDialog">
                设置结算价
              </el-button>
              <el-button type="info" size="small" @click="importPrices">
                批量导入
              </el-button>
              <span style="margin-left: 15px; color: #909399;">
                已设置 {{ settlementPrices.length }} 个合约结算价
              </span>
            </el-form-item>

            <el-form-item>
              <el-button type="danger" size="large" @click="executeSettlement" :loading="executing">
                <i class="el-icon-s-check"></i>
                执行日终结算
              </el-button>
              <span style="margin-left: 15px; color: #E6A23C;">
                <i class="el-icon-warning"></i>
                执行后将计算所有账户的盈亏并进行风险检查，请谨慎操作！
              </span>
            </el-form-item>
          </el-form>

          <!-- 结算价列表 -->
          <div v-if="settlementPrices.length > 0" style="margin-top: 20px;">
            <h4>当前结算价</h4>
            <el-table
              :data="settlementPrices"
              border
              stripe
              height="200"
            >
              <el-table-column prop="instrument_id" label="合约代码" width="150"></el-table-column>
              <el-table-column prop="settlement_price" label="结算价" width="120" align="right"></el-table-column>
              <el-table-column prop="last_price" label="最新价" width="120" align="right"></el-table-column>
              <el-table-column prop="change_rate" label="涨跌幅" width="120" align="right">
                <template slot-scope="scope">
                  <span :style="{ color: scope.row.change_rate >= 0 ? '#F56C6C' : '#67C23A' }">
                    {{ scope.row.change_rate >= 0 ? '+' : '' }}{{ (scope.row.change_rate * 100).toFixed(2) }}%
                  </span>
                </template>
              </el-table-column>
              <el-table-column label="操作" width="100">
                <template slot-scope="scope">
                  <el-button size="mini" type="text" @click="removePrice(scope.$index)">删除</el-button>
                </template>
              </el-table-column>
            </el-table>
          </div>
        </el-card>
      </el-tab-pane>

      <!-- 结算历史 -->
      <el-tab-pane label="结算历史" name="history">
        <div class="table-toolbar">
          <el-date-picker
            v-model="historyDateRange"
            type="daterange"
            range-separator="至"
            start-placeholder="开始日期"
            end-placeholder="结束日期"
            value-format="yyyy-MM-dd"
            @change="loadHistory"
          ></el-date-picker>
        </div>

        <el-table
          ref="historyTable"
          :data="historyList"
          border
          stripe
          v-loading="historyLoading"
          height="500"
        >
          <el-table-column prop="settlement_date" label="结算日期" width="120"></el-table-column>
          <el-table-column prop="instrument_count" label="合约数" width="100" align="center"></el-table-column>
          <el-table-column prop="account_count" label="账户数" width="100" align="center"></el-table-column>
          <el-table-column prop="total_profit" label="总盈亏" width="150" align="right">
            <template slot-scope="scope">
              <span :style="{ color: scope.row.total_profit >= 0 ? '#F56C6C' : '#67C23A' }">
                {{ scope.row.total_profit >= 0 ? '+' : '' }}{{ scope.row.total_profit.toLocaleString('zh-CN', { minimumFractionDigits: 2 }) }}
              </span>
            </template>
          </el-table-column>
          <el-table-column prop="total_commission" label="总手续费" width="150" align="right">
            <template slot-scope="scope">
              {{ scope.row.total_commission.toLocaleString('zh-CN', { minimumFractionDigits: 2 }) }}
            </template>
          </el-table-column>
          <el-table-column prop="profit_accounts" label="盈利账户数" width="120" align="center"></el-table-column>
          <el-table-column prop="loss_accounts" label="亏损账户数" width="120" align="center"></el-table-column>
          <el-table-column prop="liquidation_count" label="强平账户数" width="120" align="center"></el-table-column>
          <el-table-column prop="status" label="状态" width="100">
            <template slot-scope="scope">
              <el-tag :type="getStatusTagType(scope.row.status)" size="small">
                {{ getStatusName(scope.row.status) }}
              </el-tag>
            </template>
          </el-table-column>
          <el-table-column prop="execution_time" label="执行时间" width="180"></el-table-column>
          <el-table-column label="操作" width="100">
            <template slot-scope="scope">
              <el-button size="mini" type="text" @click="viewDetail(scope.row)">详情</el-button>
            </template>
          </el-table-column>
        </el-table>
      </el-tab-pane>

      <!-- 结算统计 -->
      <el-tab-pane label="结算统计" name="statistics">
        <el-row :gutter="20" class="stats-row">
          <el-col :span="6">
            <div class="stat-card">
              <div class="stat-icon">
                <i class="el-icon-document-checked"></i>
              </div>
              <div class="stat-content">
                <div class="stat-value">{{ statistics.monthSettlementCount }}</div>
                <div class="stat-label">本月结算次数</div>
              </div>
            </div>
          </el-col>

          <el-col :span="6">
            <div class="stat-card">
              <div class="stat-icon" style="color: #F56C6C">
                <i class="el-icon-user"></i>
              </div>
              <div class="stat-content">
                <div class="stat-value">{{ statistics.profitAccountsCount }}</div>
                <div class="stat-label">盈利账户数</div>
              </div>
            </div>
          </el-col>

          <el-col :span="6">
            <div class="stat-card">
              <div class="stat-icon" style="color: #67C23A">
                <i class="el-icon-user"></i>
              </div>
              <div class="stat-content">
                <div class="stat-value">{{ statistics.lossAccountsCount }}</div>
                <div class="stat-label">亏损账户数</div>
              </div>
            </div>
          </el-col>

          <el-col :span="6">
            <div class="stat-card">
              <div class="stat-icon" style="color: #409EFF">
                <i class="el-icon-coin"></i>
              </div>
              <div class="stat-content">
                <div class="stat-value">{{ statistics.totalCommission.toLocaleString() }}</div>
                <div class="stat-label">总手续费收入</div>
              </div>
            </div>
          </el-col>
        </el-row>
        <el-card class="chart-wrapper" shadow="never">
          <div id="settlement-chart" style="height:360px;"></div>
        </el-card>
      </el-tab-pane>
    </el-tabs>

    <input
      type="file"
      ref="priceFileInput"
      accept=".csv,.txt"
      style="display: none;"
      @change="handlePriceFileUpload"
    />

    <!-- 设置结算价对话框 -->
    <el-dialog
      title="设置结算价"
      :visible.sync="priceDialogVisible"
      width="500px"
    >
      <el-form :model="priceForm" label-width="100px">
        <el-form-item label="合约代码">
          <el-select v-model="priceForm.instrument_id" placeholder="请选择合约" filterable>
            <el-option
              v-for="item in availableInstruments"
              :key="item.instrument_id"
              :label="item.instrument_id"
              :value="item.instrument_id"
            ></el-option>
          </el-select>
        </el-form-item>

        <el-form-item label="结算价">
          <el-input-number
            v-model="priceForm.settlement_price"
            :precision="2"
            :step="0.1"
            controls-position="right"
          ></el-input-number>
        </el-form-item>
      </el-form>

      <div slot="footer">
        <el-button @click="priceDialogVisible = false">取消</el-button>
        <el-button type="primary" @click="addSettlementPrice">确定</el-button>
      </div>
    </el-dialog>

    <!-- 结算详情对话框 -->
    <el-dialog
      title="结算详情"
      :visible.sync="detailDialogVisible"
      width="780px"
    >
      <div v-loading="detailLoading">
        <el-descriptions :column="2" border v-if="detailData">
          <el-descriptions-item label="结算日期">{{ detailData.settlement_date }}</el-descriptions-item>
          <el-descriptions-item label="账户数">{{ detailData.account_count }}</el-descriptions-item>
          <el-descriptions-item label="总盈亏">{{ formatCurrency(detailData.total_profit) }}</el-descriptions-item>
          <el-descriptions-item label="总手续费">{{ formatCurrency(detailData.total_commission) }}</el-descriptions-item>
        </el-descriptions>

        <el-tabs v-model="detailTab" style="margin-top: 16px;">
          <el-tab-pane label="账户明细" name="accounts">
            <el-table
              :data="detailData && detailData.accounts || []"
              height="240"
              border
              size="mini"
            >
              <el-table-column prop="user_id" label="账户" width="160" />
              <el-table-column prop="balance" label="结算后权益" width="140" align="right">
                <template slot-scope="{ row }">{{ formatCurrency(row.balance) }}</template>
              </el-table-column>
              <el-table-column prop="close_profit" label="平仓盈亏" width="120" align="right">
                <template slot-scope="{ row }">{{ formatCurrency(row.close_profit) }}</template>
              </el-table-column>
              <el-table-column prop="position_profit" label="持仓盈亏" width="120" align="right">
                <template slot-scope="{ row }">{{ formatCurrency(row.position_profit) }}</template>
              </el-table-column>
              <el-table-column prop="commission" label="手续费" width="100" align="right">
                <template slot-scope="{ row }">{{ formatCurrency(row.commission) }}</template>
              </el-table-column>
            </el-table>
          </el-tab-pane>
          <el-tab-pane label="结算价" name="prices">
            <el-table
              :data="detailData && detailData.prices || []"
              height="240"
              border
              size="mini"
            >
              <el-table-column prop="instrument_id" label="合约" width="140" />
              <el-table-column prop="settlement_price" label="结算价" width="120" align="right" />
              <el-table-column prop="last_price" label="最新价" width="120" align="right" />
              <el-table-column prop="change_rate" label="涨跌幅" width="120" align="right">
                <template slot-scope="{ row }">
                  {{ ((row.change_rate || 0) * 100).toFixed(2) }}%
                </template>
              </el-table-column>
            </el-table>
          </el-tab-pane>
        </el-tabs>
      </div>
    </el-dialog>
  </div>
</template>

<script>
import dayjs from 'dayjs'
import * as echarts from 'echarts'
import {
  getSettlementHistory,
  setSettlementPrice,
  batchSetSettlementPrices,
  executeSettlement,
  getSettlementDetail
} from '@/api'

export default {
  name: 'Settlement',
  data() {
    return {
      activeTab: 'daily',
      executing: false,
      historyLoading: false,
      settlementForm: {
        settlementDate: dayjs().format('YYYY-MM-DD')
      },
      settlementPrices: [],
      historyList: [],
      historyDateRange: [],
      statistics: {
        monthSettlementCount: 0,
        profitAccountsCount: 0,
        lossAccountsCount: 0,
        totalCommission: 0
      },
      priceDialogVisible: false,
      priceForm: {
        instrument_id: '',
        settlement_price: 0
      },
      availableInstruments: [
        { instrument_id: 'IF2501' },
        { instrument_id: 'IF2502' },
        { instrument_id: 'IH2501' },
        { instrument_id: 'IC2501' }
      ],
      datePickerOptions: {
        disabledDate(time) {
          // 禁止选择未来日期
          return time.getTime() > Date.now()
        }
      },
      chartInstance: null,
      detailDialogVisible: false,
      detailLoading: false,
      detailData: null,
      detailTab: 'accounts'
    }
  },
  mounted() {
    this.loadHistory()
    this.initChart()
  },
  beforeDestroy() {
    if (this._resizeHandler) {
      window.removeEventListener('resize', this._resizeHandler)
    }
    if (this.chartInstance) {
      this.chartInstance.dispose()
    }
  },
  methods: {
    // 加载结算历史
    async loadHistory() {
      this.historyLoading = true
      try {
        const params = {}
        if (this.historyDateRange && this.historyDateRange.length === 2) {
          params.start_date = this.historyDateRange[0]
          params.end_date = this.historyDateRange[1]
        }

        const records = await getSettlementHistory(params)
        this.historyList = records || []
        this.updateStatisticsFromHistory()
        this.updateChart()
      } catch (error) {
        this.$message.error('加载结算历史失败')
        console.error(error)
      } finally {
        this.historyLoading = false
      }
    },

    // 加载统计数据（从历史记录中计算）
    async loadStatistics() {
      this.updateStatisticsFromHistory()
    },

    // 显示设置结算价对话框
    showSetPriceDialog() {
      this.priceForm = {
        instrument_id: '',
        settlement_price: 0
      }
      this.priceDialogVisible = true
    },

    // 添加结算价
    addSettlementPrice() {
      if (!this.priceForm.instrument_id || this.priceForm.settlement_price <= 0) {
        this.$message.warning('请填写完整信息')
        return
      }

      // 检查是否已存在
      const index = this.settlementPrices.findIndex(
        p => p.instrument_id === this.priceForm.instrument_id
      )

      if (index >= 0) {
        // 更新
        this.settlementPrices.splice(index, 1, {
          ...this.priceForm,
          last_price: this.priceForm.settlement_price * (1 + Math.random() * 0.02 - 0.01),
          change_rate: Math.random() * 0.04 - 0.02
        })
      } else {
        // 新增
        this.settlementPrices.push({
          ...this.priceForm,
          last_price: this.priceForm.settlement_price * (1 + Math.random() * 0.02 - 0.01),
          change_rate: Math.random() * 0.04 - 0.02
        })
      }

      this.priceDialogVisible = false
      this.$message.success('添加成功')
    },

    // 删除结算价
    removePrice(index) {
      this.settlementPrices.splice(index, 1)
    },

    // 批量导入
    importPrices() {
      if (this.$refs.priceFileInput) {
        this.$refs.priceFileInput.click()
      }
    },

    handlePriceFileUpload(event) {
      const file = event.target.files && event.target.files[0]
      if (!file) return
      const reader = new FileReader()
      reader.onload = e => {
        const lines = (e.target.result || '').split(/\r?\n/)
        lines.forEach(line => {
          const [instrument, price] = line.split(',').map(item => item && item.trim())
          const value = parseFloat(price)
          if (instrument && !isNaN(value)) {
            const existingIndex = this.settlementPrices.findIndex(p => p.instrument_id === instrument)
            const payload = {
              instrument_id: instrument,
              settlement_price: value,
              last_price: value,
              change_rate: 0
            }
            if (existingIndex >= 0) {
              this.settlementPrices.splice(existingIndex, 1, payload)
            } else {
              this.settlementPrices.push(payload)
            }
          }
        })
        this.$message.success('已导入结算价')
      }
      reader.onerror = () => this.$message.error('读取文件失败')
      reader.readAsText(file)
      event.target.value = ''
    },

    // 执行结算
    async executeSettlement() {
      if (this.settlementPrices.length === 0) {
        this.$message.warning('请先设置结算价')
        return
      }

      try {
        await this.$confirm(
          `确定要执行 ${this.settlementForm.settlementDate} 的日终结算吗？`,
          '结算确认',
          {
            type: 'warning',
            confirmButtonText: '确定执行',
            cancelButtonText: '取消'
          }
        )

        this.executing = true

        // 步骤1：批量设置结算价
        const pricesData = {
          prices: this.settlementPrices.map(p => ({
            instrument_id: p.instrument_id,
            settlement_price: p.settlement_price
          }))
        }

        const priceResponse = await batchSetSettlementPrices(pricesData)
        if (!priceResponse.data || !priceResponse.data.success) {
          const errorMsg = (priceResponse.data && priceResponse.data.error && priceResponse.data.error.message) || '设置结算价失败'
          throw new Error(errorMsg)
        }

        // 步骤2：执行日终结算
        const settlementResponse = await executeSettlement()
        if (settlementResponse.data && settlementResponse.data.success) {
          this.$message.success('结算执行成功')
          this.settlementPrices = []
          this.loadHistory()
          this.loadStatistics()
        } else {
          const errorMsg = (settlementResponse.data && settlementResponse.data.error && settlementResponse.data.error.message) || '结算执行失败'
          throw new Error(errorMsg)
        }
      } catch (error) {
        if (error !== 'cancel') {
          this.$message.error(error.message || '结算执行失败')
          console.error(error)
        }
      } finally {
        this.executing = false
      }
    },

    // 查看详情
    async viewDetail(row) {
      this.detailDialogVisible = true
      this.detailTab = 'accounts'
      this.detailLoading = true
      try {
        const detail = await getSettlementDetail(row.settlement_date)
        this.detailData = detail || {}
      } catch (error) {
        this.$message.error('加载结算详情失败')
        console.error(error)
        this.detailData = null
      } finally {
        this.detailLoading = false
      }
    },

    // 获取状态标签颜色
    getStatusTagType(status) {
      const statusMap = {
        'success': 'success',
        'failed': 'danger',
        'partial': 'warning'
      }
      return statusMap[status] || ''
    },

    // 获取状态名称
    getStatusName(status) {
      const statusMap = {
        'success': '成功',
        'failed': '失败',
        'partial': '部分成功'
      }
      return statusMap[status] || status
    },

    initChart() {
      this.$nextTick(() => {
        const dom = document.getElementById('settlement-chart')
        if (dom) {
          this.chartInstance = echarts.init(dom)
          this.updateChart()
          this._resizeHandler = () => {
            this.chartInstance && this.chartInstance.resize()
          }
          window.addEventListener('resize', this._resizeHandler)
        }
      })
    },

    updateChart() {
      if (!this.chartInstance) return
      const dates = this.historyList.map(item => item.settlement_date)
      const profits = this.historyList.map(item => item.total_profit || 0)
      const option = {
        tooltip: { trigger: 'axis' },
        grid: { left: '3%', right: '4%', bottom: '3%', containLabel: true },
        xAxis: { type: 'category', data: dates },
        yAxis: { type: 'value' },
        series: [
          {
            name: '总盈亏',
            type: 'line',
            smooth: true,
            data: profits,
            areaStyle: { opacity: 0.1 }
          }
        ]
      }
      this.chartInstance.setOption(option)
    },

    updateStatisticsFromHistory() {
      if (!this.historyList.length) {
        this.statistics = {
          monthSettlementCount: 0,
          profitAccountsCount: 0,
          lossAccountsCount: 0,
          totalCommission: 0
        }
        return
      }
      this.statistics = {
        monthSettlementCount: this.historyList.length,
        profitAccountsCount: this.historyList.reduce((sum, item) => sum + (item.profit_accounts || 0), 0),
        lossAccountsCount: this.historyList.reduce((sum, item) => sum + (item.loss_accounts || 0), 0),
        totalCommission: this.historyList.reduce((sum, item) => sum + (item.total_commission || 0), 0)
      }
    },

    formatCurrency(value) {
      return Number(value || 0).toLocaleString('zh-CN', { minimumFractionDigits: 2 })
    }
  }
}
</script>

<style lang="scss" scoped>
// @yutiansut @quantaxis - 深色主题样式
$dark-bg-primary: #0d1117;
$dark-bg-secondary: #161b22;
$dark-bg-card: #1c2128;
$dark-bg-tertiary: #21262d;
$dark-border: #30363d;
$dark-text-primary: #f0f6fc;
$dark-text-secondary: #8b949e;
$dark-text-muted: #6e7681;
$primary-color: #1890ff;
$danger-color: #f5222d;
$warning-color: #faad14;
$success-color: #52c41a;

.settlement-container {
  padding: 20px;
  background: $dark-bg-primary;
  min-height: calc(100vh - 60px);
}

.page-header {
  margin-bottom: 20px;
}

.page-header h2 {
  margin: 0;
  color: $dark-text-primary !important;
}

.tabs-container {
  background: $dark-bg-card !important;
  border: 1px solid $dark-border;
  border-radius: 8px;
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.3);
  padding: 20px;

  // 标签页样式
  ::v-deep .el-tabs__item {
    color: $dark-text-secondary !important;

    &.is-active {
      color: $primary-color !important;
    }

    &:hover {
      color: $primary-color !important;
    }
  }

  ::v-deep .el-tabs__nav-wrap::after {
    background-color: $dark-border !important;
  }

  // 表格样式
  ::v-deep .el-table {
    background: transparent !important;
    color: $dark-text-primary !important;

    &::before {
      background-color: $dark-border !important;
    }

    th.el-table__cell {
      background: $dark-bg-secondary !important;
      color: $dark-text-secondary !important;
      border-bottom: 1px solid $dark-border !important;
      font-weight: 600;
    }

    tr {
      background: $dark-bg-card !important;
    }

    td.el-table__cell {
      background: $dark-bg-card !important;
      color: $dark-text-primary !important;
      border-bottom: 1px solid $dark-border !important;
    }

    .el-table__row:hover > td.el-table__cell {
      background: $dark-bg-tertiary !important;
    }

    .el-table__row--striped .el-table__cell {
      background: rgba($dark-bg-tertiary, 0.5) !important;
    }
  }

  ::v-deep .el-button--text {
    color: $primary-color !important;

    &:hover {
      color: lighten($primary-color, 15%) !important;
    }
  }
}

.settlement-card {
  margin-bottom: 20px;

  ::v-deep .el-card {
    background: $dark-bg-card !important;
    border: 1px solid $dark-border !important;

    .el-card__header {
      background: $dark-bg-secondary !important;
      border-bottom: 1px solid $dark-border !important;
      color: $dark-text-primary !important;
    }

    .el-card__body {
      background: $dark-bg-card !important;
    }
  }
}

.chart-wrapper {
  ::v-deep .el-card {
    background: $dark-bg-card !important;
    border: 1px solid $dark-border !important;

    .el-card__body {
      background: $dark-bg-card !important;
    }
  }
}

.table-toolbar {
  margin-bottom: 15px;

  ::v-deep .el-date-editor {
    .el-input__inner {
      background: $dark-bg-tertiary !important;
      border-color: $dark-border !important;
      color: $dark-text-primary !important;
    }
  }
}

.stats-row {
  margin-bottom: 20px;
}

.stat-card {
  display: flex;
  align-items: center;
  padding: 20px;
  background: $dark-bg-card !important;
  border: 1px solid $dark-border;
  border-radius: 8px;
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.3);
}

.stat-icon {
  font-size: 40px;
  margin-right: 15px;
  color: $primary-color;
}

.stat-value {
  font-size: 28px;
  font-weight: bold;
  color: $dark-text-primary !important;
  font-family: 'JetBrains Mono', monospace;
}

.stat-label {
  font-size: 14px;
  color: $dark-text-secondary !important;
  margin-top: 5px;
}

// 表单样式
::v-deep .el-form {
  .el-form-item__label {
    color: $dark-text-secondary !important;
  }

  .el-input__inner {
    background: $dark-bg-tertiary !important;
    border-color: $dark-border !important;
    color: $dark-text-primary !important;

    &::placeholder {
      color: $dark-text-muted !important;
    }
  }

  .el-input-number {
    .el-input-number__decrease,
    .el-input-number__increase {
      background: $dark-bg-tertiary !important;
      border-color: $dark-border !important;
      color: $dark-text-secondary !important;

      &:hover {
        color: $primary-color !important;
      }
    }
  }
}

// 标签样式
::v-deep .el-tag {
  border: none !important;

  &.el-tag--success {
    background: rgba($success-color, 0.15) !important;
    color: $success-color !important;
  }

  &.el-tag--warning {
    background: rgba($warning-color, 0.15) !important;
    color: $warning-color !important;
  }

  &.el-tag--danger {
    background: rgba($danger-color, 0.15) !important;
    color: $danger-color !important;
  }
}

// 对话框样式
::v-deep .el-dialog {
  background: $dark-bg-card !important;
  border: 1px solid $dark-border !important;
  border-radius: 8px !important;

  .el-dialog__header {
    background: $dark-bg-secondary !important;
    border-bottom: 1px solid $dark-border !important;

    .el-dialog__title {
      color: $dark-text-primary !important;
    }
  }

  .el-dialog__body {
    background: $dark-bg-card !important;
  }

  .el-dialog__footer {
    background: $dark-bg-card !important;
    border-top: 1px solid $dark-border !important;
  }
}

// 描述列表样式
::v-deep .el-descriptions {
  .el-descriptions-item__label {
    background: $dark-bg-secondary !important;
    color: $dark-text-secondary !important;
  }

  .el-descriptions-item__content {
    background: $dark-bg-card !important;
    color: $dark-text-primary !important;
  }

  .el-descriptions__body {
    background: $dark-bg-card !important;
  }

  &.is-bordered .el-descriptions-item__cell {
    border-color: $dark-border !important;
  }
}

// 下拉选择框
::v-deep .el-select-dropdown {
  background: $dark-bg-secondary !important;
  border-color: $dark-border !important;

  .el-select-dropdown__item {
    color: $dark-text-primary !important;

    &:hover, &.hover {
      background: $dark-bg-tertiary !important;
    }

    &.selected {
      color: $primary-color !important;
      font-weight: 600;
    }
  }
}

// 按钮样式
::v-deep .el-button {
  &.el-button--default {
    background: $dark-bg-tertiary !important;
    border-color: $dark-border !important;
    color: $dark-text-primary !important;

    &:hover {
      border-color: $primary-color !important;
      color: $primary-color !important;
    }
  }

  &.el-button--info {
    background: $dark-bg-tertiary !important;
    border-color: $dark-border !important;
    color: $dark-text-primary !important;
  }
}
</style>
