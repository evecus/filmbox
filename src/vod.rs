// vod.rs —— 多源统一模块：如意资源（CMS采集）/ 热播（APP接口）/ 荐片（APP接口）
// 对应 OmniBox 脚本：如意资源.js v1.0.2 / 热播.js v1.0.3 / 荐片.js v1.1.1
// 统一输出格式：{ vod_id, vod_name, vod_pic, vod_remarks, source }
// 剧集统一携带不透明 q 参数，由 /api/vod/play 按源分发解析

use anyhow::anyhow;
use base64::Engine as _;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

use crate::crypto;
use crate::guazi;

const BROWSER_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36";

pub fn source_label(src: &str) -> &'static str {
    match src {
        "guazi" => "瓜子",
        "ruyi" => "如意",
        "rebang" => "热播",
        "jianpian" => "荐片",
        _ => "未知",
    }
}

fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap_or_default()
}

fn truncate(s: &str) -> String {
    if s.len() <= 200 { s.to_string() } else { format!("{}…", &s[..200.min(s.len())]) }
}

fn b64_encode(data: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(data)
}

fn b64_decode(text: &str) -> anyhow::Result<Vec<u8>> {
    Ok(base64::engine::general_purpose::STANDARD.decode(text)?)
}

// ============================================================================
// 如意资源（MacCMS 标准采集接口）
// ============================================================================

const RUYI_APIS: [&str; 3] = [
    "https://cj.rycjapi.com/api.php/provide/vod",
    "https://cj.rytvapi.com/api.php/provide/vod",
    "https://bycj.rytvapi.com/api.php/provide/vod",
];
const RUYI_IMG_HOST: &str = "https://ps.ryzypics.com";

fn ruyi_pic(path: &str) -> String {
    let p = path.trim();
    if p.is_empty() || p == "<nil>" || p == "nil" || p == "null" {
        return String::new();
    }
    if p.starts_with("http://") || p.starts_with("https://") {
        return p.to_string();
    }
    if p.starts_with('/') {
        return format!("{RUYI_IMG_HOST}{p}");
    }
    p.to_string()
}

async fn ruyi_request(params: &str) -> anyhow::Result<Value> {
    let client = http_client();
    let mut last_err = String::new();
    for api in RUYI_APIS {
        let url = format!("{api}?{params}");
        let resp = client
            .get(&url)
            .header("User-Agent", BROWSER_UA)
            .header("Accept", "application/json")
            .header("Referer", "https://cj.rycjapi.com/")
            .send()
            .await;
        match resp {
            Ok(r) => {
                let status = r.status();
                let text = r.text().await.unwrap_or_default();
                if status.is_success() {
                    match serde_json::from_str::<Value>(&text) {
                        Ok(v) => return Ok(v),
                        Err(e) => last_err = format!("JSON解析失败: {e}"),
                    }
                } else {
                    last_err = format!("HTTP {status}");
                }
                eprintln!("[ruyi] {url} 失败: {last_err}");
            }
            Err(e) => {
                eprintln!("[ruyi] {url} 失败: {e}");
                last_err = e.to_string();
            }
        }
    }
    Err(anyhow!("如意所有API均失败: {last_err}"))
}

/// 批量补全封面（对应 enrichCovers/batchGetCovers，ac=videolist&ids=...）
async fn ruyi_enrich_covers(list: &mut [Value]) {
    let need: Vec<String> = list
        .iter()
        .filter(|i| i["vod_pic"].as_str().unwrap_or("").is_empty())
        .filter_map(|i| i["vod_id"].as_str().map(String::from))
        .collect();
    if need.is_empty() {
        return;
    }
    for batch in need.chunks(20) {
        let params = format!("ac=videolist&ids={}", batch.join(","));
        if let Ok(res) = ruyi_request(&params).await {
            if let Some(items) = res["list"].as_array() {
                for item in items {
                    let id = item["vod_id"].to_string();
                    let pic = ruyi_pic(
                        item["vod_pic"].as_str()
                            .filter(|s| !s.is_empty() && *s != "<nil>")
                            .or_else(|| item["vod_img"].as_str())
                            .or_else(|| item["pic"].as_str())
                            .unwrap_or(""),
                    );
                    if pic.is_empty() {
                        continue;
                    }
                    for v in list.iter_mut() {
                        if v["vod_id"].as_str() == Some(id.as_str()) && v["vod_pic"].as_str().unwrap_or("").is_empty() {
                            v["vod_pic"] = json!(pic);
                        }
                    }
                }
            }
        }
    }
}

