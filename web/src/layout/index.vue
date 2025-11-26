<template>
  <div class="layout">
    <!-- 顶部导航栏 -->
    <div class="layout-header">
      <div class="header-left">
        <h1 class="logo">
          <i class="el-icon-office-building"></i>
          QAExchange 交易所监控系统
        </h1>
      </div>
      <div class="header-center">
        <el-menu
          :default-active="activeMenu"
          mode="horizontal"
          @select="handleMenuSelect"
        >
          <!-- 交易中心 -->
          <el-submenu index="trading">
            <template slot="title">
              <i class="el-icon-sell"></i>
              交易中心
            </template>
            <el-menu-item index="/trade">交易面板</el-menu-item>
            <el-menu-item index="/chart">K线图表</el-menu-item>
            <el-menu-item index="/accounts">账户管理</el-menu-item>
            <el-menu-item index="/orders">订单管理</el-menu-item>
            <el-menu-item index="/positions">持仓管理</el-menu-item>
            <el-menu-item index="/trades">成交记录</el-menu-item>
          </el-submenu>

          <!-- 数据分析 -->
          <el-submenu index="analysis">
            <template slot="title">
              <i class="el-icon-data-analysis"></i>
              数据分析
            </template>
            <el-menu-item index="/account-curve">资金曲线</el-menu-item>
          </el-submenu>

          <!-- 系统监控 -->
          <el-menu-item index="/dashboard">
            <i class="el-icon-monitor"></i>
            系统监控
          </el-menu-item>

          <!-- 市场总览 -->
          <el-menu-item index="/market-overview">
            <i class="el-icon-view"></i>
            市场总览
          </el-menu-item>

          <!-- 管理中心 - 仅管理员可见 -->
          <el-submenu v-if="isAdmin" index="admin">
            <template slot="title">
              <i class="el-icon-s-tools"></i>
              管理中心
            </template>
            <el-menu-item index="/admin-instruments">合约管理</el-menu-item>
            <el-menu-item index="/admin-risk">风控监控</el-menu-item>
            <el-menu-item index="/admin-settlement">结算管理</el-menu-item>
            <el-menu-item index="/admin-accounts">账户管理</el-menu-item>
            <el-menu-item index="/admin-transactions">资金流水</el-menu-item>
          </el-submenu>
        </el-menu>
      </div>
      <div class="header-right">
        <el-dropdown @command="handleUserCommand">
          <span class="user-dropdown">
            <el-avatar :size="32" icon="el-icon-user-solid"></el-avatar>
            <span class="username">{{ (userInfo && userInfo.username) || currentUser }}</span>
            <el-tag v-if="isAdmin" type="danger" size="mini" style="margin-left: 8px">管理员</el-tag>
            <i class="el-icon-arrow-down el-icon--right"></i>
          </span>
          <el-dropdown-menu slot="dropdown">
            <el-dropdown-item disabled>
              <i class="el-icon-user"></i>
              用户ID: {{ currentUser }}
            </el-dropdown-item>
            <el-dropdown-item divided command="logout">
              <i class="el-icon-switch-button"></i>
              退出登录
            </el-dropdown-item>
          </el-dropdown-menu>
        </el-dropdown>
      </div>
    </div>

    <!-- 内容区域 -->
    <div class="layout-content">
      <router-view />
    </div>
  </div>
</template>

<script>
import { mapGetters, mapActions } from 'vuex'

export default {
  name: 'Layout',
  computed: {
    ...mapGetters(['currentUser', 'userInfo', 'isAdmin']),
    activeMenu() {
      return this.$route.path
    }
  },
  methods: {
    handleMenuSelect(index) {
      this.$router.push(index)
    },
    handleUserCommand(command) {
      if (command === 'logout') {
        this.$confirm('确定要退出登录吗?', '提示', {
          confirmButtonText: '确定',
          cancelButtonText: '取消',
          type: 'warning'
        }).then(() => {
          this.$store.dispatch('logout')
          this.$message.success('已退出登录')
          this.$router.push('/login')
        }).catch(() => {
          // 用户取消
        })
      }
    }
  }
}
</script>

<style lang="scss" scoped>
.layout {
  width: 100%;
  height: 100vh;
  display: flex;
  flex-direction: column;

  .layout-header {
    height: 60px;
    background: #fff;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.1);
    display: flex;
    align-items: center;
    padding: 0 20px;
    z-index: 100;

    .header-left {
      width: 300px;

      .logo {
        font-size: 18px;
        font-weight: 600;
        color: #409eff;
        margin: 0;

        i {
          margin-right: 8px;
        }
      }
    }

    .header-center {
      flex: 1;

      .el-menu {
        border: none;
      }
    }

    .header-right {
      display: flex;
      align-items: center;
      gap: 15px;

      .user-dropdown {
        display: flex;
        align-items: center;
        gap: 8px;
        cursor: pointer;
        padding: 8px 12px;
        border-radius: 4px;
        transition: background-color 0.3s;

        &:hover {
          background-color: #f0f2f5;
        }

        .username {
          font-size: 14px;
          color: #303133;
          font-weight: 500;
        }
      }
    }
  }

  .layout-content {
    flex: 1;
    overflow: auto;
    background: #f0f2f5;
  }
}
</style>
