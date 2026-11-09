// guazi.rs —— 对齐瓜子 v1.1.0 协议（2026-07）：动态 token + 设备注册 + 新 AES/headers/多域名
// 流程：设备注册(signUp/signIn) -> refresh 取 token -> AES+RSA 加密请求 -> RSA 解密回包密钥 -> AES 解密数据

use crate::cache;
use crate::crypto;
use axum::extract::Query;
use axum::response::IntoResponse;
use rand::Rng;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::sync::OnceLock;
use std::time::Duration;

const API_HOSTS: &[&str] = &[
    "https://apinew.uozvr.com",
    "https://api.w32z7vtd.com",
    "https://api.6a7nnf7.com",
    "https://api.umygrx3.com",
    "https://api.rmedphk.com",
];

const AES_KEY: &[u8; 16] = b"OITxa5OqAYjhswxx";
const AES_IV: &[u8; 16] = b"rCMNwZASNBKZ8mXV";
const DEVICE_OLD_KEY: &str = "aLFBMWpxBrIDAD1Si/KVvm41";

const RSA_PUBLIC_KEY: &str = "-----BEGIN PUBLIC KEY-----
MIGfMA0GCSqGSIb3DQEBAQUAA4GNADCBiQKBgQDUM5+/y8sPsWkd1/RQS64X259E
UwxFXFE5HlA65MqrxnPs0JqoSRojSDy5QhwvROlaD6TwRQHKMY2OAZ6SnQeUJsCh
TEFIR9qUkwrs3/MVUMxjsv6JS6Oe/juclyJGTgVmDhB55EafXsD0SQYVj/QXXsxR
6ewR5E2kL52yAAD4yQIDAQAB
-----END PUBLIC KEY-----";

const RSA_PRIVATE_KEY: &str = "-----BEGIN PRIVATE KEY-----
MIICdgIBADANBgkqhkiG9w0BAQEFAASCAmAwggJcAgEAAoGAe6hKrWLi1zQmjTT1
ozbE4QdFeJGNxubxld6GrFGximxfMsMB6BpJhpcTouAqywAFppiKetUBBbXwYsYU
1wNr648XVmPmCMCy4rY8vdliFnbMUj086DU6Z+/oXBdWU3/b1G0DN3E9wULRSwcK
ZT3wj/cCI1vsCm3gj2R5SqkA9Y0CAwEAAQKBgAJH+4CxV0/zBVcLiBCHvSANm0l7
HetybTh/j2p0Y1sTXro4ALwAaCTUeqdBjWiLSo9lNwDHFyq8zX90+gNxa7c5EqcW
V9FmlVXr8VhfBzcZo1nXeNdXFT7tQ2yah/odtdcx+vRMSGJd1t/5k5bDd9wAvYdI
DblMAg+wiKKZ5KcdAkEA1cCakEN4NexkF5tHPRrR6XOY/XHfkqXxEhMqmNbB9U34
saTJnLWIHC8IXys6Qmzz30TtzCjuOqKRRy+FMM4TdwJBAJQZFPjsGC+RqcG5UvVM
iMPhnwe/bXEehShK86yJK/g/UiKrO87h3aEu5gcJqBygTq3BBBoH2md3pr/W+hUM
WBsCQQChfhTIrdDinKi6lRxrdBnn0Ohjg2cwuqK5zzU9p/N+S9x7Ck8wUI53DKm8
jUJE8WAG7WLj/oCOWEh+ic6NIwTdAkEAj0X8nhx6AXsgCYRql1klbqtVmL8+95KZ
K7PnLWG/IfjQUy3pPGoSaZ7fdquG8bq8oyf5+dzjE/oTXcByS+6XRQJAP/5ciy1b
L3NhUhsaOVy55MHXnPjdcTX0FaLi+ybXZIfIQ2P4rb19mVq1feMbCXhz+L1rG8oa
t5lYKfpe8k83ZA==
-----END PRIVATE KEY-----";

