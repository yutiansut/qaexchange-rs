//! 启动交易所示例

use qaexchange::exchange::AccountManager;
use qaexchange::core::account_ext::{OpenAccountRequest, AccountType};

fn main() {
    env_logger::init();

    println!("=== QAEXCHANGE Demo ===\n");

    // 创建账户管理器
    let account_mgr = AccountManager::new();

    // 开户
    let req = OpenAccountRequest {
        user_id: "demo_user".to_string(),
        user_name: "Demo User".to_string(),
        init_cash: 1000000.0,
        account_type: AccountType::Individual,
        password: "demo123".to_string(),
    };

    match account_mgr.open_account(req) {
        Ok(user_id) => {
            println!("✓ Account opened: {}", user_id);

            // 查询账户
            if let Ok(qifi) = account_mgr.get_account_qifi(&user_id) {
                println!("  Balance: {}", qifi.balance);
                println!("  Available: {}", qifi.available);
            }
        }
        Err(e) => {
            println!("✗ Failed to open account: {}", e);
        }
    }

    println!("\nDemo completed.");
}