fn ruyi_format_list(raw: &Value) -> Vec<Value> {
    raw.as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter(|i| !i["vod_id"].to_string().is_empty() && i["vod_id"].as_i64() != Some(0))
        .map(|i| {
            json!({
                "vod_id": i["vod_id"].to_string(),
                "vod_name": i["vod_name"].as_str().unwrap_or("未知标题"),
                "vod_pic": ruyi_pic(i["vod_pic"].as_str().unwrap_or("")),
                "vod_remarks": i["vod_remarks"].as_str().or(i["vod_year"].as_str()).unwrap_or(""),
            })
        })
        .collect()
}

pub async fn ruyi_search(wd: &str, page: i64) -> anyhow::Result<Value> {
    let res = ruyi_request(&format!("ac=list&wd={}&pg={page}&pagesize=30", urlencoding::encode(wd))).await?;
    let pagecount = res["pagecount"].as_i64().unwrap_or(1).max(1);
    let mut list = ruyi_format_list(&res["list"]);
    let kw = wd.trim().to_lowercase();
    list.retain(|i| i["vod_name"].as_str().unwrap_or("").to_lowercase().contains(&kw));
    list.truncate(20);
    ruyi_enrich_covers(&mut list).await;
    for v in list.iter_mut() {
        v["source"] = json!("ruyi");
    }
    Ok(json!({"list": list, "page": page, "pagecount": pagecount}))
}

pub async fn ruyi_detail(id: &str) -> anyhow::Result<Value> {
    let res = ruyi_request(&format!("ac=videolist&ids={}", urlencoding::encode(id))).await?;
    let item = res["list"]
        .get(0)
        .cloned()
        .ok_or_else(|| anyhow!("如意详情为空"))?;

    // vod_play_url: "名称$url#名称$url" ；多线路（vod_play_from 逗号分隔）共用同一组地址
    let mut episodes: Vec<Value> = Vec::new();
    let play_url = item["vod_play_url"].as_str().unwrap_or("");
    for (idx, seg) in play_url.split('#').enumerate() {
        let mut it = seg.splitn(2, '$');
        let name = it.next().unwrap_or("");
        let url = it.next().unwrap_or("");
        if url.is_empty() {
            continue;
        }
        let ep_name = if name.is_empty() { format!("第{}集", idx + 1) } else { name.to_string() };
        episodes.push(json!({"name": ep_name, "q": url, "res": []}));
    }
    if episodes.is_empty() {
        return Err(anyhow!("如意详情无剧集数据"));
    }

    Ok(json!({
        "vod": {
            "vod_id": id,
            "vod_name": item["vod_name"].as_str().unwrap_or(""),
            "vod_pic": ruyi_pic(item["vod_pic"].as_str().unwrap_or("")),
            "vod_year": item["vod_year"].as_str().unwrap_or(""),
            "vod_area": item["vod_area"].as_str().unwrap_or(""),
            "vod_actor": item["vod_actor"].as_str().unwrap_or(""),
            "vod_director": item["vod_director"].as_str().unwrap_or(""),
            "vod_content": item["vod_content"].as_str().unwrap_or("").trim(),
        },
        "episodes": episodes,
        "source": "ruyi",
    }))
}

/// 如意直连播放：地址本身就是 m3u8/mp4，无特殊请求头
pub fn ruyi_play(url: &str) -> Value {
    json!({"url": url, "header": {}})
}

// ============================================================================
// 热播（APP 接口，multipart/form-data + MD5 签名）
// ============================================================================

const REBANG_UA: &str = "okhttp-okgo/jeasonlzy";
const REBANG_BOUNDARY: &str = "18a7affc-a82a-4dc2-a848-7b0658d7665c";
const REBANG_SIGN_SECRET: &str = "7gp0bnd2sr85ydii2j32pcypscoc4w6c7g5spl";
/// 热播 API 域名列表（可用环境变量 REBANG_HOST 覆盖/追加，逗号分隔）
const REBANG_HOSTS: [&str; 3] = ["http://v.rbotv.cn", "https://v.rbotv.cn", "http://tt.ysdqjs.cn"];

