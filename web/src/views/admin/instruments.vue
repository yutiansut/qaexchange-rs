<template>
  <div class="instruments-container">
    <!-- 页面标题 -->
    <div class="page-header">
      <h2>合约管理</h2>
      <el-button type="primary" icon="el-icon-plus" @click="showCreateDialog">上市新合约</el-button>
    </div>

    <!-- 合约列表 -->
    <div class="table-container">
      <!-- ✨ 表格高度改为跟随视口, 不再写死 600px @yutiansut @quantaxis
           实测 1366x768: 表格上沿距内容区顶部 93px
           var(--qa-content-h) 由 layout/index.vue 统一定义 = 100vh - 56(顶栏) - 40(padding),
           有公告条时自动再减 40px; max(260px, ...) 是极短视口下的兜底 -->
      <el-table
        :empty-text="$emptyText(loadError)"
        ref="instrumentTable"
        :data="instruments"
        border
        stripe
        v-loading="loading"
        :height="'max(260px, calc(var(--qa-content-h, calc(100vh - 96px)) - 134px))'"
      >
        <el-table-column prop="instrument_id" label="合约代码" min-width="120" sortable show-overflow-tooltip></el-table-column>
        <el-table-column prop="instrument_name" label="合约名称" min-width="195"></el-table-column>
        <!-- show-overflow-tooltip:该列实测被 ellipsis 截断且无提示,hover 也弹不出全文。
             不加宽是因为这张表在 1280 下已横向溢出,加宽会加剧;
             tooltip 让内容 hover 可达而不增加列宽。@yutiansut @quantaxis -->
        <el-table-column prop="instrument_type" label="类型" min-width="80" sortable show-overflow-tooltip>
          <template slot-scope="scope">
            <el-tag :type="getTypeTagType(scope.row.instrument_type)" size="small">
              {{ getTypeName(scope.row.instrument_type) }}
            </el-tag>
          </template>
        </el-table-column>
        <el-table-column prop="exchange" label="交易所" min-width="70"></el-table-column>
        <el-table-column prop="contract_multiplier" label="合约乘数" min-width="100" align="right" show-overflow-tooltip></el-table-column>
        <el-table-column prop="price_tick" label="最小变动价位" min-width="120" align="right" show-overflow-tooltip>
          <template slot-scope="scope">
            {{ $num(scope.row.price_tick, 2) }}
          </template>
        </el-table-column>
        <el-table-column prop="margin_rate" label="保证金率" min-width="100" align="right" show-overflow-tooltip>
          <template slot-scope="scope">
            {{ $pct(scope.row.margin_rate, 1) }}
          </template>
        </el-table-column>
        <el-table-column prop="commission_rate" label="手续费率" min-width="100" align="right" show-overflow-tooltip>
          <template slot-scope="scope">
            {{ $pct(scope.row.commission_rate, 2) }}
          </template>
        </el-table-column>
        <el-table-column prop="status" label="状态" min-width="70" sortable>
          <template slot-scope="scope">
            <el-tag :type="getStatusTagType(scope.row.status)" size="small">
              {{ getStatusName(scope.row.status) }}
            </el-tag>
          </template>
        </el-table-column>
        <el-table-column prop="list_date" label="上市日期" min-width="95"></el-table-column>
        <el-table-column prop="expire_date" label="到期日期" min-width="95"></el-table-column>
        <el-table-column label="操作" width="200">
          <template slot-scope="scope">
            <el-button size="mini" type="text" @click="showEditDialog(scope.row)">编辑</el-button>
            <el-button
              size="mini"
              type="text"
              v-if="scope.row.status === 'active'"
              @click="suspendInstrument(scope.row)"
            >
              暂停交易
            </el-button>
            <el-button
              size="mini"
              type="text"
              v-if="scope.row.status === 'suspended'"
              @click="resumeInstrument(scope.row)"
            >
              恢复交易
            </el-button>
            <el-button
              size="mini"
              type="text"
              style="color: #F56C6C"
              @click="delistInstrument(scope.row)"
            >
              下市
            </el-button>
          </template>
        </el-table-column>
      </el-table>
    </div>

    <!-- 创建/编辑合约对话框 -->
    <el-dialog
      :title="dialogTitle"
      :visible.sync="dialogVisible"
      width="600px"
      :close-on-click-modal="false"
    >
      <el-form :model="form" :rules="rules" ref="instrumentForm" label-width="120px">
        <el-form-item label="合约代码" prop="instrument_id">
          <el-input
            v-model="form.instrument_id"
            :disabled="isEdit"
            placeholder="例如：IF2501"
          ></el-input>
        </el-form-item>

        <el-form-item label="合约名称" prop="instrument_name">
          <el-input v-model="form.instrument_name" placeholder="例如：沪深300股指期货2501"></el-input>
        </el-form-item>

        <el-form-item label="合约类型" prop="instrument_type">
          <el-select v-model="form.instrument_type" placeholder="请选择">
            <el-option label="股指期货" value="index_future"></el-option>
            <el-option label="商品期货" value="commodity_future"></el-option>
            <el-option label="股票" value="stock"></el-option>
            <el-option label="期权" value="option"></el-option>
          </el-select>
        </el-form-item>

        <el-form-item label="交易所" prop="exchange">
          <el-select v-model="form.exchange" placeholder="请选择">
            <el-option label="中金所(CFFEX)" value="CFFEX"></el-option>
            <el-option label="上期所(SHFE)" value="SHFE"></el-option>
            <el-option label="大商所(DCE)" value="DCE"></el-option>
            <el-option label="郑商所(CZCE)" value="CZCE"></el-option>
            <el-option label="上交所(SSE)" value="SSE"></el-option>
            <el-option label="深交所(SZSE)" value="SZSE"></el-option>
          </el-select>
        </el-form-item>

        <el-form-item label="合约乘数" prop="contract_multiplier">
          <el-input-number
            v-model="form.contract_multiplier"
            :min="1"
            :max="10000"
            controls-position="right"
          ></el-input-number>
        </el-form-item>

        <el-form-item label="最小变动价位" prop="price_tick">
          <el-input-number
            v-model="form.price_tick"
            :min="0.01"
            :step="0.01"
            :precision="2"
            controls-position="right"
          ></el-input-number>
        </el-form-item>

        <!-- 上市初始价：订单簿基准价 / 初始结算价 / 昨收盘价 @yutiansut @quantaxis -->
        <el-form-item label="上市初始价" prop="init_price">
          <el-input-number
            v-model="form.init_price"
            :min="0.01"
            :step="1"
            :precision="2"
            :disabled="isEdit"
            controls-position="right"
          ></el-input-number>
          <span style="margin-left: 10px; color: #909399;">
            {{ isEdit ? '已上市合约不可修改基准价' : '订单簿基准价 / 初始结算价 / 昨收盘价' }}
          </span>
        </el-form-item>

        <el-form-item label="保证金率" prop="margin_rate">
          <el-input-number
            v-model="form.margin_rate"
            :min="0.01"
            :max="1"
            :step="0.01"
            :precision="4"
            controls-position="right"
          ></el-input-number>
          <span style="margin-left: 10px; color: #909399;">{{ (form.margin_rate * 100).toFixed(1) }}%</span>
        </el-form-item>

        <el-form-item label="手续费率" prop="commission_rate">
          <el-input-number
            v-model="form.commission_rate"
            :min="0.0001"
            :max="0.01"
            :step="0.0001"
            :precision="4"
            controls-position="right"
          ></el-input-number>
          <span style="margin-left: 10px; color: #909399;">{{ (form.commission_rate * 100).toFixed(2) }}%</span>
        </el-form-item>

        <el-form-item label="涨停板" prop="limit_up_rate">
          <el-input-number
            v-model="form.limit_up_rate"
            :min="0.01"
            :max="1"
            :step="0.01"
            :precision="2"
            controls-position="right"
          ></el-input-number>
          <span style="margin-left: 10px; color: #909399;">{{ (form.limit_up_rate * 100).toFixed(0) }}%</span>
        </el-form-item>

        <el-form-item label="跌停板" prop="limit_down_rate">
          <el-input-number
            v-model="form.limit_down_rate"
            :min="0.01"
            :max="1"
            :step="0.01"
            :precision="2"
            controls-position="right"
          ></el-input-number>
          <span style="margin-left: 10px; color: #909399;">{{ (form.limit_down_rate * 100).toFixed(0) }}%</span>
        </el-form-item>

        <el-form-item label="上市日期" prop="list_date">
          <el-date-picker
            v-model="form.list_date"
            type="date"
            placeholder="选择日期"
            value-format="yyyy-MM-dd"
          ></el-date-picker>
        </el-form-item>

        <el-form-item label="到期日期" prop="expire_date">
          <el-date-picker
            v-model="form.expire_date"
            type="date"
            placeholder="选择日期"
            value-format="yyyy-MM-dd"
          ></el-date-picker>
        </el-form-item>
      </el-form>

      <div slot="footer" class="dialog-footer">
        <el-button @click="dialogVisible = false">取消</el-button>
        <el-button type="primary" @click="submitForm" :loading="submitting">确定</el-button>
      </div>
    </el-dialog>
  </div>
