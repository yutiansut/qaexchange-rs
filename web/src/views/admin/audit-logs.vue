<template>
  <div class="audit-logs-container">
    <!-- 页面标题 -->
    <div class="page-header">
      <h2>审计日志</h2>
      <div class="header-actions">
        <el-button icon="el-icon-refresh" @click="loadLogs">刷新</el-button>
        <el-button icon="el-icon-download" @click="exportLogs">导出</el-button>
      </div>
    </div>

    <!-- 搜索过滤 -->
    <el-card class="filter-card">
      <el-form :inline="true" :model="searchForm">
        <el-form-item label="账户ID">
          <el-input
            v-model="searchForm.account_id"
            placeholder="请输入账户ID"
            clearable
            style="width: 200px"
          ></el-input>
        </el-form-item>
        <el-form-item label="日志类型">
          <el-select v-model="searchForm.log_type" placeholder="全部类型" clearable style="width: 150px">
            <el-option label="登录" value="Login"></el-option>
            <el-option label="登出" value="Logout"></el-option>
            <el-option label="下单" value="OrderSubmit"></el-option>
            <el-option label="撤单" value="OrderCancel"></el-option>
            <el-option label="入金" value="Deposit"></el-option>
            <el-option label="出金" value="Withdraw"></el-option>
            <el-option label="转账" value="Transfer"></el-option>
            <el-option label="密码修改" value="PasswordChange"></el-option>
            <el-option label="账户冻结" value="AccountFreeze"></el-option>
            <el-option label="账户解冻" value="AccountUnfreeze"></el-option>
            <el-option label="风险警告" value="RiskWarning"></el-option>
            <el-option label="强制平仓" value="ForceLiquidation"></el-option>
            <el-option label="设置变更" value="SettingChange"></el-option>
            <el-option label="管理员操作" value="AdminAction"></el-option>
          </el-select>
        </el-form-item>
        <el-form-item label="时间范围">
          <el-date-picker
            v-model="searchForm.dateRange"
            type="datetimerange"
            range-separator="至"
            start-placeholder="开始时间"
            end-placeholder="结束时间"
            value-format="timestamp"
          ></el-date-picker>
        </el-form-item>
        <el-form-item>
          <el-button type="primary" icon="el-icon-search" @click="loadLogs">查询</el-button>
          <el-button icon="el-icon-refresh-left" @click="resetSearch">重置</el-button>
        </el-form-item>
      </el-form>
    </el-card>

    <!-- 日志列表 -->
    <el-card class="table-card">
      <el-table
        :data="logs"
        v-loading="loading"
        stripe
        border
        style="width: 100%"
      >
        <el-table-column prop="id" label="日志ID" width="180" show-overflow-tooltip></el-table-column>
        <el-table-column prop="timestamp" label="时间" width="180">
          <template slot-scope="scope">
            {{ formatTime(scope.row.timestamp) }}
          </template>
        </el-table-column>
        <el-table-column prop="account_id" label="账户ID" width="150"></el-table-column>
        <el-table-column prop="log_type" label="类型" width="120">
          <template slot-scope="scope">
            <el-tag :type="getLogTypeTag(scope.row.log_type)" size="small">
              {{ getLogTypeLabel(scope.row.log_type) }}
            </el-tag>
          </template>
        </el-table-column>
        <el-table-column prop="action" label="操作描述" min-width="200" show-overflow-tooltip></el-table-column>
        <el-table-column prop="ip_address" label="IP地址" width="140"></el-table-column>
        <el-table-column prop="result" label="结果" width="100">
          <template slot-scope="scope">
            <el-tag :type="scope.row.result === 'Success' ? 'success' : 'danger'" size="small">
              {{ scope.row.result === 'Success' ? '成功' : '失败' }}
            </el-tag>
          </template>
        </el-table-column>
        <el-table-column label="操作" width="100" fixed="right">
          <template slot-scope="scope">
            <el-button type="text" size="small" @click="viewDetail(scope.row)">
              详情
            </el-button>
          </template>
        </el-table-column>
      </el-table>

      <!-- 分页 -->
      <el-pagination
        class="pagination"
        @size-change="handleSizeChange"
        @current-change="handleCurrentChange"
        :current-page="pagination.page"
        :page-sizes="[20, 50, 100]"
        :page-size="pagination.page_size"
        layout="total, sizes, prev, pager, next, jumper"
        :total="pagination.total"
      ></el-pagination>
    </el-card>

    <!-- 详情弹窗 -->
    <el-dialog title="审计日志详情" :visible.sync="detailDialogVisible" width="600px">
      <el-descriptions :column="1" border v-if="currentLog">
        <el-descriptions-item label="日志ID">{{ currentLog.id }}</el-descriptions-item>
        <el-descriptions-item label="时间">{{ formatTime(currentLog.timestamp) }}</el-descriptions-item>
        <el-descriptions-item label="账户ID">{{ currentLog.account_id }}</el-descriptions-item>
        <el-descriptions-item label="类型">
          <el-tag :type="getLogTypeTag(currentLog.log_type)" size="small">
            {{ getLogTypeLabel(currentLog.log_type) }}
          </el-tag>
        </el-descriptions-item>
        <el-descriptions-item label="操作描述">{{ currentLog.action }}</el-descriptions-item>
        <el-descriptions-item label="IP地址">{{ currentLog.ip_address }}</el-descriptions-item>
        <el-descriptions-item label="User Agent">{{ currentLog.user_agent || '-' }}</el-descriptions-item>
        <el-descriptions-item label="结果">
          <el-tag :type="currentLog.result === 'Success' ? 'success' : 'danger'" size="small">
            {{ currentLog.result === 'Success' ? '成功' : '失败' }}
          </el-tag>
        </el-descriptions-item>
        <el-descriptions-item label="详细信息" v-if="currentLog.details">
          <pre class="details-pre">{{ JSON.stringify(currentLog.details, null, 2) }}</pre>
        </el-descriptions-item>
      </el-descriptions>
    </el-dialog>
  </div>