const PATH_INDEX_LIST: &str = "/App/IndexList/indexList";
const PATH_PLAY_INFO: &str = "/App/IndexPlay/playInfo";
const PATH_VURL_SHOW: &str = "/App/Resource/Vurl/show";
const PATH_VURL_DETAIL: &str = "/App/Resource/VurlDetail/showOne";
const PATH_FIND_MORE: &str = "/App/Index/findMoreVod";
const PATH_SIGN_UP: &str = "/App/Authentication/Device/signUp";
const PATH_SIGN_IN: &str = "/App/Authentication/Device/signIn";
const PATH_REFRESH: &str = "/App/Authentication/Authenticator/refresh";

const SUB_MAP: &[(&str, &str)] = &[("1", "5"), ("2", "12"), ("3", "30"), ("4", "22"), ("64", "")];

/// 播放流请求头（对应 playHeader）
pub fn play_header() -> Value {
    json!({"User-Agent": "Lavf/57.83.100", "Referer": "http://WJiZxLXA2.com/"})
}

fn auth_file_path() -> std::path::PathBuf {
    std::env::var("GUAZI_AUTH_FILE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir().join("guazi_auth.json"))
}

#[derive(Clone, Default)]
struct Auth {
    token: String,
    token_id: String,
    device_id: String,
    device_key: String,
    registered: bool,
    ready: bool,
}

impl Auth {
    fn from_file() -> Auth {
        let mut a = Auth::default();
        if let Ok(text) = std::fs::read_to_string(auth_file_path()) {
            if let Ok(v) = serde_json::from_str::<Value>(&text) {
                a.token = v["token"].as_str().unwrap_or("").to_string();
                a.token_id = v["tokenId"].as_str().or(v["token_id"].as_str()).unwrap_or("").to_string();
                a.device_id = v["deviceId"].as_str().or(v["device_id"].as_str()).unwrap_or("").to_string();
                a.device_key = v["deviceKey"].as_str().or(v["device_key"].as_str()).unwrap_or("").to_string();
                a.registered = v["registered"].as_bool().unwrap_or(false) || !a.token.is_empty();
            }
        }
        if a.device_id.is_empty() || a.device_key.is_empty() {
            a.device_id = (864150060000000i64 + rand::thread_rng().gen_range(0..10000)).to_string();
            let bytes: [u8; 20] = rand::thread_rng().gen();
            a.device_key = hex::encode_upper(bytes);
            a.token.clear();
            a.token_id.clear();
            a.registered = false;
            save_auth(&a);
        }
        a
    }
}

static AUTH_MEM: OnceLock<std::sync::RwLock<Auth>> = OnceLock::new();
static ENSURE_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
static API_HOST: OnceLock<String> = OnceLock::new();

fn auth_slot() -> &'static std::sync::RwLock<Auth> {
    AUTH_MEM.get_or_init(|| std::sync::RwLock::new(Auth::from_file()))
}

fn load_auth() -> Auth {
    auth_slot().read().expect("auth lock").clone()
}

fn store_auth(a: &Auth) {
    *auth_slot().write().expect("auth lock") = a.clone();
}

fn save_auth(a: &Auth) {
    let v = json!({
        "token": a.token, "tokenId": a.token_id,
        "deviceId": a.device_id, "deviceKey": a.device_key,
        "registered": a.registered,
    });
    if let Err(e) = std::fs::write(auth_file_path(), v.to_string()) {
        eprintln!("[guazi] 保存认证信息失败: {e}");
    }
}

fn http_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(22))
            .danger_accept_invalid_certs(true)
            .build()
            .expect("构建瓜子 HTTP 客户端失败")
    })
}