static REBANG_HOST_OK: std::sync::OnceLock<String> = std::sync::OnceLock::new();

fn rebang_hosts() -> Vec<String> {
    if let Some(h) = REBANG_HOST_OK.get() {
        return vec![h.clone()];
    }
    match std::env::var("REBANG_HOST") {
        Ok(v) if !v.trim().is_empty() => v.split(',').map(|s| s.trim().trim_end_matches('/').to_string()).filter(|s| !s.is_empty()).collect(),
        _ => REBANG_HOSTS.iter().map(|s| s.to_string()).collect(),
    }
}

async fn rebang_post(path: &str, params: &[(&str, &str)]) -> anyhow::Result<Value> {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs();
    let timestamp = now.to_string();
    let sign = crypto::md5_hex(&format!("{REBANG_SIGN_SECRET}{timestamp}"));

    let mut body = String::new();
    for (k, v) in params {
        body += &format!("--{REBANG_BOUNDARY}\r\nContent-Disposition: form-data; name=\"{k}\"\r\n\r\n{v}\r\n");
    }
    body += &format!("--{REBANG_BOUNDARY}\r\nContent-Disposition: form-data; name=\"sign\"\r\n\r\n{sign}\r\n");
    body += &format!("--{REBANG_BOUNDARY}\r\nContent-Disposition: form-data; name=\"timestamp\"\r\n\r\n{timestamp}\r\n");
    body += &format!("--{REBANG_BOUNDARY}--\r\n");

    let client = http_client();
    let mut last_err = String::new();
    for host in rebang_hosts() {
        let url = format!("{host}{path}");
        eprintln!("[rebang] POST {url} params={params:?}");
        let r = client
            .post(&url)
            .header("User-Agent", REBANG_UA)
            .header("Accept-Language", "zh-CN,zh;q=0.8")
            .header("Referer", &host)
            .header("Content-Type", format!("multipart/form-data; boundary={REBANG_BOUNDARY}"))
            .body(body.clone())
            .send()
            .await;
        let resp = match r {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[rebang] {host} 连接失败: {e}");
                last_err = e.to_string();
                continue;
            }
        };
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            eprintln!("[rebang] {host}{path} HTTP {status}");
            last_err = format!("HTTP {status}: {}", truncate(&text));
            continue;
        }
        match serde_json::from_str(&text) {
            Ok(v) => {
                let _ = REBANG_HOST_OK.set(host);
                return Ok(v);
            }
            Err(e) => {
                last_err = format!("JSON解析失败: {e}");
            }
        }
    }
    anyhow::bail!("热播所有域名均失败: {last_err}")
}

fn rebang_format_list(raw: &Value) -> Vec<Value> {
    raw.as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter(|i| i["vod_id"].as_i64() != Some(0) && !i["vod_id"].to_string().is_empty())
        .map(|i| {
            json!({
                "vod_id": i["vod_id"].to_string(),
                "vod_name": i["vod_name"].as_str().unwrap_or(""),
                "vod_pic": i["vod_pic"].as_str().or(i["vod_pic_thumb"].as_str()).unwrap_or(""),
                "vod_remarks": i["vod_remarks"].as_str().unwrap_or(""),
            })
        })
        .collect()
}

pub async fn rebang_search(wd: &str, page: i64) -> anyhow::Result<Value> {
    let data = rebang_post(
        "/v3/home/search",
        &[("limit", "12"), ("page", &page.to_string()), ("keyword", wd)],
    )
    .await?;
    let list = rebang_format_list(&data["data"]["list"]);
    Ok(json!({"list": list, "page": page, "pagecount": 9999}))
}

