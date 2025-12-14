<template>
  <div class="announcements-container">
    <!-- 页面标题 -->
    <div class="page-header">
      <h2>系统公告管理</h2>
      <div class="header-actions">
        <el-button type="primary" icon="el-icon-plus" @click="showCreateDialog">新建公告</el-button>
        <el-button icon="el-icon-refresh" @click="loadAnnouncements">刷新</el-button>
      </div>
    </div>

    <!-- 搜索过滤 -->
    <el-card class="filter-card">
      <el-form :inline="true" :model="searchForm">
        <el-form-item label="公告类型">
          <el-select v-model="searchForm.announcement_type" placeholder="全部类型" clearable style="width: 150px">
            <el-option label="系统公告" value="System"></el-option>
            <el-option label="维护通知" value="Maintenance"></el-option>
            <el-option label="交易提醒" value="Trading"></el-option>
            <el-option label="风险提示" value="Risk"></el-option>
            <el-option label="活动推广" value="Promotion"></el-option>
          </el-select>
        </el-form-item>
        <el-form-item label="状态">
          <el-select v-model="searchForm.active_only" placeholder="全部状态" style="width: 120px">
            <el-option label="全部" :value="false"></el-option>
            <el-option label="有效" :value="true"></el-option>
          </el-select>
        </el-form-item>
        <el-form-item>
          <el-button type="primary" icon="el-icon-search" @click="loadAnnouncements">查询</el-button>
        </el-form-item>
      </el-form>
    </el-card>

    <!-- 公告列表 -->
    <el-card class="table-card">
      <el-table
        :data="announcements"
        v-loading="loading"
        stripe
        border
        style="width: 100%"
      >
        <el-table-column prop="id" label="公告ID" width="180" show-overflow-tooltip></el-table-column>
        <el-table-column prop="title" label="标题" min-width="200" show-overflow-tooltip></el-table-column>
        <el-table-column prop="announcement_type" label="类型" width="120">
          <template slot-scope="scope">
            <el-tag :type="getTypeTag(scope.row.announcement_type)" size="small">
              {{ getTypeLabel(scope.row.announcement_type) }}
            </el-tag>
          </template>
        </el-table-column>
        <el-table-column prop="priority" label="优先级" width="100">
          <template slot-scope="scope">
            <el-tag :type="getPriorityTag(scope.row.priority)" size="small">
              {{ getPriorityLabel(scope.row.priority) }}
            </el-tag>
          </template>
        </el-table-column>
        <el-table-column prop="created_at" label="创建时间" width="180">
          <template slot-scope="scope">
            {{ formatTime(scope.row.created_at) }}
          </template>
        </el-table-column>
        <el-table-column prop="effective_until" label="有效期" width="180">
          <template slot-scope="scope">
            {{ scope.row.effective_until ? formatTime(scope.row.effective_until) : '永久' }}
          </template>
        </el-table-column>
        <el-table-column label="状态" width="80">
          <template slot-scope="scope">
            <el-tag :type="isActive(scope.row) ? 'success' : 'info'" size="small">
              {{ isActive(scope.row) ? '有效' : '已过期' }}
            </el-tag>
          </template>
        </el-table-column>
        <el-table-column label="操作" width="150" fixed="right">
          <template slot-scope="scope">
            <el-button type="text" size="small" @click="viewDetail(scope.row)">查看</el-button>
            <el-button type="text" size="small" style="color: #F56C6C;" @click="handleDelete(scope.row)">删除</el-button>
          </template>
        </el-table-column>
      </el-table>

      <!-- 分页 -->
      <el-pagination
        class="pagination"
        @size-change="handleSizeChange"
        @current-change="handleCurrentChange"
        :current-page="pagination.page"
        :page-sizes="[10, 20, 50]"
        :page-size="pagination.page_size"
        layout="total, sizes, prev, pager, next, jumper"
        :total="pagination.total"
      ></el-pagination>
    </el-card>

    <!-- 新建公告弹窗 -->
    <el-dialog title="新建公告" :visible.sync="createDialogVisible" width="600px">
      <el-form :model="createForm" :rules="createRules" ref="createForm" label-width="100px">
        <el-form-item label="标题" prop="title">
          <el-input v-model="createForm.title" placeholder="请输入公告标题"></el-input>
        </el-form-item>
        <el-form-item label="内容" prop="content">
          <el-input
            type="textarea"
            v-model="createForm.content"
            :rows="6"
            placeholder="请输入公告内容"
          ></el-input>
        </el-form-item>
        <el-form-item label="公告类型" prop="announcement_type">
          <el-select v-model="createForm.announcement_type" placeholder="请选择公告类型" style="width: 100%">
            <el-option label="系统公告" value="System"></el-option>
            <el-option label="维护通知" value="Maintenance"></el-option>
            <el-option label="交易提醒" value="Trading"></el-option>
            <el-option label="风险提示" value="Risk"></el-option>
            <el-option label="活动推广" value="Promotion"></el-option>
          </el-select>
        </el-form-item>
        <el-form-item label="优先级" prop="priority">
          <el-select v-model="createForm.priority" placeholder="请选择优先级" style="width: 100%">
            <el-option label="低" value="Low"></el-option>
            <el-option label="普通" value="Normal"></el-option>
            <el-option label="高" value="High"></el-option>
            <el-option label="紧急" value="Urgent"></el-option>
          </el-select>
        </el-form-item>
        <el-form-item label="有效期">
          <el-date-picker
            v-model="createForm.dateRange"
            type="datetimerange"
            range-separator="至"
            start-placeholder="生效时间"
            end-placeholder="失效时间"
            value-format="timestamp"
            style="width: 100%"
          ></el-date-picker>
        </el-form-item>
      </el-form>
      <div slot="footer">
        <el-button @click="createDialogVisible = false">取消</el-button>
        <el-button type="primary" @click="submitCreate" :loading="submitting">发布</el-button>
      </div>
    </el-dialog>

    <!-- 详情弹窗 -->
    <el-dialog title="公告详情" :visible.sync="detailDialogVisible" width="600px">
      <el-descriptions :column="1" border v-if="currentAnnouncement">
        <el-descriptions-item label="公告ID">{{ currentAnnouncement.id }}</el-descriptions-item>
        <el-descriptions-item label="标题">{{ currentAnnouncement.title }}</el-descriptions-item>
        <el-descriptions-item label="类型">
          <el-tag :type="getTypeTag(currentAnnouncement.announcement_type)" size="small">
            {{ getTypeLabel(currentAnnouncement.announcement_type) }}
          </el-tag>
        </el-descriptions-item>
        <el-descriptions-item label="优先级">
          <el-tag :type="getPriorityTag(currentAnnouncement.priority)" size="small">
            {{ getPriorityLabel(currentAnnouncement.priority) }}
          </el-tag>
        </el-descriptions-item>
        <el-descriptions-item label="创建时间">{{ formatTime(currentAnnouncement.created_at) }}</el-descriptions-item>
        <el-descriptions-item label="创建者">{{ currentAnnouncement.created_by || '-' }}</el-descriptions-item>
        <el-descriptions-item label="生效时间">{{ currentAnnouncement.effective_from ? formatTime(currentAnnouncement.effective_from) : '立即生效' }}</el-descriptions-item>
        <el-descriptions-item label="失效时间">{{ currentAnnouncement.effective_until ? formatTime(currentAnnouncement.effective_until) : '永久有效' }}</el-descriptions-item>
        <el-descriptions-item label="内容">
          <div class="announcement-content">{{ currentAnnouncement.content }}</div>
        </el-descriptions-item>
      </el-descriptions>
    </el-dialog>
  </div>
