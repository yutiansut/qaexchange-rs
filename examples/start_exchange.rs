//! 启动交易所示例

use qaexchange::core::account_ext::{AccountType, OpenAccountRequest};
use qaexchange::exchange::AccountManager;

fn main() {
    env_logger::init();

    println!("=== QAEXCHANGE Demo ===\n");

    // 创建账户管理器
    let account_mgr = AccountManager::new();

    // 开户 (Phase 10: 使用user_id和account_id分离)
    let req = OpenAccountRequest {
        user_id: "demo_user".to_string(),              // 用户ID（所有者）
        account_id: None,                              // 自动生成账户ID
        account_name: "Demo User Account".to_string(), // 账户名称
        init_cash: 1000000.0,
        account_type: AccountType::Individual,
    };

    match account_mgr.open_account(req) {
        Ok(account_id) => {
            println!("✓ Account opened!");
            println!("  Account ID: {}", account_id);
            println!("  User ID: demo_user");

            // 查询账户（使用account_id）
            if let Ok(qifi) = account_mgr.get_account_qifi(&account_id) {
                println!("  Balance: {}", qifi.balance);
                println!("  Available: {}", qifi.available);
            }

            // 也可以通过user_id查询默认账户
            if let Ok(account) = account_mgr.get_default_account("demo_user") {
                let acc = account.read();
                println!("\n通过user_id查询默认账户:");
                println!("  Balance: {}", acc.accounts.balance);
                println!("  Available: {}", acc.accounts.available);
            }
        }
        Err(e) => {
            println!("✗ Failed to open account: {}", e);
        }
    }

    println!("\nDemo completed.");
}
