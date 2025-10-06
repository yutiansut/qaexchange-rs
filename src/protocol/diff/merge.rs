//! JSON Merge Patch 实现
//!
//! 基于 RFC 7386 (JSON Merge Patch) 标准实现差分更新算法
//!
//! # 核心功能
//!
//! - `merge_patch`: 将单个 patch 应用到 target JSON 对象
//! - `apply_patches`: 批量应用多个 patch（按顺序）
//!
//! # RFC 7386 规则
//!
//! 1. 如果 patch 是 null，删除 target 中对应的键
//! 2. 如果 patch 和 target 都是对象，递归合并每个键
//! 3. 否则，用 patch 的值替换 target 的值
//!
//! # 示例
//!
//! ```rust
//! use serde_json::json;
//! use qaexchange::protocol::diff::merge::merge_patch;
//!
//! let mut target = json!({
//!     "user_id": "user123",
//!     "balance": 100000.0,
//!     "available": 95000.0
//! });
//!
//! let patch = json!({
//!     "balance": 105000.0,    // 更新
//!     "available": 100000.0,  // 更新
//!     "margin_used": 5000.0   // 新增
//! });
//!
//! merge_patch(&mut target, &patch);
//!
//! assert_eq!(target["balance"], 105000.0);
//! assert_eq!(target["available"], 100000.0);
//! assert_eq!(target["margin_used"], 5000.0);
//! ```

use serde_json::Value;

/// 将 JSON Merge Patch 应用到目标对象
///
/// 实现 RFC 7386 标准的 merge patch 算法。
///
/// # 参数
///
/// * `target` - 目标 JSON 对象（会被原地修改）
/// * `patch` - 要应用的 patch 对象
///
/// # 规则
///
/// 1. 如果 `patch` 不是对象，直接替换 `target`
/// 2. 如果 `target` 不是对象，用空对象替换并继续
/// 3. 对于 `patch` 中的每个键：
///    - 值为 `null`：删除 `target` 中对应的键
///    - 值为对象：递归合并
///    - 其他：替换 `target` 中的值
///
/// # 示例
///
/// ```rust
/// use serde_json::json;
/// use qaexchange::protocol::diff::merge::merge_patch;
///
/// // 示例 1: 基本更新
/// let mut target = json!({"a": 1, "b": 2});
/// let patch = json!({"b": 3, "c": 4});
/// merge_patch(&mut target, &patch);
/// assert_eq!(target, json!({"a": 1, "b": 3, "c": 4}));
///
/// // 示例 2: 删除字段
/// let mut target = json!({"a": 1, "b": 2});
/// let patch = json!({"b": null});
/// merge_patch(&mut target, &patch);
/// assert_eq!(target, json!({"a": 1}));
///
/// // 示例 3: 嵌套对象合并
/// let mut target = json!({"user": {"name": "Alice", "age": 30}});
/// let patch = json!({"user": {"age": 31, "city": "Beijing"}});
/// merge_patch(&mut target, &patch);
/// assert_eq!(target, json!({"user": {"name": "Alice", "age": 31, "city": "Beijing"}}));
/// ```
pub fn merge_patch(target: &mut Value, patch: &Value) {
    // 规则 1: 如果 patch 不是对象，直接替换
    if !patch.is_object() {
        *target = patch.clone();
        return;
    }

    // 规则 2: 如果 target 不是对象，替换为空对象
    if !target.is_object() {
        *target = Value::Object(serde_json::Map::new());
    }

    // 规则 3: 递归合并对象
    let patch_obj = patch.as_object().unwrap();
    let target_obj = target.as_object_mut().unwrap();

    for (key, value) in patch_obj {
        if value.is_null() {
            // 删除键
            target_obj.remove(key);
        } else if value.is_object() {
            // 如果 patch 值是对象，需要递归合并
            if !target_obj.contains_key(key) {
                // target 中不存在该键，先创建空对象
                target_obj.insert(key.clone(), Value::Object(serde_json::Map::new()));
            }

            let target_value = target_obj.get_mut(key).unwrap();
            // 递归合并（即使 target_value 不是对象，也会在递归中被处理）
            merge_patch(target_value, value);
        } else {
            // 替换或新增（非对象值）
            target_obj.insert(key.clone(), value.clone());
        }
    }
}