/// 对应 findAvailableHost：HEAD 探活，状态 < 500 即可用
async fn api_host() -> &'static str {
    if let Some(h) = API_HOST.get() {
        return h;
    }
    for host in API_HOSTS {
        let probe = reqwest::Client::builder()
            .timeout(Duration::from_secs(3))
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap();
        if let Ok(res) = probe.head(*host).header("User-Agent", "Mozilla/5.0").send().await {
            if res.status().as_u16() < 500 {
                eprintln!("[guazi] 选用 API 域名: {host}");
                let _ = API_HOST.set((*host).to_string());
                return API_HOST.get().unwrap();
            }
        }
        eprintln!("[guazi] 域名不可用: {host}");
    }
    let _ = API_HOST.set(API_HOSTS[0].to_string());
    API_HOST.get().unwrap()
}

fn now_ts() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        .to_string()
}

/// 对应 apiJsonStringify：JSON 序列化后把非 ASCII 转成 \\uXXXX（UTF-16 码元，小写）
fn api_json_stringify(v: &Value) -> String {
    let s = serde_json::to_string(v).unwrap_or_default();
    let mut out = String::with_capacity(s.len() + 64);
    for c in s.chars() {
        if (c as u32) < 0x80 {
            out.push(c);
        } else {
            let mut buf = [0u16; 2];
            for u in c.encode_utf16(&mut buf) {
                out.push_str(&format!("\\u{:04x}", u));
            }
        }
    }
    out
}

/// 对应 rsaEncrypt：RSA 公钥 PKCS#1 加密，输出 base64
fn rsa_encrypt_pub(data: &[u8]) -> anyhow::Result<String> {
    use rsa::pkcs8::DecodePublicKey;
    use rsa::traits::PaddingScheme;
    let mut rng = rand::thread_rng();
    let key = rsa::RsaPublicKey::from_public_key_pem(RSA_PUBLIC_KEY)
        .map_err(|e| anyhow::anyhow!("解析RSA公钥失败: {e}"))?;
    let ct = rsa::Pkcs1v15Encrypt.encrypt(&mut rng, &key, data)?;
    use base64::Engine as _;
    Ok(base64::engine::general_purpose::STANDARD.encode(ct))
}

/// 对应 getHeaders
fn get_headers(auth: &Auth) -> reqwest::header::HeaderMap {
    use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
    let mut h = HeaderMap::new();
    let mut set = |k: &'static str, v: String| {
        h.insert(HeaderName::from_static(k), HeaderValue::from_str(&v).unwrap());
    };
    set("user-agent", "Lavf/57.83.100".into());
    set("code", "GZ0369".into());
    set("deviceid", auth.device_id.clone());
    set("lang", "zh_cn".into());
    set("cache-control", "no-cache".into());
    set("content-type", "application/x-www-form-urlencoded".into());
    set("version", "2604028".into());
    set("packagename", "com.ae06aebdbb.y286327f5a.ofe849883320260517".into());
    set("ver", "3.0.3.2".into());
    set("api-ver", "3.0.3.2".into());
    h
}

fn truncate(s: &str) -> String {
    s.chars().take(150).collect()
}

