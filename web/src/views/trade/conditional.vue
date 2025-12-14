<template>
  <div class="conditional-page">
    <div class="page-header">
      <h2>条件单</h2>
      <p class="page-desc">设置止损、止盈等条件触发的订单</p>
    </div>
    <conditional-order-form />
  </div>
</template>

<script>
/**
 * 条件单页面 @yutiansut @quantaxis
 */
import ConditionalOrderForm from '@/components/ConditionalOrderForm.vue'

export default {
  name: 'ConditionalPage',

  components: {
    ConditionalOrderForm
  }
}
</script>

<style scoped>
.conditional-page {
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