pub async fn rebang_detail(id: &str) -> anyhow::Result<Value> {
    let data = rebang_post("/v3/home/vod_details", &[("vod_id", id)]).await?;
    let v = &data["data"];
    if v.is_null() {
        anyhow::bail!("热播详情为空");
    }

    // vod_play_list: [{flag, urls:[{name,url}], parse_urls, referer, ua}]
    let mut episodes: Vec<Value> = Vec::new();
    let lines = v["vod_play_list"].as_array().cloned().unwrap_or_default();
    let multi = lines.len() > 1;
    for (li, line) in lines.iter().enumerate() {
        let parse_urls = line["parse_urls"].clone();
        let referer = line["referer"].as_str().unwrap_or("");
        let ua = line["ua"].as_str().unwrap_or("");
        for (ei, item) in line["urls"].as_array().cloned().unwrap_or_default().iter().enumerate() {
            let url = item["url"].as_str().unwrap_or("");
            if url.is_empty() {
                continue;
            }
            let raw_name = item["name"].as_str().unwrap_or("").to_string();
            let ep_name = if raw_name.is_empty() { format!("第{}集", ei + 1) } else { raw_name };
            let name = if multi {
                format!("【线路{}】{}", li + 1, ep_name)
            } else {
                ep_name
            };
            // playId = Base64(JSON{url, p, r, u})，对应 convertVodPlayListToSources
            let meta = json!({
                "url": url,
                "p": parse_urls,
                "r": referer,
                "u": ua,
            });
            let q = b64_encode(meta.to_string().as_bytes());
            episodes.push(json!({"name": name, "q": q, "res": []}));
        }
    }
    if episodes.is_empty() {
        anyhow::bail!("热播详情无剧集数据");
    }

    Ok(json!({
        "vod": {
            "vod_id": id,
            "vod_name": v["vod_name"].as_str().unwrap_or(""),
            "vod_pic": v["vod_pic"].as_str().or(v["vod_pic_thumb"].as_str()).unwrap_or(""),
            "vod_year": v["vod_year"].as_str().or(v["tag"].as_str()).unwrap_or(""),
            "vod_area": v["vod_area"].as_str().unwrap_or(""),
            "vod_actor": v["vod_actor"].as_str().unwrap_or(""),
            "vod_director": v["vod_director"].as_str().unwrap_or(""),
            "vod_content": v["vod_content"].as_str().unwrap_or(""),
        },
        "episodes": episodes,
        "source": "rebang",
    }))
}

/// 热播播放：q = Base64(JSON{url,p,r,u})；有解析接口时先调解析，否则直连
pub async fn rebang_play(q: &str) -> anyhow::Result<Value> {
    let decoded = b64_decode(q)?;
    let meta: Value = serde_json::from_slice(&decoded)?;
    let url = meta["url"].as_str().unwrap_or("").to_string();
    if url.is_empty() {
        anyhow::bail!("热播播放数据缺少 url");
    }

    let mut headers = Map::new();
    if let Some(r) = meta["r"].as_str().filter(|s| !s.is_empty()) {
        headers.insert("Referer".into(), json!(r));
    }
    if let Some(u) = meta["u"].as_str().filter(|s| !s.is_empty()) {
        headers.insert("User-Agent".into(), json!(u));
    }

    let parse_urls = meta["p"].as_array().cloned().unwrap_or_default();
    if let Some(p0) = parse_urls.first().and_then(Value::as_str).filter(|s| !s.is_empty()) {
        // 解析接口：GET p[0] + urlencode(url) → {url} 或 {data:{url}}，可带 UA 覆盖
        let parse_url = format!("{}{}", p0, urlencoding::encode(&url));
        eprintln!("[rebang] 调解析接口: {parse_url}");
        let client = http_client();
        let mut req = client.get(&parse_url);
        if let Some(Value::String(ua)) = headers.get("User-Agent") {
            req = req.header("User-Agent", ua);
        }
        if let Some(Value::String(r)) = headers.get("Referer") {
            req = req.header("Referer", r);
        }
        let resp = req.send().await?;
        let text = resp.text().await?;
        let parsed: Value = serde_json::from_str(text.trim()).map_err(|e| anyhow!("解析接口返回非JSON: {e} | {}", truncate(&text)))?;
        let real = parsed["url"].as_str()
            .or(parsed["data"]["url"].as_str())
            .unwrap_or("");
        if real.is_empty() {
            anyhow::bail!("解析接口未返回播放地址: {}", truncate(&parsed.to_string()));
        }
        if let Some(ua) = parsed["UA"].as_str().filter(|s| !s.is_empty()) {
            headers.insert("User-Agent".into(), json!(ua));
        }
        eprintln!("[rebang] 解析成功: {}", truncate(real));
        return Ok(json!({"url": real, "header": headers}));
    }

    Ok(json!({"url": url, "header": headers}))
}

// ============================================================================
// 荐片（APP H5 接口）
// ============================================================================

