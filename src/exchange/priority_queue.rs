//! 优先级订单队列
//!
//! 支持三级优先级：
//! - Critical: VIP用户、大额订单
//! - Normal: 普通订单
//! - Low: 批量回测订单

use std::collections::VecDeque;
use parking_lot::Mutex;
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use super::order_router::SubmitOrderRequest;

/// 订单优先级
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum OrderPriority {
    /// 低优先级（批量回测）
    Low = 0,
    /// 普通优先级
    Normal = 1,
    /// 高优先级（VIP用户/大额订单）
    Critical = 2,
}

impl Default for OrderPriority {
    fn default() -> Self {
        OrderPriority::Normal
    }
}

/// 优先级订单请求（扩展SubmitOrderRequest）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriorityOrderRequest {
    /// 原始订单请求
    pub order: SubmitOrderRequest,
    /// 优先级
    pub priority: OrderPriority,
    /// 提交时间戳（纳秒）
    pub submit_time: i64,
}

/// 优先级订单队列
pub struct PriorityOrderQueue {
    /// 高优先级队列
    critical_queue: Arc<Mutex<VecDeque<PriorityOrderRequest>>>,
    /// 普通优先级队列
    normal_queue: Arc<Mutex<VecDeque<PriorityOrderRequest>>>,
    /// 低优先级队列
    low_queue: Arc<Mutex<VecDeque<PriorityOrderRequest>>>,

    /// 低优先级队列最大长度（防止堆积）
    low_queue_limit: usize,

    /// VIP用户列表
    vip_users: Arc<Mutex<Vec<String>>>,

    /// 大额订单阈值（金额）
    critical_amount_threshold: f64,

    /// 统计：队列长度峰值
    max_queue_length: Arc<Mutex<usize>>,
}

impl PriorityOrderQueue {
    /// 创建新的优先级队列
    ///
    /// # 参数
    /// - `low_queue_limit`: 低优先级队列最大长度（默认100）
    /// - `critical_amount_threshold`: 大额订单阈值（默认1,000,000）
    pub fn new(low_queue_limit: usize, critical_amount_threshold: f64) -> Self {
        Self {
            critical_queue: Arc::new(Mutex::new(VecDeque::with_capacity(100))),
            normal_queue: Arc::new(Mutex::new(VecDeque::with_capacity(1000))),
            low_queue: Arc::new(Mutex::new(VecDeque::with_capacity(low_queue_limit))),
            low_queue_limit,
            vip_users: Arc::new(Mutex::new(Vec::new())),
            critical_amount_threshold,
            max_queue_length: Arc::new(Mutex::new(0)),
        }
    }

    /// 添加VIP用户
    pub fn add_vip_user(&self, user_id: String) {
        self.vip_users.lock().push(user_id);
        log::info!("Added VIP user: {}", user_id);
    }

    /// 批量添加VIP用户
    pub fn add_vip_users(&self, users: Vec<String>) {
        let mut vip_list = self.vip_users.lock();
        for user_id in users {
            vip_list.push(user_id.clone());
            log::info!("Added VIP user: {}", user_id);
        }
    }

    /// 检查用户是否为VIP
    fn is_vip_user(&self, user_id: &str) -> bool {
        self.vip_users.lock().iter().any(|id| id == user_id)
    }

    /// 计算订单优先级
    fn calculate_priority(&self, req: &SubmitOrderRequest) -> OrderPriority {
        // 1. VIP用户 → Critical
        if self.is_vip_user(&req.account_id) {
            log::debug!("VIP user detected: {}", req.account_id);
            return OrderPriority::Critical;
        }

        // 2. 大额订单 → Critical
        let order_amount = req.price * req.volume;
        if order_amount >= self.critical_amount_threshold {
            log::debug!("Large order detected: amount={:.2}", order_amount);
            return OrderPriority::Critical;
        }

        // 3. 默认 → Normal
        OrderPriority::Normal
    }