/// 对应 rawApiRequest：加密请求 -> 解密回包（含业务码重试与 token 刷新）
fn raw_api_request<'a>(path: &'a str, params: &'a Value, retry: u32) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<Value>> + Send + 'a>> {
    Box::pin(async move {
    let is_auth_path = path.starts_with("/App/Authentication/");
    if !is_auth_path {
        ensure_token(false).await?;
    }
    let auth = load_auth();
    let host = api_host().await;

    // 参数中的 token/token_id 替换为当前动态值
    let mut request_params: Map<String, Value> = params.as_object().cloned().unwrap_or_default();
    if request_params.contains_key("token") {
        request_params.insert("token".into(), json!(auth.token));
    }
    if request_params.contains_key("token_id") {
        request_params.insert("token_id".into(), json!(auth.token_id));
    }

    let json_params = api_json_stringify(&Value::Object(request_params));
    let request_key = crypto::aes_encrypt_hex(&json_params, AES_KEY, AES_IV)?;
    let keys_b64 = rsa_encrypt_pub(api_json_stringify(&json!({"iv": "rCMNwZASNBKZ8mXV", "key": "OITxa5OqAYjhswxx"})).as_bytes())?;
    let time = now_ts();
    let sign_str = format!(
        "token_id=,token={token},phone_type=1,request_key={request_key},app_id=1,time={time},keys={keys}*&zvdvdvddbfikkkumtmdwqppp?|4Y!s!2br",
        token = auth.token,
        keys = keys_b64,
    );
    let signature = crypto::md5_hex(&sign_str).to_uppercase();

    let body = format!(
        "token={token}&token_id=&phone_type=1&time={time}&phone_model=xiaomi-25031&keys={keys}&request_key={request_key}&signature={signature}&app_id=1&ad_version=1",
        token = urlencoding::encode(&auth.token),
        keys = urlencoding::encode(&keys_b64),
        request_key = urlencoding::encode(&request_key),
    );

    eprintln!("[guazi] {host}{path} 发送请求 (retry={retry}) params={}", truncate(&json_params));
    let t0 = std::time::Instant::now();
    let resp = http_client()
        .post(format!("{host}{path}"))
        .headers(get_headers(&auth))
        .body(body)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("网络错误: {e}"))?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    eprintln!("[guazi] {path} http={status} {}ms body_len={}", t0.elapsed().as_millis(), text.len());

    if !status.is_success() {
        anyhow::bail!("HTTP {status} {path}");
    }
    let response: Value = serde_json::from_str(&text)
        .map_err(|e| anyhow::anyhow!("响应非JSON: {e} | 片段: {}", truncate(&text)))?;

    if let Some(code) = response.get("code").and_then(Value::as_i64) {
        if code != 200 {
            if retry < 1 && !is_auth_path {
                eprintln!("[guazi] {path} 业务错误 code={code}，强制刷新 token 重试");
                ensure_token(true).await?;
                return raw_api_request(path, params, retry + 1).await;
            }
            anyhow::bail!("请求失败 code={code} msg={}",
                response.get("msg").and_then(Value::as_str).unwrap_or("?"));
        }
    }

    let enc = response.get("data").cloned().unwrap_or(Value::Null);
    let keys_b64_resp = enc["keys"].as_str().ok_or_else(|| anyhow::anyhow!("回包缺少 keys: {}", truncate(&text)))?;
    let keys_json = crypto::rsa_no_padding_decrypt(RSA_PRIVATE_KEY, keys_b64_resp)
        .map_err(|e| anyhow::anyhow!("RSA解密失败: {e}"))?;
    let key_info: Value = serde_json::from_str(&keys_json)?;
    let key = key_info["key"].as_str().ok_or_else(|| anyhow::anyhow!("keys.key 缺失"))?;
    let iv = key_info["iv"].as_str().ok_or_else(|| anyhow::anyhow!("keys.iv 缺失"))?;
    let response_key = enc["response_key"].as_str().ok_or_else(|| anyhow::anyhow!("回包缺少 response_key: {}", truncate(&text)))?;
    let decrypted = crypto::aes_decrypt_hex(response_key, key.as_bytes(), iv.as_bytes())
        .map_err(|e| anyhow::anyhow!("AES解密失败: {e}"))?;
    eprintln!("[guazi] {path} 解密成功 len={} 预览={}", decrypted.len(), truncate(&decrypted));
    let result: Value = serde_json::from_str(&decrypted)
        .map_err(|e| anyhow::anyhow!("解密后JSON解析失败: {e} | 预览: {}", truncate(&decrypted)))?;
    Ok(result)
    })
}