const JP_HOST: &str = "https://h5.jianpianips1.com";
const JP_IMG_HOST: &str = "https://img.ztfgh.com";
/// 荐片 API 域名列表（主域名常被境外封锁/频繁更换；可用环境变量 JP_HOST 覆盖，逗号分隔）
const JP_HOSTS: [&str; 3] = [
    "https://h5.jianpianips1.com",
    "https://h5.jianpianips2.com",
    "https://h5.jianpianips3.com",
];

static JP_HOST_OK: std::sync::OnceLock<String> = std::sync::OnceLock::new();

fn jp_hosts() -> Vec<String> {
    if let Some(h) = JP_HOST_OK.get() {
        return vec![h.clone()];
    }
    match std::env::var("JP_HOST") {
        Ok(v) if !v.trim().is_empty() => v.split(',').map(|s| s.trim().trim_end_matches('/').to_string()).filter(|s| !s.is_empty()).collect(),
        _ => JP_HOSTS.iter().map(|s| s.to_string()).collect(),
    }
}

fn jp_headers(host: &str) -> reqwest::header::HeaderMap {
    use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
    let mut h = HeaderMap::new();
    h.insert("User-Agent", HeaderValue::from_static(BROWSER_UA));
    h.insert("Accept", HeaderValue::from_static("application/json, text/plain, */*"));
    h.insert("Accept-Language", HeaderValue::from_static("zh-CN,zh;q=0.9"));
    h.insert(HeaderName::from_static("origin"), HeaderValue::from_str(host).unwrap_or(HeaderValue::from_static(JP_HOST)));
    h.insert("Referer", HeaderValue::from_str(&format!("{host}/")).unwrap_or(HeaderValue::from_static("https://h5.jianpianips1.com/")));
    h
}

fn jp_pic(path: &str) -> String {
    let p = path.trim();
    if p.is_empty() {
        return String::new();
    }
    if p.starts_with("http://") || p.starts_with("https://") {
        return p.to_string();
    }
    if p.starts_with('/') {
        format!("{JP_IMG_HOST}{p}")
    } else {
        format!("{JP_IMG_HOST}/{p}")
    }
}

async fn jp_request(path: &str) -> anyhow::Result<Value> {
    let client = http_client();
    let mut last_err = String::new();
    for host in jp_hosts() {
        let url = format!("{host}{path}");
        let resp = client.get(&url).headers(jp_headers(&host)).send().await;
        let r = match resp {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[jianpian] {host} 连接失败: {e}");
                last_err = e.to_string();
                continue;
            }
        };
        let status = r.status();
        let text = r.text().await.unwrap_or_default();
        if !status.is_success() {
            eprintln!("[jianpian] {host}{path} HTTP {status}");
            last_err = format!("HTTP {status}: {}", truncate(&text));
            continue;
        }
        match serde_json::from_str::<Value>(&text) {
            Ok(v) => {
                if v["code"].as_i64() != Some(1) {
                    // 业务错误：域名连通正常，直接返回不再换域名
                    let _ = JP_HOST_OK.set(host);
                    anyhow::bail!("荐片业务错误: {}", v["msg"].as_str().unwrap_or("未知"));
                }
                let _ = JP_HOST_OK.set(host);
                return Ok(v);
            }
            Err(e) => {
                last_err = format!("JSON解析失败: {e}");
            }
        }
    }
    anyhow::bail!("荐片所有域名均失败: {last_err}")
}

fn jp_format_list(raw: &Value) -> Vec<Value> {
    raw.as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter(|i| !i["id"].to_string().is_empty() && i["id"].as_i64() != Some(0))
        .map(|i| {
            let pic = jp_pic(i["path"].as_str().or(i["tvimg"].as_str()).or(i["tagimg"].as_str()).unwrap_or(""));
            let remarks = i["mask"].as_str().map(String::from).unwrap_or_default();
            json!({
                "vod_id": i["id"].to_string(),
                "vod_name": i["title"].as_str().unwrap_or("未知标题"),
                "vod_pic": pic,
                "vod_remarks": if remarks.is_empty() {
                    i["score"].as_str().map(|s| format!("评分:{s}")).unwrap_or_default()
                } else { remarks },
            })
        })
        .collect()
}