    /// 入队订单
    ///
    /// # 返回
    /// - `true`: 入队成功
    /// - `false`: 队列已满（仅低优先级队列会拒绝）
    pub fn enqueue(&self, order: SubmitOrderRequest) -> bool {
        let priority = self.calculate_priority(&order);

        let req = PriorityOrderRequest {
            order,
            priority,
            submit_time: chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0),
        };

        match priority {
            OrderPriority::Critical => {
                self.critical_queue.lock().push_back(req);
                log::trace!("Enqueued CRITICAL order: {}", req.order.account_id);
                true
            }
            OrderPriority::Normal => {
                self.normal_queue.lock().push_back(req);
                log::trace!("Enqueued NORMAL order: {}", req.order.account_id);
                true
            }
            OrderPriority::Low => {
                let mut queue = self.low_queue.lock();
                if queue.len() >= self.low_queue_limit {
                    log::warn!("Low priority queue full (limit={}), rejecting order",
                        self.low_queue_limit);
                    return false;
                }
                queue.push_back(req);
                log::trace!("Enqueued LOW order: {}", req.order.account_id);
                true
            }
        }
    }

    /// 出队订单（按优先级顺序）
    ///
    /// # 调度策略
    /// 1. Critical队列优先清空
    /// 2. Normal队列次之
    /// 3. Low队列限流处理（批量大小限制）
    pub fn dequeue(&self) -> Option<PriorityOrderRequest> {
        // 1. 优先处理Critical队列
        if let Some(req) = self.critical_queue.lock().pop_front() {
            log::trace!("Dequeued CRITICAL order: {}", req.order.account_id);
            return Some(req);
        }

        // 2. 处理Normal队列
        if let Some(req) = self.normal_queue.lock().pop_front() {
            log::trace!("Dequeued NORMAL order: {}", req.order.account_id);
            return Some(req);
        }

        // 3. 限流处理Low队列（防止占用过多资源）
        let low_queue_len = self.low_queue.lock().len();
        if low_queue_len > 0 && low_queue_len < self.low_queue_limit {
            if let Some(req) = self.low_queue.lock().pop_front() {
                log::trace!("Dequeued LOW order: {}", req.order.account_id);
                return Some(req);
            }
        }

        None
    }

    /// 批量出队（最多N个订单）
    ///
    /// # 参数
    /// - `batch_size`: 批量大小（默认100）
    ///
    /// # 返回
    /// 订单列表（按优先级排序）
    pub fn dequeue_batch(&self, batch_size: usize) -> Vec<PriorityOrderRequest> {
        let mut batch = Vec::with_capacity(batch_size);

        // 1. Critical队列（全部取出）
        {
            let mut critical = self.critical_queue.lock();
            while !critical.is_empty() && batch.len() < batch_size {
                if let Some(req) = critical.pop_front() {
                    batch.push(req);
                }
            }
        }

        // 2. Normal队列
        {
            let mut normal = self.normal_queue.lock();
            while !normal.is_empty() && batch.len() < batch_size {
                if let Some(req) = normal.pop_front() {
                    batch.push(req);
                }
            }
        }

        // 3. Low队列（限流：最多批量大小的10%）
        {
            let low_limit = (batch_size / 10).max(1);
            let mut low = self.low_queue.lock();
            let mut low_count = 0;
            while !low.is_empty() && batch.len() < batch_size && low_count < low_limit {
                if let Some(req) = low.pop_front() {
                    batch.push(req);
                    low_count += 1;
                }
            }
        }

        if !batch.is_empty() {
            log::debug!("Dequeued batch: {} orders (critical/normal/low)", batch.len());
        }

        batch
    }

    /// 获取队列长度统计
    pub fn get_queue_lengths(&self) -> (usize, usize, usize) {
        let critical_len = self.critical_queue.lock().len();
        let normal_len = self.normal_queue.lock().len();
        let low_len = self.low_queue.lock().len();

        // 更新峰值
        let total = critical_len + normal_len + low_len;
        let mut max_len = self.max_queue_length.lock();
        if total > *max_len {
            *max_len = total;
        }

        (critical_len, normal_len, low_len)
    }

    /// 获取队列总长度
    pub fn total_len(&self) -> usize {
        let (c, n, l) = self.get_queue_lengths();
        c + n + l
    }

    /// 清空所有队列
    pub fn clear(&self) {
        self.critical_queue.lock().clear();
        self.normal_queue.lock().clear();
        self.low_queue.lock().clear();
        log::info!("All priority queues cleared");
    }

    /// 获取队列统计信息
    pub fn get_statistics(&self) -> PriorityQueueStatistics {
        let (critical_len, normal_len, low_len) = self.get_queue_lengths();
        PriorityQueueStatistics {
            critical_queue_length: critical_len,
            normal_queue_length: normal_len,
            low_queue_length: low_len,
            max_queue_length: *self.max_queue_length.lock(),
            vip_user_count: self.vip_users.lock().len(),
        }
    }
}

