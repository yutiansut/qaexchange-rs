<template>
  <div id="app">
    <router-view/>
  </div>
</template>

<script>
import { mapGetters, mapActions } from 'vuex'

export default {
  name: 'App',

  computed: {
    ...mapGetters(['isLoggedIn']),
    ...mapGetters('websocket', ['connectionState'])
  },

  watch: {
    // 监听登录状态变化
    isLoggedIn: {
      handler(newValue, oldValue) {
        if (newValue && !oldValue) {
          // 用户刚登录，初始化 WebSocket
          this.initializeWebSocket()
        } else if (!newValue && oldValue) {
          // 用户登出，销毁 WebSocket
          this.cleanupWebSocket()
        }
      },
      immediate: true
    }
  },

  mounted() {
    // 如果已登录，初始化 WebSocket
    if (this.isLoggedIn) {
      this.initializeWebSocket()
    }
  },

  beforeDestroy() {
    // 清理 WebSocket
    this.cleanupWebSocket()
  },

  methods: {
    ...mapActions('websocket', ['initWebSocket', 'connectWebSocket', 'destroyWebSocket']),

    async initializeWebSocket() {
      try {
        console.log('[App] Initializing WebSocket...')
        await this.initWebSocket()
        await this.connectWebSocket()
        console.log('[App] WebSocket initialized successfully')
      } catch (error) {
        console.error('[App] Failed to initialize WebSocket:', error)
        // 不阻塞应用启动，WebSocket 失败不应影响其他功能
      }
    },

    cleanupWebSocket() {
      console.log('[App] Cleaning up WebSocket...')
      this.destroyWebSocket()
    }
  }
}
</script>

<style lang="scss">
* {
  margin: 0;
  padding: 0;
  box-sizing: border-box;
}

body {
  font-family: 'Helvetica Neue', Helvetica, 'PingFang SC', 'Hiragino Sans GB',
    'Microsoft YaHei', '微软雅黑', Arial, sans-serif;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
}

#app {
  width: 100%;
  height: 100vh;
  overflow: hidden;
}

// 全局滚动条样式
::-webkit-scrollbar {
  width: 8px;
  height: 8px;
}

::-webkit-scrollbar-thumb {
  background-color: #ddd;
  border-radius: 4px;
}

::-webkit-scrollbar-thumb:hover {
  background-color: #ccc;
}

::-webkit-scrollbar-track {
  background-color: #f5f5f5;
}
</style>
