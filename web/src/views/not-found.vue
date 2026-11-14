<template>
  <div class="not-found">
    <div class="nf-code">404</div>
    <div class="nf-title">页面不存在</div>
    <div class="nf-path">{{ $route.fullPath }}</div>
    <div class="nf-actions">
      <el-button type="primary" @click="$router.replace('/dashboard')">返回系统总览</el-button>
      <el-button @click="$router.back()">返回上一页</el-button>
    </div>
  </div>
</template>

<script>
/**
 * 404 兜底页 @yutiansut @quantaxis
 *
 * ⚠️ 加它的理由:路由表原本 33 条全是具体路径,**没有任何通配兜底**。
 * 访问未注册路径(实测 `/#/user/commission` —— 真实路由是 `/#/commission`)
 * 会渲染**完全空白**的页面:`tables:0, buttons:[], bodyText:""`,
 * 既没有 404 提示也没有重定向。用户看到的是「白屏」,分不清是
 * 「路径写错了」「页面挂了」还是「在加载」。
 *
 * 这里显示出实际路径,便于用户/运维直接看出是拼写问题。
 */
export default {
  name: 'NotFoundPage'
}
</script>

<style scoped lang="scss">
@import '@/styles/variables.scss';

.not-found {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  min-height: 60vh;
  padding: 40px 20px;
  text-align: center;
}

.nf-code {
  font-size: 72px;
  font-weight: bold;
  line-height: 1;
  color: #409EFF;
}

.nf-title {
  margin-top: 12px;
  font-size: 20px;
  color: $dark-text-primary;
}

.nf-path {
  margin-top: 8px;
  max-width: 100%;
  overflow-wrap: break-word;
  font-family: monospace;
  font-size: 13px;
  color: #8b949e;
}

.nf-actions {
  margin-top: 24px;
}
</style>