/// 优先级队列统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriorityQueueStatistics {
    pub critical_queue_length: usize,
    pub normal_queue_length: usize,
    pub low_queue_length: usize,
    pub max_queue_length: usize,
    pub vip_user_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_order(account_id: &str, price: f64, volume: f64) -> SubmitOrderRequest {
        SubmitOrderRequest {
            account_id: account_id.to_string(),
            instrument_id: "SHFE.cu2501".to_string(),
            direction: "BUY".to_string(),
            offset: "OPEN".to_string(),
            volume,
            price,
            order_type: "LIMIT".to_string(),
        }
    }

    #[test]
    fn test_priority_queue_ordering() {
        let queue = PriorityOrderQueue::new(100, 1_000_000.0);

        // 添加VIP用户
        queue.add_vip_user("vip_user".to_string());

        // 入队不同优先级订单
        queue.enqueue(create_test_order("normal_user", 100.0, 10.0));  // Normal
        queue.enqueue(create_test_order("vip_user", 100.0, 10.0));     // Critical (VIP)
        queue.enqueue(create_test_order("whale_user", 50000.0, 100.0)); // Critical (大额)

        // 验证出队顺序：Critical优先
        let order1 = queue.dequeue().unwrap();
        assert_eq!(order1.priority, OrderPriority::Critical);
        assert_eq!(order1.order.account_id, "vip_user");

        let order2 = queue.dequeue().unwrap();
        assert_eq!(order2.priority, OrderPriority::Critical);
        assert_eq!(order2.order.account_id, "whale_user");

        let order3 = queue.dequeue().unwrap();
        assert_eq!(order3.priority, OrderPriority::Normal);
        assert_eq!(order3.order.account_id, "normal_user");
    }

    #[test]
    fn test_low_queue_limit() {
        let queue = PriorityOrderQueue::new(2, 1_000_000.0);  // 限制2个

        // 低优先级订单（价格×数量 < 阈值）
        assert!(queue.enqueue(create_test_order("user1", 100.0, 10.0)));
        assert!(queue.enqueue(create_test_order("user2", 100.0, 10.0)));

        // 第3个应该被拒绝（队列已满）
        assert!(!queue.enqueue(create_test_order("user3", 100.0, 10.0)));

        let stats = queue.get_statistics();
        assert_eq!(stats.low_queue_length, 2);
    }

    #[test]
    fn test_batch_dequeue() {
        let queue = PriorityOrderQueue::new(100, 1_000_000.0);
        queue.add_vip_user("vip".to_string());

        // 入队20个订单
        for i in 0..5 {
            queue.enqueue(create_test_order("vip", 100.0, 10.0));  // Critical
        }
        for i in 0..10 {
            queue.enqueue(create_test_order(&format!("user{}", i), 100.0, 10.0));  // Normal
        }
        for i in 0..5 {
            queue.enqueue(create_test_order(&format!("low{}", i), 10.0, 10.0));  // Low
        }

        // 批量出队（最多10个）
        let batch = queue.dequeue_batch(10);
        assert_eq!(batch.len(), 10);

        // 前5个应该是Critical
        for i in 0..5 {
            assert_eq!(batch[i].priority, OrderPriority::Critical);
        }

        // 后5个应该是Normal
        for i in 5..10 {
            assert_eq!(batch[i].priority, OrderPriority::Normal);
        }
    }
}