</template>

<script>
/**
 * 系统公告管理页面 @yutiansut @quantaxis
 */
import { queryAnnouncements, getAnnouncement, createAnnouncement, deleteAnnouncement } from '@/api'

export default {
  name: 'Announcements',

  data() {
    return {
      loading: false,
      submitting: false,
      announcements: [],
      searchForm: {
        announcement_type: '',
        active_only: false
      },
      pagination: {
        page: 1,
        page_size: 10,
        total: 0
      },
      createDialogVisible: false,
      detailDialogVisible: false,
      currentAnnouncement: null,
      createForm: {
        title: '',
        content: '',
        announcement_type: 'System',
        priority: 'Normal',
        dateRange: []
      },
      createRules: {
        title: [{ required: true, message: '请输入公告标题', trigger: 'blur' }],
        content: [{ required: true, message: '请输入公告内容', trigger: 'blur' }],
        announcement_type: [{ required: true, message: '请选择公告类型', trigger: 'change' }],
        priority: [{ required: true, message: '请选择优先级', trigger: 'change' }]
      },
      typeMap: {
        System: '系统公告',
        Maintenance: '维护通知',
        Trading: '交易提醒',
        Risk: '风险提示',
        Promotion: '活动推广'
      },
      priorityMap: {
        Low: '低',
        Normal: '普通',
        High: '高',
        Urgent: '紧急'
      }
    }
  },

  created() {
    this.loadAnnouncements()
  },

  methods: {
    async loadAnnouncements() {
      this.loading = true
      try {
        const params = {
          page: this.pagination.page,
          page_size: this.pagination.page_size,
          active_only: this.searchForm.active_only
        }
        if (this.searchForm.announcement_type) {
          params.announcement_type = this.searchForm.announcement_type
        }
        const res = await queryAnnouncements(params)
        if (res.success) {
          this.announcements = res.data.announcements || []
          this.pagination.total = res.data.total || 0
        }
      } catch (err) {
        console.error('加载公告失败:', err)
        this.$message.error('加载公告失败')
      } finally {
        this.loading = false
      }
    },

    showCreateDialog() {
      this.createForm = {
        title: '',
        content: '',
        announcement_type: 'System',
        priority: 'Normal',
        dateRange: []
      }
      this.createDialogVisible = true
    },

    async submitCreate() {
      this.$refs.createForm.validate(async (valid) => {
        if (!valid) return

        this.submitting = true
        try {
          const data = {
            admin_token: 'demo_admin_token', // 实际应从登录状态获取
            title: this.createForm.title,
            content: this.createForm.content,
            announcement_type: this.createForm.announcement_type,
            priority: this.createForm.priority
          }
          if (this.createForm.dateRange && this.createForm.dateRange.length === 2) {
            data.effective_from = this.createForm.dateRange[0]
            data.effective_until = this.createForm.dateRange[1]
          }
          const res = await createAnnouncement(data)
          if (res.success) {
            this.$message.success('公告发布成功')
            this.createDialogVisible = false
            this.loadAnnouncements()
          } else {
            this.$message.error(res.error || '发布失败')
          }
        } catch (err) {
          console.error('发布公告失败:', err)
          this.$message.error('发布公告失败')
        } finally {
          this.submitting = false
        }
      })
    },

    async viewDetail(row) {
      try {
        const res = await getAnnouncement(row.id)
        if (res.success) {
          this.currentAnnouncement = res.data
          this.detailDialogVisible = true
        }
      } catch (err) {
        console.error('获取公告详情失败:', err)
        this.$message.error('获取公告详情失败')
      }
    },

    handleDelete(row) {
      this.$confirm('确定要删除该公告吗？', '提示', {
        confirmButtonText: '确定',
        cancelButtonText: '取消',
        type: 'warning'
      }).then(async () => {
        try {
          const res = await deleteAnnouncement(row.id)
          if (res.success) {
            this.$message.success('删除成功')
            this.loadAnnouncements()
          } else {
            this.$message.error(res.error || '删除失败')
          }
        } catch (err) {
          console.error('删除公告失败:', err)
          this.$message.error('删除公告失败')
        }
      }).catch(() => {})
    },

    handleSizeChange(val) {
      this.pagination.page_size = val
      this.pagination.page = 1
      this.loadAnnouncements()
    },

    handleCurrentChange(val) {
      this.pagination.page = val
      this.loadAnnouncements()
    },

    formatTime(timestamp) {
      if (!timestamp) return '-'
      const date = new Date(timestamp)
      return date.toLocaleString('zh-CN')
    },

    getTypeLabel(type) {
      return this.typeMap[type] || type
    },

    getTypeTag(type) {
      const tagMap = {
        System: '',
        Maintenance: 'warning',
        Trading: 'success',
        Risk: 'danger',
        Promotion: 'info'
      }
      return tagMap[type] || ''
    },

    getPriorityLabel(priority) {
      return this.priorityMap[priority] || priority
    },

    getPriorityTag(priority) {
      const tagMap = {
        Low: 'info',
        Normal: '',
        High: 'warning',
        Urgent: 'danger'
      }
      return tagMap[priority] || ''
    },

    isActive(row) {
      if (!row.effective_until) return true
      return new Date(row.effective_until) > new Date()
    }
  }
}
</script>

<style scoped>
.announcements-container {
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

.announcement-content {
  white-space: pre-wrap;
  word-break: break-all;
  max-height: 300px;
  overflow: auto;
  padding: 10px;
  background: #f5f7fa;
  border-radius: 4px;
}
</style>