fn apply_auth(auth: &mut Auth, result: &Value) -> anyhow::Result<()> {
    let new_token = result["token"].as_str().unwrap_or("").to_string();
    if new_token.is_empty() {
        anyhow::bail!("Token 获取失败: {}", truncate(&result.to_string()));
    }
    auth.token = new_token;
    // app_user_id / token_id 兼容数字与字符串
    let tid = match result.get("app_user_id").or_else(|| result.get("token_id")) {
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::String(s)) => s.clone(),
        _ => String::new(),
    };
    if !tid.is_empty() {
        auth.token_id = tid;
    }
    Ok(())
}

async fn sign_up(auth: &mut Auth) -> anyhow::Result<()> {
    let result = raw_api_request(
        PATH_SIGN_UP,
        &json!({"new_key": auth.device_key, "old_key": DEVICE_OLD_KEY, "phone_type": 1, "code": ""}),
        0,
    ).await?;
    apply_auth(auth, &result)?;
    auth.registered = true;
    eprintln!("[guazi] 设备注册成功 token_id={}", auth.token_id);
    Ok(())
}

async fn sign_in(auth: &mut Auth) -> anyhow::Result<()> {
    let result = raw_api_request(
        PATH_SIGN_IN,
        &json!({"new_key": auth.device_key, "old_key": DEVICE_OLD_KEY}),
        0,
    ).await?;
    apply_auth(auth, &result)?;
    eprintln!("[guazi] 设备登录成功 token_id={}", auth.token_id);
    Ok(())
}

/// 对应 ensureToken：串行化，必要时注册/登录并 refresh
pub async fn ensure_token(force: bool) -> anyhow::Result<()> {
    let lock = ENSURE_LOCK.get_or_init(|| tokio::sync::Mutex::new(()));
    let _guard = lock.lock().await;
    let mut auth = Auth::from_file();
    if !force && auth.ready && !auth.token.is_empty() {
        return Ok(());
    }
    if auth.token.is_empty() {
        if auth.registered {
            sign_in(&mut auth).await?;
        } else {
            sign_up(&mut auth).await?;
        }
    }
    match raw_api_request(PATH_REFRESH, &json!({}), 0).await {
        Ok(r) => apply_auth(&mut auth, &r)?,
        Err(e) => {
            eprintln!("[guazi] refresh 失败: {e}");
            if !auth.registered {
                anyhow::bail!("refresh 失败且未注册: {e}");
            }
            sign_in(&mut auth).await?;
        }
    }
    auth.ready = true;
    save_auth(&auth);
    store_auth(&auth);
    Ok(())
}

/// 对应 apiRequest（兼容层）：带缓存
async fn api_request(path: &str, data: &Value, cache_ttl: Option<Duration>) -> anyhow::Result<Value> {
    let body_json = serde_json::to_string(data)?;
    if let Some(_ttl) = cache_ttl {
        if let Some(v) = cache::get(&format!("guazi:{path}|{body_json}")) {
            return Ok(v);
        }
    }
    let result = raw_api_request(path, data, 0).await?;
    if let Some(ttl) = cache_ttl {
        cache::set(&format!("guazi:{path}|{body_json}"), result.clone(), ttl);
    }
    Ok(result)
}

fn get_resolution_score(res: &str) -> i32 {
    let r = res.to_lowercase().replace('p', "");
    match r.as_str() {
        "8k" => 100,
        "4k" | "2160" => 90,
        "1440" => 80,
        "1080" => 70,
        "720" => 60,
        "超清" => 50,
        "高清" => 40,
        "标清" => 30,
        _ => 10,
    }
}

/// 数字字段兼容数字与字符串类型
fn num_flex(v: &Value) -> i64 {
    match v {
        Value::Number(n) => n.as_i64().unwrap_or(0),
        Value::String(s) => s.trim().parse().unwrap_or(0),
        _ => 0,
    }
}