pub async fn jianpian_search(wd: &str, page: i64) -> anyhow::Result<Value> {
    // 原脚本把 "keyword=xx&page=N" 整体作为 key 参数值，此处保持一致
    let path = format!("/api/v2/search/videoV2?key=keyword={}&page={page}", urlencoding::encode(wd));
    let res = jp_request(&path).await?;
    let mut list = jp_format_list(&res["data"]);
    let kw = wd.trim().to_lowercase();
    list.retain(|i| i["vod_name"].as_str().unwrap_or("").to_lowercase().contains(&kw));
    for v in list.iter_mut() {
        v["source"] = json!("jianpian");
    }
    Ok(json!({"list": list, "page": page, "pagecount": 1}))
}

pub async fn jianpian_detail(id: &str) -> anyhow::Result<Value> {
    let res = jp_request(&format!("/api/video/detailv2?id={}", urlencoding::encode(id))).await?;
    let v = &res["data"];
    if v.is_null() {
        anyhow::bail!("荐片详情为空");
    }

    let pic = jp_pic(v["tvimg"].as_str().or(v["thumbnail"].as_str()).or(v["path"].as_str()).unwrap_or(""));
    let actors = v["actors"]
        .as_array()
        .map(|a| a.iter().filter_map(|x| x["name"].as_str()).collect::<Vec<_>>().join(" "))
        .unwrap_or_default();

    // source_list_source: [{name, source_list:[{source_name,url}]}]；屏蔽 VIP/FTP/常规 线路
    let mut episodes: Vec<Value> = Vec::new();
    for src in v["source_list_source"].as_array().cloned().unwrap_or_default() {
        let line_name = src["name"].as_str().unwrap_or("");
        let lower = line_name.to_lowercase();
        if lower.contains("vip") || lower.contains("ftp") || line_name == "常规线路" || line_name == "常规" {
            eprintln!("[jianpian] 屏蔽线路: {line_name}");
            continue;
        }
        for item in src["source_list"].as_array().cloned().unwrap_or_default() {
            let url = item["url"].as_str().unwrap_or("");
            if url.is_empty() {
                continue;
            }
            let name = item["source_name"].as_str().unwrap_or("").to_string();
            let name = if name.is_empty() { format!("第{}集", episodes.len() + 1) } else { name };
            // playId = urlencode(url) + "|||" + Base64(JSON{sid,fid,v,e})，对应 convertToPlaySources
            let meta = json!({"sid": id, "fid": format!("{id}#{line_name}#{}", episodes.len()), "v": v["title"].as_str().unwrap_or(""), "e": name});
            let q = format!("{}|||{}", urlencoding::encode(url), b64_encode(meta.to_string().as_bytes()));
            episodes.push(json!({"name": name, "q": q, "res": []}));
        }
    }
    if episodes.is_empty() {
        anyhow::bail!("荐片详情无可用剧集（线路可能均被屏蔽）");
    }

    Ok(json!({
        "vod": {
            "vod_id": id,
            "vod_name": v["title"].as_str().unwrap_or(""),
            "vod_pic": pic,
            "vod_year": v["year"].as_str().unwrap_or(""),
            "vod_area": v["area"].as_str().unwrap_or(""),
            "vod_actor": actors,
            "vod_director": "",
            "vod_content": v["description"].as_str().unwrap_or(""),
        },
        "episodes": episodes,
        "source": "jianpian",
    }))
}

/// 荐片播放：q = urlencode(url)|||Base64(meta)；直连可播格式直接给头，否则原样返回由前端尝试
pub fn jianpian_play(q: &str) -> anyhow::Result<Value> {
    let (main, _) = q.split_once("|||").unwrap_or((q, ""));
    let url = urlencoding::decode(main).map(|c| c.into_owned()).unwrap_or_else(|_| main.to_string());
    if url.is_empty() {
        anyhow::bail!("荐片播放地址为空");
    }
    let header = json!({"User-Agent": BROWSER_UA, "Referer": JP_HOST, "Origin": JP_HOST});
    Ok(json!({"url": url, "header": header}))
}

// ============================================================================
// 统一入口：聚合搜索 / 详情分发 / 播放分发
// ============================================================================

