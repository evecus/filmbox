// cache.rs —— 极简内存 TTL 缓存（对应 瓜子.js 里的 cache 对象）

use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

static CACHE: std::sync::OnceLock<Mutex<HashMap<String, (Instant, Value)>>> = std::sync::OnceLock::new();

fn map() -> &'static Mutex<HashMap<String, (Instant, Value)>> {
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn get(key: &str) -> Option<Value> {
    let mut m = map().lock().ok()?;
    let (expire, _v) = m.get(key)?.clone();
    if Instant::now() > expire {
        m.remove(key);
        return None;
    }
    Some(m.get(key)?.1.clone())
}

pub fn set(key: &str, value: Value, ttl: Duration) {
    if let Ok(mut m) = map().lock() {
        // 超过 300 条直接清空，与 JS 行为一致
        if m.len() > 300 {
            m.clear();
        }
        m.insert(key.to_string(), (Instant::now() + ttl, value));
    }
}