fn to_item(item: &Value) -> Value {
    let continu = num_flex(&item["vod_continu"]);
    // vod_id 兼容数字与字符串两种类型
    let vid = match &item["vod_id"] {
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        _ => String::new(),
    };
    json!({
        "vod_id": format!("{vid}/{}", continu),
        "vod_name": item["vod_name"].as_str().unwrap_or(""),
        "vod_pic": item["vod_pic"].as_str().unwrap_or(""),
        "vod_remarks": if continu == 0 { "电影".to_string() } else { format!("更新至{continu}集") },
    })
}

fn vod_id_part(vod_id: &str) -> String {
    let first = vod_id.split('/').next().unwrap_or(vod_id).trim();
    if !first.is_empty() {
        return first.to_string();
    }
    vod_id.chars().filter(|c| c.is_ascii_digit()).collect()
}

fn list_to_value(list: &Value) -> Vec<Value> {
    list.as_array().map(|a| a.iter().map(to_item).collect()).unwrap_or_default()
}

/// 分类列表（IndexList，含 sub 子分类映射）
pub async fn index_list(tid: &str, page: i64, area: &str, year: &str, sort: &str) -> anyhow::Result<Value> {
    let sub = SUB_MAP.iter().find(|(k, _)| *k == tid).map(|(_, v)| v.to_string()).unwrap_or_else(|| "0".into());
    let data = api_request(
        PATH_INDEX_LIST,
        &json!({"area": area, "year": year, "pageSize": "20", "sort": sort, "page": page.to_string(), "tid": tid, "sub": sub}),
        Some(Duration::from_secs(300)),
    ).await?;
    let total_page = num_flex(&data["totalPage"]);
    Ok(json!({
        "list": list_to_value(&data["list"]),
        "page": page,
        "pagecount": if total_page == 0 { 999 } else { total_page },
    }))
}

/// HTTP: /api/guazi/category
pub async fn category(Query(q): Query<BTreeMap<String, String>>) -> impl IntoResponse {
    let tid = q.get("tid").cloned().unwrap_or_else(|| "1".into());
    let page = q.get("page").and_then(|s| s.parse::<i64>().ok()).unwrap_or(1);
    let area = q.get("area").cloned().unwrap_or_else(|| "0".into());
    let year = q.get("year").cloned().unwrap_or_else(|| "0".into());
    let sort = q.get("sort").cloned().unwrap_or_else(|| "d_id".into());
    axum::Json(index_list(&tid, page, &area, &year, &sort).await.unwrap_or_else(|e| {
        json!({"list": [], "page": page, "pagecount": 0, "error": e.to_string()})
    }))
}

/// 搜索（findMoreVod + 标题精确过滤）
pub async fn search_inner(wd: &str, page: i64) -> anyhow::Result<Value> {
    let data = api_request(
        PATH_FIND_MORE,
        &json!({"keywords": wd, "order_val": "1", "page": page.to_string()}),
        Some(Duration::from_secs(60)),
    ).await?;
    let kw = wd.to_lowercase();
    let list: Vec<Value> = data["list"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter(|it| it["vod_name"].as_str().map(|n| n.to_lowercase().contains(&kw)).unwrap_or(false))
                .map(to_item)
                .collect()
        })
        .unwrap_or_default();
    let total_page = num_flex(&data["totalPage"]);
    Ok(json!({
        "list": list,
        "page": page,
        "pagecount": if total_page == 0 { 1 } else { total_page },
    }))
}

/// HTTP: /api/guazi/search
pub async fn search(Query(q): Query<BTreeMap<String, String>>) -> impl IntoResponse {
    let wd = q.get("wd").cloned().unwrap_or_default();
    let page = q.get("page").and_then(|s| s.parse::<i64>().ok()).unwrap_or(1);
    if wd.is_empty() {
        return axum::Json(json!({"list": [], "page": 1, "pagecount": 0}));
    }
    axum::Json(search_inner(&wd, page).await.unwrap_or_else(|e| {
        json!({"list": [], "page": page, "pagecount": 0, "error": e.to_string()})
    }))
}