/// 聚合搜索：src = guazi|ruyi|rebang|jianpian|all
pub async fn unified_search(src: &str, wd: &str, page: i64) -> Value {
    if src != "all" {
        let res = match src {
            "guazi" => guazi::search_inner(wd, page).await,
            "ruyi" => ruyi_search(wd, page).await,
            "rebang" => rebang_search(wd, page).await,
            "jianpian" => jianpian_search(wd, page).await,
            _ => Err(anyhow!("未知源: {src}")),
        };
        return match res {
            Ok(mut v) => {
                for item in v["list"].as_array_mut().unwrap_or(&mut vec![]) {
                    if item["source"].is_null() {
                        item["source"] = json!(src);
                    }
                }
                json!({"ok": true, "list": v["list"], "page": v["page"], "pagecount": v["pagecount"]})
            }
            Err(e) => json!({"ok": false, "error": format!("{}搜索失败: {e}", source_label(src)), "list": []}),
        };
    }

    // src=all：四源并发聚合
    let (g, r, rb, jp): (anyhow::Result<Value>, anyhow::Result<Value>, anyhow::Result<Value>, anyhow::Result<Value>) = tokio::join!(
        guazi::search_inner(wd, page),
        ruyi_search(wd, page),
        rebang_search(wd, page),
        jianpian_search(wd, page),
    );
    let mut merged: Vec<Value> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    for (label, res) in [("guazi", g), ("ruyi", r), ("rebang", rb), ("jianpian", jp)] {
        match res {
            Ok(v) => {
                for mut item in v["list"].as_array().cloned().unwrap_or_default() {
                    item["source"] = json!(label);
                    merged.push(item);
                }
            }
            Err(e) => errors.push(format!("{}: {e}", source_label(label))),
        }
    }
    let pagecount = merged.len().min(60) / 20 + 1;
    json!({"ok": true, "list": merged, "page": page, "pagecount": pagecount, "errors": errors})
}

/// 统一详情分发说明：瓜子沿用 /api/guazi/detail（前端特判），本端点只服务三个新源
/// HTTP handler: /api/vod/search
pub async fn search(axum::extract::Query(q): axum::extract::Query<BTreeMap<String, String>>) -> impl axum::response::IntoResponse {
    let src = q.get("src").cloned().unwrap_or_else(|| "all".into());
    let wd = q.get("wd").cloned().unwrap_or_default();
    let page: i64 = q.get("page").and_then(|p| p.parse().ok()).unwrap_or(1);
    if wd.trim().is_empty() {
        return axum::Json(json!({"ok": false, "error": "关键词不能为空", "list": []}));
    }
    axum::Json(unified_search(&src, &wd, page).await)
}

/// HTTP handler: /api/vod/detail?src=&id=（非瓜子源）
pub async fn detail(axum::extract::Query(q): axum::extract::Query<BTreeMap<String, String>>) -> impl axum::response::IntoResponse {
    let src = q.get("src").cloned().unwrap_or_default();
    let id = q.get("id").cloned().unwrap_or_default();
    if id.is_empty() {
        return axum::Json(json!({"error": "id 不能为空"}));
    }
    let res = match src.as_str() {
        "ruyi" => ruyi_detail(&id).await,
        "rebang" => rebang_detail(&id).await,
        "jianpian" => jianpian_detail(&id).await,
        other => Err(anyhow!("该源不支持此端点: {other}（瓜子请用 /api/guazi/detail）")),
    };
    match res {
        Ok(v) => axum::Json(v),
        Err(e) => axum::Json(json!({"error": e.to_string()})),
    }
}

/// HTTP handler: /api/vod/play?src=&q=
pub async fn play(axum::extract::Query(q): axum::extract::Query<BTreeMap<String, String>>) -> impl axum::response::IntoResponse {
    let src = q.get("src").cloned().unwrap_or_default();
    let ep_q = q.get("q").cloned().unwrap_or_default();
    if ep_q.is_empty() {
        return axum::Json(json!({"error": "q 不能为空"}));
    }
    let res = match src.as_str() {
        "ruyi" => Ok(ruyi_play(&ep_q)),
        "rebang" => rebang_play(&ep_q).await,
        "jianpian" => jianpian_play(&ep_q),
        other => Err(anyhow!("该源不支持此端点: {other}（瓜子请用 /api/guazi/play）")),
    };
    match res {
        Ok(v) => axum::Json(v),
        Err(e) => axum::Json(json!({"error": format!("取播放地址失败: {e}")})),
    }
}
