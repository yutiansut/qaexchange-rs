<template>
  <div class="announcements-page">
    <!-- 页面标题 -->
    <div class="page-header">
      <h2>
        <i class="el-icon-bell"></i>
        系统公告
      </h2>
      <p class="description">查看系统最新公告和通知</p>
    </div>

    <!-- 筛选区域 -->
    <div class="filter-section">
      <el-select
        v-model="filterType"
        placeholder="选择公告类型"
        clearable
        size="small"
        @change="loadAnnouncements"
      >
        <el-option label="全部" value="" />
        <el-option label="系统公告" value="System" />
        <el-option label="维护通知" value="Maintenance" />
        <el-option label="交易提醒" value="Trading" />
        <el-option label="风控通知" value="Risk" />
        <el-option label="活动推广" value="Promotion" />
      </el-select>
      <el-button size="small" icon="el-icon-refresh" @click="loadAnnouncements">刷新</el-button>
    </div>

    <!-- 公告列表 -->
    <div class="announcement-list" v-loading="loading">
      <el-empty v-if="!loading && announcements.length === 0" description="暂无公告" />

      <div
        v-for="announcement in announcements"
        :key="announcement.id"
        class="announcement-card"
        :class="{ 'urgent': announcement.priority === 'Urgent' }"
        @click="showDetail(announcement)"
      >
        <div class="card-header">
          <div class="tags">
            <el-tag :type="getPriorityType(announcement.priority)" size="small">
              {{ announcement.priority }}
            </el-tag>
            <el-tag type="info" size="small" effect="plain">
              {{ announcement.announcement_type }}
            </el-tag>
          </div>
          <span class="date">{{ formatDate(announcement.created_at) }}</span>
        </div>
        <h3 class="card-title">{{ announcement.title }}</h3>
        <p class="card-preview">{{ getContentPreview(announcement.content) }}</p>
        <div class="card-footer">
          <span class="read-more">查看详情 <i class="el-icon-arrow-right"></i></span>
        </div>
      </div>
    </div>

    <!-- 分页 -->
    <div class="pagination-wrapper" v-if="total > pageSize">
      <el-pagination
        layout="prev, pager, next"
        :total="total"
        :page-size="pageSize"
        :current-page.sync="currentPage"
        @current-change="loadAnnouncements"
      />
    </div>

    <!-- 公告详情弹窗 -->
    <el-dialog
      :title="selectedAnnouncement ? selectedAnnouncement.title : '公告详情'"
      :visible.sync="showDetailDialog"
      width="700px"
      custom-class="announcement-detail-dialog"
    >
      <div v-if="selectedAnnouncement" class="detail-content">
        <div class="detail-meta">
          <el-tag :type="getPriorityType(selectedAnnouncement.priority)" size="small">
            {{ selectedAnnouncement.priority }}
          </el-tag>
          <el-tag type="info" size="small" effect="plain">
            {{ selectedAnnouncement.announcement_type }}
          </el-tag>
          <span class="detail-date">发布时间: {{ formatDate(selectedAnnouncement.created_at) }}</span>
        </div>
        <div class="detail-body" v-html="selectedAnnouncement.content"></div>
        <div v-if="selectedAnnouncement.effective_until" class="detail-validity">
          <i class="el-icon-time"></i>
          有效期至: {{ formatDate(selectedAnnouncement.effective_until) }}
        </div>
      </div>
      <span slot="footer" class="dialog-footer">
        <el-button type="primary" @click="showDetailDialog = false">关闭</el-button>
      </span>
    </el-dialog>
  </div>
</template>

<script>
// @yutiansut @quantaxis - 用户公告页面
import { queryAnnouncements } from '@/api'

export default {
  name: 'Announcements',
  data() {
    return {
      loading: false,
      announcements: [],
      filterType: '',
      currentPage: 1,
      pageSize: 10,
      total: 0,
      showDetailDialog: false,
      selectedAnnouncement: null
    }
  },
  mounted() {
    this.loadAnnouncements()
  },
  methods: {
    async loadAnnouncements() {
      this.loading = true
      try {
        const params = {
          active_only: true,
          page: this.currentPage,
          page_size: this.pageSize
        }
        if (this.filterType) {
          params.announcement_type = this.filterType
        }
        const res = await queryAnnouncements(params)
        // 过滤有效期内的公告
        const now = Date.now()
        this.announcements = (res.announcements || []).filter(a => {
          const from = a.effective_from ? a.effective_from * 1000 : 0
          const until = a.effective_until ? a.effective_until * 1000 : Number.MAX_SAFE_INTEGER
          return now >= from && now <= until
        })
        // 按优先级和时间排序
        const priorityOrder = { 'Urgent': 0, 'High': 1, 'Normal': 2, 'Low': 3 }
        this.announcements.sort((a, b) => {
          const pDiff = (priorityOrder[a.priority] || 3) - (priorityOrder[b.priority] || 3)
          if (pDiff !== 0) return pDiff
          return (b.created_at || 0) - (a.created_at || 0)
        })
        this.total = res.total || this.announcements.length
      } catch (error) {
        console.error('加载公告失败', error)
        this.$message.error('加载公告失败')
      } finally {
        this.loading = false
      }
    },

    showDetail(announcement) {
      this.selectedAnnouncement = announcement
      this.showDetailDialog = true
    },

    formatDate(timestamp) {
      if (!timestamp) return ''
      const date = new Date(timestamp * 1000)
      return date.toLocaleDateString('zh-CN', {
        year: 'numeric',
        month: '2-digit',
        day: '2-digit',
        hour: '2-digit',
        minute: '2-digit'
      })
    },

    getPriorityType(priority) {
      const map = { 'Urgent': 'danger', 'High': 'warning', 'Normal': 'info', 'Low': '' }
      return map[priority] || 'info'
    },

    getContentPreview(content) {
      if (!content) return ''
      // 移除 HTML 标签，只显示纯文本
      const text = content.replace(/<[^>]+>/g, '')
      return text.length > 100 ? text.substring(0, 100) + '...' : text
    }
  }
}
</script>