</template>

<script>
import {
  getAllInstruments,
  createInstrument,
  updateInstrument,
  suspendInstrument,
  resumeInstrument,
  delistInstrument
} from '@/api'

export default {
  name: 'Instruments',
  data() {
    return {
      // 加载失败原因 —— 与「真的没数据」区分开,交给 $emptyText 渲染
      // @yutiansut @quantaxis
      loadError: null,
      loading: false,
      instruments: [],
      dialogVisible: false,
      isEdit: false,
      submitting: false,
      form: {
        instrument_id: '',
        instrument_name: '',
        instrument_type: 'index_future',
        exchange: 'CFFEX',
        contract_multiplier: 300,
        price_tick: 0.2,
        init_price: 3000,
        margin_rate: 0.12,
        commission_rate: 0.0001,
        limit_up_rate: 0.1,
        limit_down_rate: 0.1,
        list_date: '',
        expire_date: ''
      },
      rules: {
        instrument_id: [
          { required: true, message: '请输入合约代码', trigger: 'blur' }
        ],
        init_price: [
          { required: true, message: '请输入上市初始价', trigger: 'blur' }
        ],
        instrument_name: [
          { required: true, message: '请输入合约名称', trigger: 'blur' }
        ],
        instrument_type: [
          { required: true, message: '请选择合约类型', trigger: 'change' }
        ],
        exchange: [
          { required: true, message: '请选择交易所', trigger: 'change' }
        ]
      }
    }
  },
  computed: {
    dialogTitle() {
      return this.isEdit ? '编辑合约' : '上市新合约'
    }
  },
  mounted() {
    this.loadInstruments()
  },
  methods: {
    // 加载合约列表
    async loadInstruments() {
      this.loading = true; this.loadError = null
      try {
        // request 拦截器已解包 { success, data, error }，这里直接拿到合约数组
        const instruments = await getAllInstruments()
        this.instruments = instruments || []
      } catch (error) {
        this.loadError = this.$errMsg(error, '加载失败')
        this.$message.error('加载合约列表失败')
        console.error(error)
      } finally {
        this.loading = false
      }
    },

    // 显示创建对话框
    showCreateDialog() {
      this.isEdit = false
      this.resetForm()
      this.dialogVisible = true
    },

    // 显示编辑对话框
    showEditDialog(row) {
      this.isEdit = true
      // ⚠️ 不能整体替换 —— `/api/admin/instruments` 返回的行里**没有 init_price**
      //    (InstrumentInfo 本来就不存它),整体替换会把 data() 里的默认值冲成
      //    undefined;而 init_price 是 required 且在编辑态被 disabled,
      //    用户既填不了、校验又过不去 → 点「确定」时 validate 失败直接 return,
      //    请求根本不发出,只在输入框下方显示红字,连 toast 都没有。
      //    合并即可保住默认值。@yutiansut @quantaxis
      //
      // ✨ 但合并前必须先 resetForm() —— 否则**上一条记录的残值会漏进这一条**。
      //    实测:先编辑一条 margin_rate 异常的记录,再编辑一条只有 instrument_id
      //    的记录,四个费率格仍显示上一条的值。`resetForm()`(:345)是现成的,
      //    新增对话框(:327)一直在用,这里漏了。@yutiansut @quantaxis
      this.resetForm()
      this.form = { ...this.form, ...row }
      this.dialogVisible = true
    },

    // 重置表单
    resetForm() {
      this.form = {
        instrument_id: '',
        instrument_name: '',
        instrument_type: 'index_future',
        exchange: 'CFFEX',
        contract_multiplier: 300,
        price_tick: 0.2,
        init_price: 3000,
        margin_rate: 0.12,
        commission_rate: 0.0001,
        limit_up_rate: 0.1,
        limit_down_rate: 0.1,
        list_date: '',
        expire_date: ''
      }
      if (this.$refs.instrumentForm) {
        this.$refs.instrumentForm.clearValidate()
      }
    },

    // 提交表单
    submitForm() {
      this.$refs.instrumentForm.validate(async (valid) => {
        if (!valid) return

        this.submitting = true
        try {
          // request 拦截器已解包：失败会 reject 到 catch，走到这里即成功
          if (this.isEdit) {
            await updateInstrument(this.form.instrument_id, this.form)
          } else {
            await createInstrument(this.form)
          }

          this.$message.success(this.isEdit ? '合约更新成功' : '合约上市成功')
          this.dialogVisible = false
          this.loadInstruments()
        } catch (error) {
          this.$message.error(this.isEdit ? '合约更新失败' : '合约上市失败')
          console.error(error)
        } finally {
          this.submitting = false
        }
      })
    },

    // 暂停交易
    async suspendInstrument(row) {
      try {
        await this.$confirm(`确定要暂停 ${row.instrument_id} 的交易吗？`, '提示', {
          type: 'warning'
        })

        // request 拦截器已解包：失败会 reject 到 catch
        await suspendInstrument(row.instrument_id)
        this.$message.success('已暂停交易')
        this.loadInstruments()
      } catch (error) {
        if (error !== 'cancel') {
          this.$message.error('暂停交易失败')
          console.error(error)
        }
      }
    },

    // 恢复交易
    async resumeInstrument(row) {
      try {
        await this.$confirm(`确定要恢复 ${row.instrument_id} 的交易吗？`, '提示', {
          type: 'success'
        })

        // request 拦截器已解包：失败会 reject 到 catch
        await resumeInstrument(row.instrument_id)
        this.$message.success('已恢复交易')
        this.loadInstruments()
      } catch (error) {
        if (error !== 'cancel') {
          this.$message.error('恢复交易失败')
          console.error(error)
        }
      }
    },

    // 下市合约
    async delistInstrument(row) {
      try {
        await this.$confirm(
          `确定要下市 ${row.instrument_id} 吗？此操作不可逆！请确保该合约没有未平仓持仓。`,
          '警告',
          {
            type: 'error',
            confirmButtonText: '确定下市',
            cancelButtonText: '取消'
          }
        )

        // request 拦截器已解包：失败会 reject 到 catch
        await delistInstrument(row.instrument_id)
        this.$message.success('合约已下市')
        this.loadInstruments()
      } catch (error) {
        if (error !== 'cancel') {
          this.$message.error('合约下市失败')
          console.error(error)
        }
      }
    },

    // 获取类型标签颜色
    getTypeTagType(type) {
      const typeMap = {
        'index_future': 'primary',
        'commodity_future': 'success',
        'stock': 'info',
        'option': 'warning'
      }
      return typeMap[type] || ''
    },

    // 获取类型名称
    getTypeName(type) {
      const typeMap = {
        'index_future': '股指期货',
        'commodity_future': '商品期货',
        'stock': '股票',
        'option': '期权'
      }
      return typeMap[type] || type
    },

    // 获取状态标签颜色
    getStatusTagType(status) {
      const statusMap = {
        'active': 'success',
        'suspended': 'warning',
        'delisted': 'info'
      }
      return statusMap[status] || ''
    },

    // 获取状态名称
    getStatusName(status) {
      const statusMap = {
        'active': '正常',
        'suspended': '暂停交易',
        'delisted': '已下市'
      }
      return statusMap[status] || status
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

.instruments-container {
  padding: 20px;
  background: $dark-bg-primary;
  // ⚠️ 用 --qa-content-h,不要手写 calc(100vh - N)。@yutiansut @quantaxis
  //    原值 -60px 少算了 content-wrapper 的上下 padding(各 20px):
  //    真实 chrome = top-header 56 + padding 40 = 96px(有公告条时再 +40)。
  //    实测本页在无内容可滚时仍有 41px 的死滚动条
  //    (.content-wrapper 的 scrollHeight - clientHeight = 41)。
  //    `layout/index.vue:546` 早就写明「各页面请一律用 var(--qa-content-h)」。
  min-height: var(--qa-content-h, calc(100vh - 96px));
}

.page-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 20px;
}

.page-header h2 {
  margin: 0;
  color: $dark-text-primary !important;
}

.table-container {
  background: $dark-bg-card !important;
  border: 1px solid $dark-border;
  border-radius: 8px;
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.3);
  padding: 20px;

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

    .el-table__fixed,
    .el-table__fixed-right {
      background: $dark-bg-card !important;

      &::before {
        background-color: $dark-border !important;
      }
    }
  }

  ::v-deep .el-button--text {
    color: $primary-color !important;

    &:hover {
      color: lighten($primary-color, 15%) !important;
    }
  }
}