</template>

<script>
/**
 * 审计日志管理页面 @yutiansut @quantaxis
 */
import { queryAuditLogs, getAuditLog } from '@/api'

export default {
  name: 'AuditLogs',

  data() {
    return {
      loading: false,
      logs: [],
      searchForm: {
        account_id: '',
        log_type: '',
        dateRange: []
      },
      pagination: {
        page: 1,
        page_size: 20,
        total: 0
      },
      detailDialogVisible: false,
      currentLog: null,
      logTypeMap: {
        Login: '登录',
        Logout: '登出',
        OrderSubmit: '下单',
        OrderCancel: '撤单',
        Deposit: '入金',
        Withdraw: '出金',
        Transfer: '转账',
        PasswordChange: '密码修改',
        AccountFreeze: '账户冻结',
        AccountUnfreeze: '账户解冻',
        RiskWarning: '风险警告',
        ForceLiquidation: '强制平仓',
        SettingChange: '设置变更',
        AdminAction: '管理员操作'
      }
    }
  },

  created() {
    this.loadLogs()
  },

  methods: {
    async loadLogs() {
      this.loading = true
      try {
        const params = {
          page: this.pagination.page,
          page_size: this.pagination.page_size
        }
        if (this.searchForm.account_id) {
          params.account_id = this.searchForm.account_id
        }
        if (this.searchForm.log_type) {
          params.log_type = this.searchForm.log_type
        }
        if (this.searchForm.dateRange && this.searchForm.dateRange.length === 2) {
          params.start_time = this.searchForm.dateRange[0]
          params.end_time = this.searchForm.dateRange[1]
        }
        const res = await queryAuditLogs(params)
        if (res.success) {
          this.logs = res.data.logs || []
          this.pagination.total = res.data.total || 0
        }
      } catch (err) {
        console.error('加载审计日志失败:', err)
        this.$message.error('加载审计日志失败')
      } finally {
        this.loading = false
      }
    },

    async viewDetail(log) {
      try {
        const res = await getAuditLog(log.id)
        if (res.success) {
          this.currentLog = res.data
          this.detailDialogVisible = true
        }
      } catch (err) {
        console.error('获取日志详情失败:', err)
        this.$message.error('获取日志详情失败')
      }
    },

    resetSearch() {
      this.searchForm = {
        account_id: '',
        log_type: '',
        dateRange: []
      }
      this.pagination.page = 1
      this.loadLogs()
    },

    handleSizeChange(val) {
      this.pagination.page_size = val
      this.pagination.page = 1
      this.loadLogs()
    },

    handleCurrentChange(val) {
      this.pagination.page = val
      this.loadLogs()
    },

    exportLogs() {
      this.$message.info('导出功能开发中')
    },

    formatTime(timestamp) {
      if (!timestamp) return '-'
      const date = new Date(timestamp)
      return date.toLocaleString('zh-CN')
    },

    getLogTypeLabel(type) {
      return this.logTypeMap[type] || type
    },

    getLogTypeTag(type) {
      const dangerTypes = ['AccountFreeze', 'ForceLiquidation', 'RiskWarning']
      const warningTypes = ['Withdraw', 'PasswordChange']
      const successTypes = ['Login', 'Deposit']
      if (dangerTypes.includes(type)) return 'danger'
      if (warningTypes.includes(type)) return 'warning'
      if (successTypes.includes(type)) return 'success'
      return 'info'
    }
  }
}
</script>

<style scoped>
.audit-logs-container {
  padding: 20px;
}

.page-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 20px;
}

.page-header h2 {
  margin: 0;
  color: #303133;
}

.header-actions {
  display: flex;
  gap: 10px;
}

.filter-card {
  margin-bottom: 20px;
}

.table-card {
  margin-bottom: 20px;
}

.pagination {
  margin-top: 20px;
  text-align: right;
}

.details-pre {
  background: #f5f7fa;
  padding: 10px;
  border-radius: 4px;
  font-size: 12px;
  max-height: 200px;
  overflow: auto;
  white-space: pre-wrap;
  word-break: break-all;
}
</style>