/// HTTP: /api/guazi/detail?vod_id=xxx
pub async fn detail(Query(q): Query<BTreeMap<String, String>>) -> impl IntoResponse {
    let vod_id_raw = q.get("vod_id").cloned().unwrap_or_default();
    let vod_id = vod_id_part(&vod_id_raw);
    if vod_id.is_empty() {
        return axum::Json(json!({"error": "vod_id 不能为空"}));
    }

    let auth = load_auth();
    let detail_body = json!({
        "token_id": auth.token_id,
        "vod_id": vod_id,
        "mobile_time": now_ts(),
        "token": auth.token,
    });
    let play_body = json!({"vurl_cloud_id": "2", "vod_d_id": vod_id});
    let (detail_res, play_res) = tokio::join!(
        api_request(PATH_PLAY_INFO, &detail_body, Some(Duration::from_secs(600))),
        api_request(PATH_VURL_SHOW, &play_body, Some(Duration::from_secs(600))),
    );

    let detail_err = detail_res.as_ref().err().map(|e| e.to_string());

    // 降级策略：playInfo 失败也能用 Vurl/show 的剧集数据继续播放
    let vod = detail_res
        .as_ref()
        .ok()
        .map(|d| d["vodInfo"].clone())
        .filter(|v| !v.is_null())
        .unwrap_or_else(|| json!({"vod_name": "", "vod_id": vod_id}));

    let mut episodes: Vec<Value> = Vec::new();
    let mut play_err: Option<String> = None;
    match &play_res {
        Ok(play_data) => {
            if let Some(lines) = play_data["list"].as_array() {
                for (idx, line) in lines.iter().enumerate() {
                    let play = &line["play"];
                    let Some(play_obj) = play.as_object() else { continue };
                    // 诊断日志：打印每条线路 play 对象的真实结构（分辨率键、param 长度）
                    let struct_info: Vec<String> = play_obj
                        .iter()
                        .map(|(k, v)| {
                            let plen = v["param"].as_str().map(|s| s.len()).unwrap_or_else(|| if v["param"].is_null() { 0 } else { 999 });
                            format!("{k}:param_len={plen}")
                        })
                        .collect();
                    eprintln!(
                        "[guazi] Vurl/show 行{idx} name={:?} pay_playable={} play=[{}]",
                        line["name"].as_str().unwrap_or(""),
                        line["pay_playable"].as_str().unwrap_or("-"),
                        struct_info.join(", ")
                    );
                    // 对应 JS 真值检查 if (val.param)：空串/缺失都跳过
                    let mut entries: Vec<(&String, &str)> = play_obj
                        .iter()
                        .filter_map(|(k, v)| v["param"].as_str().filter(|s| !s.is_empty()).map(|p| (k, p)))
                        .collect();
                    if entries.is_empty() {
                        continue;
                    }
                    entries.sort_by(|a, b| get_resolution_score(b.0).cmp(&get_resolution_score(a.0)));
                    let param = entries[0].1.to_string();
                    let name = if lines.len() == 1 {
                        vod["vod_name"].as_str().filter(|s| !s.is_empty()).unwrap_or("正片").to_string()
                    } else {
                        line["name"].as_str().unwrap_or("").to_string()
                    };
                    let res_list: Vec<String> = entries.iter().map(|(k, _)| k.to_string()).collect();
                    episodes.push(json!({"name": name, "param": param, "res": res_list}));
                }
            }
        }
        Err(e) => {
            play_err = Some(e.to_string());
        }
    }

    if episodes.is_empty() {
        let mut err = json!({"error": "未获取到可播放的剧集"});
        if let Some(e) = &detail_err { err["detail_error"] = json!(e); }
        if let Some(e) = &play_err { err["play_error"] = json!(e); }
        // 附带原始数据样本便于诊断（截断）
        if let Ok(play_data) = &play_res {
            let sample = play_data["list"].get(0).map(|l| truncate(&l.to_string())).unwrap_or_default();
            err["raw_sample"] = json!(sample);
        }
        return axum::Json(err);
    }

    axum::Json(json!({
        "vod": {
            "vod_id": vod_id,
            "vod_name": vod["vod_name"].as_str().unwrap_or(""),
            "vod_pic": vod["vod_pic"].as_str().unwrap_or(""),
            "vod_year": vod["vod_year"].as_str().unwrap_or(""),
            "vod_area": vod["vod_area"].as_str().unwrap_or(""),
            "vod_actor": vod["vod_actor"].as_str().unwrap_or(""),
            "vod_director": vod["vod_director"].as_str().unwrap_or(""),
            "vod_content": vod["vod_use_content"].as_str().or(vod["vod_content"].as_str()).unwrap_or("").trim(),
        },
        "episodes": episodes,
        "warnings": {"detail": detail_err, "play": play_err},
    }))
}