// 标签样式
::v-deep .el-tag {
  border: none !important;

  &.el-tag--primary {
    background: rgba($primary-color, 0.15) !important;
    color: $primary-color !important;
  }

  &.el-tag--success {
    background: rgba(82, 196, 26, 0.15) !important;
    color: #52c41a !important;
  }

  &.el-tag--warning {
    background: rgba(250, 173, 20, 0.15) !important;
    color: #faad14 !important;
  }

  &.el-tag--info {
    background: rgba($dark-text-muted, 0.15) !important;
    color: $dark-text-secondary !important;
  }
}

// 对话框样式
/* 「上市新合约」弹窗内容高 823px,默认 top:15vh —— 1366×768 下 footer
   落在折线以下 169px、1280×800 下 143px。wrapper 可滚动所以内容可达
   (不是"按钮消失"),但用户要先滚才能看到确定/取消。@yutiansut @quantaxis */
::v-deep .el-dialog {
  margin-top: 5vh !important;
}
::v-deep .el-dialog__body {
  max-height: 70vh;
  overflow-y: auto;
}
::v-deep .el-dialog {
  background: $dark-bg-card !important;
  border: 1px solid $dark-border !important;
  border-radius: 8px !important;

  .el-dialog__header {
    background: $dark-bg-secondary !important;
    border-bottom: 1px solid $dark-border !important;
    padding: 15px 20px !important;
    border-radius: 8px 8px 0 0 !important;

    .el-dialog__title {
      color: $dark-text-primary !important;
    }

    .el-dialog__headerbtn .el-dialog__close {
      color: $dark-text-secondary !important;
    }
  }

  .el-dialog__body {
    background: $dark-bg-card !important;
    padding: 20px !important;
  }

  .el-dialog__footer {
    background: $dark-bg-card !important;
    border-top: 1px solid $dark-border !important;
    padding: 15px 20px !important;
  }
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

    &:focus {
      border-color: $primary-color !important;
    }

    &[disabled] {
      background: $dark-bg-secondary !important;
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

  .el-select .el-input__inner {
    background: $dark-bg-tertiary !important;
  }
}

// 日期选择器
::v-deep .el-date-editor {
  .el-input__inner {
    background: $dark-bg-tertiary !important;
    border-color: $dark-border !important;
    color: $dark-text-primary !important;
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
}
</style>