<style lang="scss" scoped>
// @yutiansut @quantaxis - 公告页面深色主题
$primary-color: #1890ff;
$dark-bg-primary: #0d1117;
$dark-bg-secondary: #161b22;
$dark-bg-tertiary: #21262d;
$dark-border: #30363d;
$dark-text-primary: #f0f6fc;
$dark-text-secondary: #8b949e;
$dark-text-muted: #6e7681;

.announcements-page {
  min-height: calc(100vh - 150px);
}

.page-header {
  margin-bottom: 24px;

  h2 {
    display: flex;
    align-items: center;
    gap: 10px;
    color: $dark-text-primary;
    font-size: 24px;
    font-weight: 600;
    margin: 0 0 8px;

    i {
      color: $primary-color;
    }
  }

  .description {
    color: $dark-text-secondary;
    margin: 0;
    font-size: 14px;
  }
}

.filter-section {
  display: flex;
  gap: 12px;
  margin-bottom: 20px;

  ::v-deep .el-select {
    width: 200px;

    .el-input__inner {
      background: $dark-bg-tertiary;
      border-color: $dark-border;
      color: $dark-text-primary;
    }
  }

  ::v-deep .el-button {
    background: $dark-bg-tertiary;
    border-color: $dark-border;
    color: $dark-text-secondary;

    &:hover {
      border-color: $primary-color;
      color: $primary-color;
    }
  }
}

.announcement-list {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.announcement-card {
  background: $dark-bg-secondary;
  border: 1px solid $dark-border;
  border-radius: 12px;
  padding: 20px;
  cursor: pointer;
  transition: all 0.2s ease;

  &:hover {
    border-color: $primary-color;
    transform: translateY(-2px);
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
  }

  &.urgent {
    border-left: 4px solid #f56c6c;
  }

  .card-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 12px;
  }

  .tags {
    display: flex;
    gap: 8px;
  }

  .date {
    color: $dark-text-muted;
    font-size: 12px;
  }

  .card-title {
    color: $dark-text-primary;
    font-size: 16px;
    font-weight: 600;
    margin: 0 0 10px;
  }

  .card-preview {
    color: $dark-text-secondary;
    font-size: 14px;
    line-height: 1.6;
    margin: 0 0 12px;
  }

  .card-footer {
    display: flex;
    justify-content: flex-end;
  }

  .read-more {
    color: $primary-color;
    font-size: 13px;
    display: flex;
    align-items: center;
    gap: 4px;
  }
}

.pagination-wrapper {
  display: flex;
  justify-content: center;
  margin-top: 24px;

  ::v-deep .el-pagination {
    .btn-prev, .btn-next, .el-pager li {
      background: $dark-bg-tertiary;
      border: 1px solid $dark-border;
      color: $dark-text-secondary;

      &:hover {
        color: $primary-color;
      }

      &.active {
        background: $primary-color;
        border-color: $primary-color;
        color: white;
      }
    }
  }
}

// 详情弹窗样式
::v-deep .announcement-detail-dialog {
  background: $dark-bg-secondary !important;
  border: 1px solid $dark-border;
  border-radius: 12px;

  .el-dialog__header {
    border-bottom: 1px solid $dark-border;
    padding: 16px 20px;

    .el-dialog__title {
      color: $dark-text-primary;
      font-size: 18px;
      font-weight: 600;
    }

    .el-dialog__headerbtn .el-dialog__close {
      color: $dark-text-secondary;
    }
  }

  .el-dialog__body {
    padding: 20px;
    max-height: 500px;
    overflow-y: auto;
  }

  .el-dialog__footer {
    border-top: 1px solid $dark-border;
    padding: 16px 20px;
  }
}

.detail-content {
  .detail-meta {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-bottom: 20px;
    padding-bottom: 16px;
    border-bottom: 1px solid $dark-border;
  }

  .detail-date {
    color: $dark-text-muted;
    font-size: 12px;
    margin-left: auto;
  }

  .detail-body {
    color: $dark-text-secondary;
    font-size: 14px;
    line-height: 1.8;

    p {
      margin: 0 0 12px;
    }

    a {
      color: $primary-color;
    }
  }

  .detail-validity {
    margin-top: 20px;
    padding-top: 16px;
    border-top: 1px solid $dark-border;
    color: $dark-text-muted;
    font-size: 13px;
    display: flex;
    align-items: center;
    gap: 6px;
  }
}
</style>
