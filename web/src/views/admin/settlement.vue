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
            <vxe-table
              :data="settlementPrices"
              border
              stripe
              height="200"
            >
              <vxe-table-column field="instrument_id" title="合约代码" width="150"></vxe-table-column>
              <vxe-table-column field="settlement_price" title="结算价" width="120" align="right"></vxe-table-column>
              <vxe-table-column field="last_price" title="最新价" width="120" align="right"></vxe-table-column>
              <vxe-table-column field="change_rate" title="涨跌幅" width="120" align="right">
                <template slot-scope="{ row }">
                  <span :style="{ color: row.change_rate >= 0 ? '#F56C6C' : '#67C23A' }">
                    {{ row.change_rate >= 0 ? '+' : '' }}{{ (row.change_rate * 100).toFixed(2) }}%
                  </span>
                </template>
              </vxe-table-column>
              <vxe-table-column title="操作" width="100">
                <template slot-scope="{ row, rowIndex }">
                  <el-button size="mini" type="text" @click="removePrice(rowIndex)">删除</el-button>
                </template>
              </vxe-table-column>
            </vxe-table>
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

        <vxe-table
          ref="historyTable"
          :data="historyList"
          border
          stripe
          resizable
          highlight-hover-row
          :loading="historyLoading"
          height="500"
        >
          <vxe-table-column field="settlement_date" title="结算日期" width="120"></vxe-table-column>
          <vxe-table-column field="instrument_count" title="合约数" width="100" align="center"></vxe-table-column>
          <vxe-table-column field="account_count" title="账户数" width="100" align="center"></vxe-table-column>
          <vxe-table-column field="total_profit" title="总盈亏" width="150" align="right">
            <template slot-scope="{ row }">
              <span :style="{ color: row.total_profit >= 0 ? '#F56C6C' : '#67C23A' }">
                {{ row.total_profit >= 0 ? '+' : '' }}{{ row.total_profit.toLocaleString('zh-CN', { minimumFractionDigits: 2 }) }}
              </span>
            </template>
          </vxe-table-column>
          <vxe-table-column field="total_commission" title="总手续费" width="150" align="right">
            <template slot-scope="{ row }">
              {{ row.total_commission.toLocaleString('zh-CN', { minimumFractionDigits: 2 }) }}
            </template>
          </vxe-table-column>
          <vxe-table-column field="profit_accounts" title="盈利账户数" width="120" align="center"></vxe-table-column>
          <vxe-table-column field="loss_accounts" title="亏损账户数" width="120" align="center"></vxe-table-column>
          <vxe-table-column field="liquidation_count" title="强平账户数" width="120" align="center"></vxe-table-column>
          <vxe-table-column field="status" title="状态" width="100">
            <template slot-scope="{ row }">
              <el-tag :type="getStatusTagType(row.status)" size="small">
                {{ getStatusName(row.status) }}
              </el-tag>
            </template>
          </vxe-table-column>
          <vxe-table-column field="execution_time" title="执行时间" width="180"></vxe-table-column>
          <vxe-table-column title="操作" width="100">
            <template slot-scope="{ row }">
              <el-button size="mini" type="text" @click="viewDetail(row)">详情</el-button>
            </template>
          </vxe-table-column>
        </vxe-table>
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

        <!-- TODO: 添加图表展示月度结算趋势 -->
      </el-tab-pane>
    </el-tabs>

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
  </div>
</template>

<script>
import dayjs from 'dayjs'
import {
  getSettlementHistory,
  setSettlementPrice,
  batchSetSettlementPrices,
  executeSettlement
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
      }
    }
  },
  mounted() {
    this.loadHistory()
    this.loadStatistics()
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

        const response = await getSettlementHistory(params)
        if (response.data && response.data.success) {
          this.historyList = response.data.data || []
        } else {
          const errorMsg = (response.data && response.data.error && response.data.error.message) || '加载结算历史失败'
          this.$message.error(errorMsg)
        }
      } catch (error) {
        this.$message.error('加载结算历史失败')
        console.error(error)
      } finally {
        this.historyLoading = false
      }
    },

    // 加载统计数据（从历史记录中计算）
    async loadStatistics() {
      try {
        // 从最近的结算历史中计算统计数据
        if (this.historyList.length > 0) {
          this.statistics = {
            monthSettlementCount: this.historyList.length,
            profitAccountsCount: this.historyList.reduce((sum, item) => sum + (item.profit_accounts || 0), 0),
            lossAccountsCount: this.historyList.reduce((sum, item) => sum + (item.loss_accounts || 0), 0),
            totalCommission: this.historyList.reduce((sum, item) => sum + (item.total_commission || 0), 0)
          }
        }
      } catch (error) {
        console.error('加载统计数据失败', error)
      }
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
      this.$message.info('批量导入功能开发中...')
      // TODO: 实现 CSV/Excel 导入功能
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
    viewDetail(row) {
      this.$message.info('查看详情功能开发中...')
      // TODO: 打开详情对话框，显示各账户的结算结果
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
    }
  }
}
</script>

<style scoped>
.settlement-container {
  padding: 20px;
}

.page-header {
  margin-bottom: 20px;
}

.page-header h2 {
  margin: 0;
  color: #303133;
}

.tabs-container {
  background: #fff;
  border-radius: 4px;
  box-shadow: 0 2px 4px rgba(0, 0, 0, 0.1);
  padding: 20px;
}

.settlement-card {
  margin-bottom: 20px;
}

.table-toolbar {
  margin-bottom: 15px;
}

.stats-row {
  margin-bottom: 20px;
}

.stat-card {
  display: flex;
  align-items: center;
  padding: 20px;
  background: #fff;
  border-radius: 8px;
  box-shadow: 0 2px 4px rgba(0, 0, 0, 0.1);
}

.stat-icon {
  font-size: 40px;
  margin-right: 15px;
  color: #409EFF;
}

.stat-value {
  font-size: 28px;
  font-weight: bold;
  color: #303133;
}

.stat-label {
  font-size: 14px;
  color: #909399;
  margin-top: 5px;
}
</style>