/// 批量应用多个 JSON Merge Patch（按顺序）
///
/// 将多个 patch 按顺序应用到目标对象，常用于差分推送场景。
///
/// # 参数
///
/// * `snapshot` - 业务快照（会被原地修改）
/// * `patches` - patch 数组（按时间顺序）
///
/// # 示例
///
/// ```rust
/// use serde_json::json;
/// use qaexchange::protocol::diff::merge::apply_patches;
///
/// let mut snapshot = json!({
///     "trade": {
///         "user123": {
///             "accounts": {
///                 "ACC001": {"balance": 100000.0}
///             }
///         }
///     }
/// });
///
/// let patches = vec![
///     json!({"trade": {"user123": {"accounts": {"ACC001": {"balance": 105000.0}}}}}),
///     json!({"trade": {"user123": {"accounts": {"ACC001": {"available": 100000.0}}}}}),
/// ];
///
/// apply_patches(&mut snapshot, patches);
///
/// assert_eq!(snapshot["trade"]["user123"]["accounts"]["ACC001"]["balance"], 105000.0);
/// assert_eq!(snapshot["trade"]["user123"]["accounts"]["ACC001"]["available"], 100000.0);
/// ```
pub fn apply_patches(snapshot: &mut Value, patches: Vec<Value>) {
    for patch in patches {
        merge_patch(snapshot, &patch);
    }
}

