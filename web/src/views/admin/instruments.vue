<template>
  <div class="instruments-container">
    <!-- 页面标题 -->
    <div class="page-header">
      <h2>合约管理</h2>
      <el-button type="primary" icon="el-icon-plus" @click="showCreateDialog">上市新合约</el-button>
    </div>

    <!-- 合约列表 -->
    <div class="table-container">
      <vxe-table
        ref="instrumentTable"
        :data="instruments"
        border
        stripe
        resizable
        highlight-hover-row
        :loading="loading"
        :sort-config="{ trigger: 'cell', remote: false }"
        height="600"
      >
        <vxe-table-column field="instrument_id" title="合约代码" width="120" sortable></vxe-table-column>
        <vxe-table-column field="instrument_name" title="合约名称" width="150"></vxe-table-column>
        <vxe-table-column field="instrument_type" title="类型" width="100" sortable>
          <template slot-scope="{ row }">
            <el-tag :type="getTypeTagType(row.instrument_type)" size="small">
              {{ getTypeName(row.instrument_type) }}
            </el-tag>
          </template>
        </vxe-table-column>
        <vxe-table-column field="exchange" title="交易所" width="100"></vxe-table-column>
        <vxe-table-column field="contract_multiplier" title="合约乘数" width="100" align="right"></vxe-table-column>
        <vxe-table-column field="price_tick" title="最小变动价位" width="120" align="right">
          <template slot-scope="{ row }">
            {{ row.price_tick.toFixed(2) }}
          </template>
        </vxe-table-column>
        <vxe-table-column field="margin_rate" title="保证金率" width="100" align="right">
          <template slot-scope="{ row }">
            {{ (row.margin_rate * 100).toFixed(1) }}%
          </template>
        </vxe-table-column>
        <vxe-table-column field="commission_rate" title="手续费率" width="100" align="right">
          <template slot-scope="{ row }">
            {{ (row.commission_rate * 100).toFixed(2) }}%
          </template>
        </vxe-table-column>
        <vxe-table-column field="status" title="状态" width="100" sortable>
          <template slot-scope="{ row }">
            <el-tag :type="getStatusTagType(row.status)" size="small">
              {{ getStatusName(row.status) }}
            </el-tag>
          </template>
        </vxe-table-column>
        <vxe-table-column field="list_date" title="上市日期" width="120"></vxe-table-column>
        <vxe-table-column field="expire_date" title="到期日期" width="120"></vxe-table-column>
        <vxe-table-column title="操作" width="200" fixed="right">
          <template slot-scope="{ row }">
            <el-button size="mini" type="text" @click="showEditDialog(row)">编辑</el-button>
            <el-button
              size="mini"
              type="text"
              v-if="row.status === 'active'"
              @click="suspendInstrument(row)"
            >
              暂停交易
            </el-button>
            <el-button
              size="mini"
              type="text"
              v-if="row.status === 'suspended'"
              @click="resumeInstrument(row)"
            >
              恢复交易
            </el-button>
            <el-button
              size="mini"
              type="text"
              style="color: #F56C6C"
              @click="delistInstrument(row)"
            >
              下市
            </el-button>
          </template>
        </vxe-table-column>
      </vxe-table>
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
      this.loading = true
      try {
        const response = await getAllInstruments()
        if (response.data && response.data.success) {
          this.instruments = response.data.data || []
        } else {
          const errorMsg = (response.data && response.data.error && response.data.error.message) || '加载合约列表失败'
          this.$message.error(errorMsg)
        }
      } catch (error) {
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
      this.form = { ...row }
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
          let response
          if (this.isEdit) {
            response = await updateInstrument(this.form.instrument_id, this.form)
          } else {
            response = await createInstrument(this.form)
          }

          if (response.data && response.data.success) {
            this.$message.success(this.isEdit ? '合约更新成功' : '合约上市成功')
            this.dialogVisible = false
            this.loadInstruments()
          } else {
            const defaultMsg = this.isEdit ? '合约更新失败' : '合约上市失败'
            const errorMsg = (response.data && response.data.error && response.data.error.message) || defaultMsg
            this.$message.error(errorMsg)
          }
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

        const response = await suspendInstrument(row.instrument_id)
        if (response.data && response.data.success) {
          this.$message.success('已暂停交易')
          this.loadInstruments()
        } else {
          const errorMsg = (response.data && response.data.error && response.data.error.message) || '暂停交易失败'
          this.$message.error(errorMsg)
        }
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

        const response = await resumeInstrument(row.instrument_id)
        if (response.data && response.data.success) {
          this.$message.success('已恢复交易')
          this.loadInstruments()
        } else {
          const errorMsg = (response.data && response.data.error && response.data.error.message) || '恢复交易失败'
          this.$message.error(errorMsg)
        }
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

        const response = await delistInstrument(row.instrument_id)
        if (response.data && response.data.success) {
          this.$message.success('合约已下市')
          this.loadInstruments()
        } else {
          const errorMsg = (response.data && response.data.error && response.data.error.message) || '合约下市失败'
          this.$message.error(errorMsg)
        }
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

<style scoped>
.instruments-container {
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

.table-container {
  background: #fff;
  border-radius: 4px;
  box-shadow: 0 2px 4px rgba(0, 0, 0, 0.1);
  padding: 20px;
}
</style>
