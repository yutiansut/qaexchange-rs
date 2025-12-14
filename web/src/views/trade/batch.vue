<template>
  <div class="batch-page">
    <div class="page-header">
      <h2>批量下单</h2>
      <p class="page-desc">一次提交多笔订单，提高交易效率</p>
    </div>
    <batch-order-form />
  </div>
</template>

<script>
/**
 * 批量下单页面 @yutiansut @quantaxis
 */
import BatchOrderForm from '@/components/BatchOrderForm.vue'

export default {
  name: 'BatchPage',

  components: {
    BatchOrderForm
  }
}
</script>

<style scoped>
.batch-page {
  padding: 20px;
}

.page-header {
  margin-bottom: 20px;
}

.page-header h2 {
  margin: 0 0 8px 0;
  color: #333;
  font-size: 24px;
}

.page-desc {
  margin: 0;
  color: #999;
  font-size: 14px;
}
</style>