/// 计算两个 JSON 对象的差异（生成 Merge Patch）
///
/// 生成从 `original` 到 `updated` 的最小 merge patch。
///
/// # 参数
///
/// * `original` - 原始对象
/// * `updated` - 更新后的对象
///
/// # 返回
///
/// 符合 RFC 7386 的 merge patch 对象
///
/// # 示例
///
/// ```rust
/// use serde_json::json;
/// use qaexchange::protocol::diff::merge::create_patch;
///
/// let original = json!({"a": 1, "b": 2, "c": 3});
/// let updated = json!({"a": 1, "b": 99, "d": 4});
///
/// let patch = create_patch(&original, &updated);
///
/// // patch 应该包含: b 的更新, c 的删除, d 的新增
/// assert_eq!(patch["b"], 99);
/// assert_eq!(patch["c"], json!(null));
/// assert_eq!(patch["d"], 4);
/// ```
pub fn create_patch(original: &Value, updated: &Value) -> Value {
    // 如果不是对象，直接返回 updated
    if !original.is_object() || !updated.is_object() {
        return updated.clone();
    }

    let mut patch = serde_json::Map::new();
    let original_obj = original.as_object().unwrap();
    let updated_obj = updated.as_object().unwrap();

    // 找出被删除或修改的键
    for (key, original_value) in original_obj {
        if let Some(updated_value) = updated_obj.get(key) {
            // 键存在：检查是否有变化
            if original_value != updated_value {
                if original_value.is_object() && updated_value.is_object() {
                    // 递归计算嵌套对象的 patch
                    let nested_patch = create_patch(original_value, updated_value);
                    if !nested_patch.as_object().unwrap().is_empty() {
                        patch.insert(key.clone(), nested_patch);
                    }
                } else {
                    // 值类型不同或非对象，直接替换
                    patch.insert(key.clone(), updated_value.clone());
                }
            }
        } else {
            // 键被删除
            patch.insert(key.clone(), Value::Null);
        }
    }

    // 找出新增的键
    for (key, updated_value) in updated_obj {
        if !original_obj.contains_key(key) {
            patch.insert(key.clone(), updated_value.clone());
        }
    }

    Value::Object(patch)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_merge_patch_basic_update() {
        let mut target = json!({"a": 1, "b": 2});
        let patch = json!({"b": 3, "c": 4});
        merge_patch(&mut target, &patch);
        assert_eq!(target, json!({"a": 1, "b": 3, "c": 4}));
    }

    #[test]
    fn test_merge_patch_delete_field() {
        let mut target = json!({"a": 1, "b": 2, "c": 3});
        let patch = json!({"b": null});
        merge_patch(&mut target, &patch);
        assert_eq!(target, json!({"a": 1, "c": 3}));
    }

    #[test]
    fn test_merge_patch_nested_object() {
        let mut target = json!({
            "user": {
                "name": "Alice",
                "age": 30
            }
        });
        let patch = json!({
            "user": {
                "age": 31,
                "city": "Beijing"
            }
        });
        merge_patch(&mut target, &patch);
        assert_eq!(
            target,
            json!({
                "user": {
                    "name": "Alice",
                    "age": 31,
                    "city": "Beijing"
                }
            })
        );
    }

    #[test]
    fn test_merge_patch_replace_non_object() {
        let mut target = json!({"a": "string"});
        let patch = json!({"a": {"nested": "object"}});
        merge_patch(&mut target, &patch);
        assert_eq!(target, json!({"a": {"nested": "object"}}));
    }

    #[test]
    fn test_merge_patch_empty_patch() {
        let mut target = json!({"a": 1, "b": 2});
        let patch = json!({});
        merge_patch(&mut target, &patch);
        assert_eq!(target, json!({"a": 1, "b": 2}));
    }

    #[test]
    fn test_merge_patch_replace_target() {
        let mut target = json!("old_value");
        let patch = json!({"new": "object"});
        merge_patch(&mut target, &patch);
        assert_eq!(target, json!({"new": "object"}));
    }

    #[test]
    fn test_apply_patches_sequential() {
        let mut snapshot = json!({
            "trade": {
                "user123": {
                    "accounts": {
                        "ACC001": {
                            "balance": 100000.0
                        }
                    }
                }
            }
        });

        let patches = vec![
            json!({"trade": {"user123": {"accounts": {"ACC001": {"balance": 105000.0}}}}}),
            json!({"trade": {"user123": {"accounts": {"ACC001": {"available": 100000.0}}}}}),
            json!({"trade": {"user123": {"accounts": {"ACC001": {"margin_used": 5000.0}}}}}),
        ];

        apply_patches(&mut snapshot, patches);

        assert_eq!(
            snapshot["trade"]["user123"]["accounts"]["ACC001"]["balance"],
            105000.0
        );
        assert_eq!(
            snapshot["trade"]["user123"]["accounts"]["ACC001"]["available"],
            100000.0
        );
        assert_eq!(
            snapshot["trade"]["user123"]["accounts"]["ACC001"]["margin_used"],
            5000.0
        );
    }

    #[test]
    fn test_apply_patches_delete_and_add() {
        let mut snapshot = json!({
            "orders": {
                "order1": {"status": "alive"},
                "order2": {"status": "alive"}
            }
        });

        let patches = vec![
            json!({"orders": {"order1": {"status": "finished"}}}),
            json!({"orders": {"order2": null}}), // 删除 order2
            json!({"orders": {"order3": {"status": "alive"}}}), // 新增 order3
        ];

        apply_patches(&mut snapshot, patches);

        assert_eq!(snapshot["orders"]["order1"]["status"], "finished");
        assert_eq!(snapshot["orders"].get("order2"), None);
        assert_eq!(snapshot["orders"]["order3"]["status"], "alive");
    }

    #[test]
    fn test_create_patch_basic() {
        let original = json!({"a": 1, "b": 2, "c": 3});
        let updated = json!({"a": 1, "b": 99, "d": 4});

        let patch = create_patch(&original, &updated);

        assert_eq!(patch.get("a"), None); // a 未变化，不包含在 patch 中
        assert_eq!(patch["b"], 99);
        assert_eq!(patch["c"], json!(null));
        assert_eq!(patch["d"], 4);
    }

    #[test]
    fn test_create_patch_nested() {
        let original = json!({
            "user": {
                "name": "Alice",
                "age": 30,
                "address": {
                    "city": "Beijing",
                    "zip": "100000"
                }
            }
        });

        let updated = json!({
            "user": {
                "name": "Alice",
                "age": 31,
                "address": {
                    "city": "Shanghai",
                    "zip": "200000"
                },
                "phone": "12345678"
            }
        });

        let patch = create_patch(&original, &updated);

        assert_eq!(patch["user"]["age"], 31);
        assert_eq!(patch["user"]["phone"], "12345678");
        assert_eq!(patch["user"]["address"]["city"], "Shanghai");
        assert_eq!(patch["user"]["address"]["zip"], "200000");
    }

    #[test]
    fn test_create_patch_empty() {
        let original = json!({"a": 1, "b": 2});
        let updated = json!({"a": 1, "b": 2});

        let patch = create_patch(&original, &updated);

        assert_eq!(patch, json!({}));
    }

    #[test]
    fn test_roundtrip_patch() {
        let original = json!({
            "user_id": "user123",
            "balance": 100000.0,
            "available": 95000.0
        });

        let updated = json!({
            "user_id": "user123",
            "balance": 105000.0,
            "available": 100000.0,
            "margin_used": 5000.0
        });

        // 创建 patch
        let patch = create_patch(&original, &updated);

        // 应用 patch
        let mut result = original.clone();
        merge_patch(&mut result, &patch);

        // 验证结果等于 updated
        assert_eq!(result, updated);
    }

    #[test]
    fn test_rfc7386_example_1() {
        // RFC 7386 示例 1
        let mut target = json!({"a": "b"});
        let patch = json!({"a": "c"});
        merge_patch(&mut target, &patch);
        assert_eq!(target, json!({"a": "c"}));
    }

    #[test]
    fn test_rfc7386_example_2() {
        // RFC 7386 示例 2
        let mut target = json!({"a": "b"});
        let patch = json!({"b": "c"});
        merge_patch(&mut target, &patch);
        assert_eq!(target, json!({"a": "b", "b": "c"}));
    }

    #[test]
    fn test_rfc7386_example_3() {
        // RFC 7386 示例 3
        let mut target = json!({"a": "b"});
        let patch = json!({"a": null});
        merge_patch(&mut target, &patch);
        assert_eq!(target, json!({}));
    }

    #[test]
    fn test_rfc7386_example_4() {
        // RFC 7386 示例 4
        let mut target = json!({"a": "b", "b": "c"});
        let patch = json!({"a": null});
        merge_patch(&mut target, &patch);
        assert_eq!(target, json!({"b": "c"}));
    }

    #[test]
    fn test_rfc7386_example_5() {
        // RFC 7386 示例 5
        let mut target = json!({"a": ["b"]});
        let patch = json!({"a": "c"});
        merge_patch(&mut target, &patch);
        assert_eq!(target, json!({"a": "c"}));
    }

    #[test]
    fn test_rfc7386_example_6() {
        // RFC 7386 示例 6
        let mut target = json!({"a": "c"});
        let patch = json!({"a": ["b"]});
        merge_patch(&mut target, &patch);
        assert_eq!(target, json!({"a": ["b"]}));
    }

    #[test]
    fn test_rfc7386_example_7() {
        // RFC 7386 示例 7
        let mut target = json!({"a": {"b": "c"}});
        let patch = json!({"a": {"b": "d", "c": null}});
        merge_patch(&mut target, &patch);
        assert_eq!(target, json!({"a": {"b": "d"}}));
    }

    #[test]
    fn test_rfc7386_example_8() {
        // RFC 7386 示例 8
        let mut target = json!({"a": [{"b": "c"}]});
        let patch = json!({"a": [1]});
        merge_patch(&mut target, &patch);
        assert_eq!(target, json!({"a": [1]}));
    }

    #[test]
    fn test_rfc7386_example_9() {
        // RFC 7386 示例 9
        let mut target = json!(["a", "b"]);
        let patch = json!(["c", "d"]);
        merge_patch(&mut target, &patch);
        assert_eq!(target, json!(["c", "d"]));
    }

    #[test]
    fn test_rfc7386_example_10() {
        // RFC 7386 示例 10
        let mut target = json!({"a": "b"});
        let patch = json!(["c"]);
        merge_patch(&mut target, &patch);
        assert_eq!(target, json!(["c"]));
    }

    #[test]
    fn test_rfc7386_example_11() {
        // RFC 7386 示例 11
        let mut target = json!({"a": "foo"});
        let patch = json!(null);
        merge_patch(&mut target, &patch);
        assert_eq!(target, json!(null));
    }

    #[test]
    fn test_rfc7386_example_12() {
        // RFC 7386 示例 12
        let mut target = json!({"a": "foo"});
        let patch = json!("bar");
        merge_patch(&mut target, &patch);
        assert_eq!(target, json!("bar"));
    }

    #[test]
    fn test_rfc7386_example_13() {
        // RFC 7386 示例 13
        let mut target = json!({"e": null});
        let patch = json!({"a": 1});
        merge_patch(&mut target, &patch);
        assert_eq!(target, json!({"e": null, "a": 1}));
    }

    #[test]
    fn test_rfc7386_example_14() {
        // RFC 7386 示例 14
        let mut target = json!([1, 2]);
        let patch = json!({"a": "b", "c": null});
        merge_patch(&mut target, &patch);
        assert_eq!(target, json!({"a": "b"}));
    }

    #[test]
    fn test_rfc7386_example_15() {
        // RFC 7386 示例 15
        let mut target = json!({});
        let patch = json!({"a": {"bb": {"ccc": null}}});
        merge_patch(&mut target, &patch);
        assert_eq!(target, json!({"a": {"bb": {}}}));
    }
}
