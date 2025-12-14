<template>
  <div class="transfer-page">
    <div class="page-header">
      <h2>银期转账</h2>
      <p class="page-desc">银行与期货账户之间的资金转账</p>
    </div>
    <transfer-form />
  </div>
</template>

<script>
/**
 * 银期转账页面 @yutiansut @quantaxis
 */
import TransferForm from '@/components/TransferForm.vue'

export default {
  name: 'TransferPage',

  components: {
    TransferForm
  }
}
</script>

<style scoped>
.transfer-page {
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