/// 对应 parseParamString：? 去除、&amp; 还原、百分号解码（+ 号转空格）、vod_d_id -> vod_id
fn parse_param_string(value: &str) -> BTreeMap<String, String> {
    let text = value.trim().replace("&amp;", "&");
    let text = text.strip_prefix('?').unwrap_or(&text);
    let mut out = BTreeMap::new();
    for pair in text.split('&') {
        if pair.is_empty() {
            continue;
        }
        let mut it = pair.splitn(2, '=');
        let k = it.next().unwrap_or("");
        let v = it.next().unwrap_or("");
        if k.is_empty() {
            continue;
        }
        let dec = |s: &str| -> String {
            urlencoding::decode(&s.replace('+', " ")).map(|c| c.into_owned()).unwrap_or_else(|_| s.to_string())
        };
        let key = dec(k);
        let key = if key == "vod_d_id" { "vod_id".to_string() } else { key };
        out.insert(key, dec(v));
    }
    out
}

/// HTTP: /api/guazi/play?param=...&res=1080
pub async fn play(Query(q): Query<BTreeMap<String, String>>) -> impl IntoResponse {
    let param = q.get("param").cloned().unwrap_or_default();
    let res = q.get("res").cloned().unwrap_or_default();
    if param.is_empty() {
        return axum::Json(json!({"error": "param 不能为空"}));
    }

    let mut request_params: Map<String, Value> = parse_param_string(&param)
        .into_iter()
        .map(|(k, v)| (k, Value::String(v)))
        .collect();
    if !res.is_empty() {
        request_params.insert("resolution".to_string(), Value::String(res));
    }

    match raw_api_request(PATH_VURL_DETAIL, &Value::Object(request_params), 0).await {
        Ok(data) => axum::Json(json!({
            "url": data["url"].as_str().unwrap_or(""),
            "header": play_header(),
        })),
        Err(e) => axum::Json(json!({"error": format!("取播放地址失败: {e}")})),
    }
}

/// HTTP: /api/guazi/debug?path=...&data={...} —— 诊断用
pub async fn debug(Query(q): Query<BTreeMap<String, String>>) -> impl IntoResponse {
    let path = q.get("path").cloned().unwrap_or_default();
    let data: Value = q.get("data").and_then(|s| serde_json::from_str(s).ok()).unwrap_or(json!({}));
    if !path.starts_with('/') {
        return axum::Json(json!({"error": "path 需以 / 开头", "hosts": API_HOSTS}));
    }
    eprintln!("[guazi-debug] path={path} data={data}");
    match raw_api_request(&path, &data, 0).await {
        Ok(v) => {
            eprintln!("[guazi-debug] 成功: {}", truncate(&v.to_string()));
            axum::Json(v)
        }
        Err(e) => {
            eprintln!("[guazi-debug] 失败: {e}");
            axum::Json(json!({"error": e.to_string()}))
        }
    }
}
