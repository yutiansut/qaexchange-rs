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
            <el-option label="系统公告" value="SYSTEM"></el-option>
            <el-option label="维护通知" value="MAINTENANCE"></el-option>
            <el-option label="交易提醒" value="TRADING"></el-option>
            <el-option label="风险提示" value="RISK"></el-option>
            <el-option label="活动推广" value="PROMOTION"></el-option>
          </el-select>
        </el-form-item>
        <el-form-item label="状态">
          <el-select v-model="searchForm.active_only" placeholder="全部状态" style="width: 120px">
            <el-option label="全部" :value="false"></el-option>
            <el-option label="有效" :value="true"></el-option>
          </el-select>
        </el-form-item>
        <el-form-item>
          <el-button type="primary" icon="el-icon-search" @click="handleSearch">查询</el-button>
        </el-form-item>
      </el-form>
    </el-card>

    <!-- 公告列表 -->
    <el-card class="table-card">
      <el-table
        :empty-text="$emptyText(loadError)"
        :data="announcements"
        v-loading="loading"
        stripe
        border
        style="width: 100%"
      >
        <el-table-column prop="id" label="公告ID" min-width="180" show-overflow-tooltip></el-table-column>
        <el-table-column prop="title" label="标题" min-width="200" show-overflow-tooltip></el-table-column>
        <el-table-column prop="announcement_type" label="类型" min-width="120">
          <template slot-scope="scope">
            <el-tag :type="getTypeTag(scope.row.announcement_type)" size="small">
              {{ getTypeLabel(scope.row.announcement_type) }}
            </el-tag>
          </template>
        </el-table-column>
        <el-table-column prop="priority" label="优先级" min-width="100">
          <template slot-scope="scope">
            <el-tag :type="getPriorityTag(scope.row.priority)" size="small">
              {{ getPriorityLabel(scope.row.priority) }}
            </el-tag>
          </template>
        </el-table-column>
        <el-table-column prop="publish_time" label="发布时间" min-width="180">
          <template slot-scope="scope">
            {{ formatTime(scope.row.publish_time) }}
          </template>
        </el-table-column>
        <el-table-column prop="expire_time" label="有效期" min-width="180">
          <template slot-scope="scope">
            {{ scope.row.expire_time ? formatTime(scope.row.expire_time) : '永久' }}
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
            <el-option label="系统公告" value="SYSTEM"></el-option>
            <el-option label="维护通知" value="MAINTENANCE"></el-option>
            <el-option label="交易提醒" value="TRADING"></el-option>
            <el-option label="风险提示" value="RISK"></el-option>
            <el-option label="活动推广" value="PROMOTION"></el-option>
          </el-select>
        </el-form-item>
        <el-form-item label="优先级" prop="priority">
          <el-select v-model="createForm.priority" placeholder="请选择优先级" style="width: 100%">
            <el-option label="低" value="LOW"></el-option>
            <el-option label="普通" value="NORMAL"></el-option>
            <el-option label="高" value="HIGH"></el-option>
            <el-option label="紧急" value="URGENT"></el-option>
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
        <el-descriptions-item label="发布时间">{{ formatTime(currentAnnouncement.publish_time) }}</el-descriptions-item>
        <!-- 后端字段是 author,没有 created_by(models.rs:666)。@yutiansut @quantaxis -->
        <el-descriptions-item label="创建者">{{ currentAnnouncement.author || '-' }}</el-descriptions-item>
        <el-descriptions-item label="失效时间">{{ currentAnnouncement.expire_time ? formatTime(currentAnnouncement.expire_time) : '永久有效' }}</el-descriptions-item>
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
      // 加载失败原因 —— 与「真的没数据」区分开,交给 $emptyText 渲染
      // @yutiansut @quantaxis
      loadError: null,
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
        announcement_type: 'SYSTEM',
        priority: 'NORMAL',
        dateRange: []
      },
      createRules: {
        title: [{ required: true, message: '请输入公告标题', trigger: 'blur' }],
        content: [{ required: true, message: '请输入公告内容', trigger: 'blur' }],
        announcement_type: [{ required: true, message: '请选择公告类型', trigger: 'change' }],
        priority: [{ required: true, message: '请选择优先级', trigger: 'change' }]
      },
      // 后端使用 SCREAMING_SNAKE_CASE @yutiansut @quantaxis
      typeMap: {
        SYSTEM: '系统公告',
        MAINTENANCE: '维护通知',
        TRADING: '交易提醒',
        RISK: '风险提示',
        PROMOTION: '活动推广'
      },
      priorityMap: {
        LOW: '低',
        NORMAL: '普通',
        HIGH: '高',
        URGENT: '紧急'
      }
    }
  },

  created() {
    this.loadAnnouncements()
  },

  methods: {
    // 查询 —— 必须先回第一页再拉数据。@yutiansut @quantaxis
    //
    // ⚠️ 原来「查询」按钮直接绑 loadAnnouncements,不重置页码。
    // 于是:翻到第 3 页 → 改筛选条件 → 点查询 → 仍带 page=3 请求。
    // 新条件下总数可能只有 1 页,后端返回空数组,页面显示「暂无数据」,
    // 而分页条显示「共 N 条」—— 与 P1-5 完全同一种「查到了却看不见」。
    // 不能直接在 loadAnnouncements 里重置:它同时被两个分页 handler 调用
    // (:339 / :344),塞 page=1 会让分页永远翻不动。故单开一层。
    handleSearch() {
      this.pagination.page = 1
      this.loadAnnouncements()
    },

    async loadAnnouncements() {
      this.loading = true; this.loadError = null
      try {
        const params = {
          page: this.pagination.page,
          page_size: this.pagination.page_size,
          active_only: this.searchForm.active_only
        }
        if (this.searchForm.announcement_type) {
          params.announcement_type = this.searchForm.announcement_type
        }
        // axios拦截器已提取data @yutiansut @quantaxis
        const data = await queryAnnouncements(params)
        this.announcements = (data && data.announcements) || []
        this.pagination.total = (data && data.total) || 0
      } catch (err) {
        this.loadError = (err && err.message) || '加载失败'
        console.error('加载公告失败:', err)
        this.$message.error(this.$errMsg(err, '加载公告失败'))
      } finally {
        this.loading = false
      }
    },

    showCreateDialog() {
      this.createForm = {
        title: '',
        content: '',
        announcement_type: 'SYSTEM',
        priority: 'NORMAL',
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
            admin_token: 'qaexchange_admin_2024', // 管理员令牌 @yutiansut @quantaxis
            title: this.createForm.title,
            content: this.createForm.content,
            announcement_type: this.createForm.announcement_type,
            priority: this.createForm.priority
          }
          if (this.createForm.dateRange && this.createForm.dateRange.length === 2) {
            data.publish_time = this.createForm.dateRange[0]
            data.expire_time = this.createForm.dateRange[1]
          }
          // axios拦截器已处理响应 @yutiansut @quantaxis
          await createAnnouncement(data)
          this.$message.success('公告发布成功')
          this.createDialogVisible = false
          this.loadAnnouncements()
        } catch (err) {
          console.error('发布公告失败:', err)
          this.$message.error(this.$errMsg(err, '发布公告失败'))
        } finally {
          this.submitting = false
        }
      })
    },

    async viewDetail(row) {
      try {
        // axios拦截器已提取data @yutiansut @quantaxis
        const data = await getAnnouncement(row.id)
        this.currentAnnouncement = data
        this.detailDialogVisible = true
      } catch (err) {
        console.error('获取公告详情失败:', err)
        this.$message.error(this.$errMsg(err, '获取公告详情失败'))
      }
    },

    handleDelete(row) {
      this.$confirm('确定要删除该公告吗？', '提示', {
        confirmButtonText: '确定',
        cancelButtonText: '取消',
        type: 'warning'
      }).then(async () => {
        try {
          // axios拦截器已处理响应 @yutiansut @quantaxis
          await deleteAnnouncement(row.id)
          this.$message.success('删除成功')
          this.loadAnnouncements()
        } catch (err) {
          console.error('删除公告失败:', err)
          this.$message.error(this.$errMsg(err, '删除公告失败'))
        }
      // 用户点「取消」时 $confirm 会 reject('cancel'),空 catch 是接住它,
      // 不接会产生一条未捕获的 Promise rejection。@yutiansut @quantaxis
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
      // ✨ 键必须是 SCREAMING_SNAKE —— 后端 models.rs:619 的两个枚举都带
      //    #[serde(rename_all = "SCREAMING_SNAKE_CASE")]。上一轮改了同文件的
      //    typeMap/priorityMap(决定文字)却漏了这两个 tagMap(决定颜色),
      //    结果文字正常但颜色全部 fall through 到默认灰。 @yutiansut @quantaxis
      const tagMap = {
        SYSTEM: '',
        MAINTENANCE: 'warning',
        TRADING: 'success',
        RISK: 'danger',
        PROMOTION: 'info'
      }
      return tagMap[type] || ''
    },

    getPriorityLabel(priority) {
      return this.priorityMap[priority] || priority
    },

    getPriorityTag(priority) {
      const tagMap = {
        LOW: 'info',
        NORMAL: '',
        HIGH: 'warning',
        URGENT: 'danger'
      }
      return tagMap[priority] || ''
    },

    isActive(row) {
      if (!row.expire_time) return true
      return new Date(row.expire_time) > new Date()
    }
  }
}
</script>

<style scoped lang="scss">
@import '@/styles/variables.scss';
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
  color: $dark-text-primary;
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
