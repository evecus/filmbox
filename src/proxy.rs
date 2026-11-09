// proxy.rs —— 图片代理与视频流代理（解决豆瓣/瓜子 CDN 防盗链、m3u8 重写）

use axum::body::Body;
use axum::extract::Query;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::OnceLock;
use std::time::Duration;

const PLAY_UA: &str = "Lavf/57.83.100";
const PLAY_REFERER: &str = "http://WJiZxLXA2.com/";

fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .danger_accept_invalid_certs(true)
            .build()
            .expect("构建代理 HTTP 客户端失败")
    })
}

fn err_resp(code: StatusCode, msg: &str) -> Response {
    (code, msg.to_string()).into_response()
}

fn proxy_url_param(q: &BTreeMap<String, String>) -> Option<String> {
    q.get("url")
        .map(|u| u.trim_start_matches("http://").trim_start_matches("https://").to_string())
        .map(|_| q.get("url").cloned().unwrap_or_default())
        .filter(|u| u.starts_with("http"))
}

/// GET /api/img?url=... —— 图片代理，自动带 Referer
pub async fn img(Query(q): Query<BTreeMap<String, String>>) -> Response {
    let Some(url) = proxy_url_param(&q) else {
        return err_resp(StatusCode::BAD_REQUEST, "缺少 url 参数");
    };

    let referer = if url.contains("doubanio") || url.contains("douban.com") {
        "https://m.douban.com"
    } else {
        &url[..]
    };

    let resp = match client().get(&url).header("Referer", referer).header("User-Agent", PLAY_UA).send().await {
        Ok(r) => r,
        Err(e) => return err_resp(StatusCode::BAD_GATEWAY, &format!("图片代理失败: {e}")),
    };

    let mut headers = HeaderMap::new();
    if let Some(ct) = resp.headers().get(header::CONTENT_TYPE) {
        headers.insert(header::CONTENT_TYPE, ct.clone());
    } else {
        headers.insert(header::CONTENT_TYPE, "image/jpeg".parse().unwrap());
    }
    headers.insert(header::CACHE_CONTROL, "public, max-age=86400".parse().unwrap());

    match resp.bytes().await {
        Ok(bytes) => (StatusCode::OK, headers, bytes).into_response(),
        Err(e) => err_resp(StatusCode::BAD_GATEWAY, &format!("读取图片失败: {e}")),
    }
}

/// GET /api/stream?url=... —— 视频流代理：
/// - 携带播放 UA（对应 play() 返回的 header）
/// - 透传 Range / 206，支持拖动进度
/// - m3u8 内容自动重写内部地址为代理地址
pub async fn stream(Query(q): Query<BTreeMap<String, String>>) -> Response {
    let Some(url) = proxy_url_param(&q) else {
        return err_resp(StatusCode::BAD_REQUEST, "缺少 url 参数");
    };

    // 支持按源自定义请求头（ua/referer 查询参数覆盖默认的瓜子播放头）
    let ua = q.get("ua").cloned().filter(|s| !s.is_empty()).unwrap_or_else(|| PLAY_UA.to_string());
    let referer = q.get("referer").cloned().filter(|s| !s.is_empty()).unwrap_or_else(|| PLAY_REFERER.to_string());
    let mut req = client()
        .get(&url)
        .header("User-Agent", ua)
        .header("Referer", referer);
    let range = q.get("range").cloned().or_else(|| q.get("Range").cloned());
    if let Some(r) = &range {
        req = req.header("Range", r);
    }

    let resp = match req.send().await {
        Ok(r) => r,
        Err(e) => return err_resp(StatusCode::BAD_GATEWAY, &format!("视频代理失败: {e}")),
    };
    let status = resp.status();

    // m3u8：读全文，重写 URI 后返回
    let ct = resp
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_lowercase();
    let is_m3u8 = ct.contains("mpegurl") || url.to_lowercase().ends_with(".m3u8");
    if is_m3u8 {
        let Ok(text) = resp.text().await else {
            return err_resp(StatusCode::BAD_GATEWAY, "读取 m3u8 失败");
        };
        let rewritten = rewrite_m3u8(&text, &url);
        return (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/vnd.apple.mpegurl")],
            rewritten,
        )
            .into_response();
    }

    let mut headers = HeaderMap::new();
    for key in [header::CONTENT_TYPE, header::CONTENT_LENGTH, header::CONTENT_RANGE, header::ACCEPT_RANGES, header::ETAG, header::LAST_MODIFIED] {
        if let Some(v) = resp.headers().get(&key) {
            headers.insert(key, v.clone());
        }
    }
    if !headers.contains_key(header::CONTENT_TYPE) {
        headers.insert(header::CONTENT_TYPE, "video/mp4".parse().unwrap());
    }

    let axum_status = StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::OK);
    (axum_status, headers, Body::from_stream(resp.bytes_stream())).into_response()
}

/// 将 m3u8 中的相对/绝对地址重写为经本站 /api/stream 代理的地址
fn rewrite_m3u8(text: &str, base: &str) -> String {
    let mut out = String::with_capacity(text.len() + 256);
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            out.push('\n');
            continue;
        }
        if trimmed.starts_with('#') {
            // 处理 #EXT-X-KEY:...URI="xxx" 等属性中的 URI
            out.push_str(&rewrite_uri_attrs(line, base));
            out.push('\n');
        } else {
            out.push_str(&to_proxy_url(trimmed, base));
            out.push('\n');
        }
    }
    out
}

fn rewrite_uri_attrs(line: &str, base: &str) -> String {
    let mut result = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(pos) = rest.find("URI=\"") {
        result.push_str(&rest[..pos + 5]);
        rest = &rest[pos + 5..];
        if let Some(end) = rest.find('"') {
            let uri = &rest[..end];
            result.push_str(&to_proxy_url(uri, base));
            result.push('"');
            rest = &rest[end + 1..];
        } else {
            break;
        }
    }
    result.push_str(rest);
    result
}

fn to_proxy_url(uri: &str, base: &str) -> String {
    let absolute = if uri.starts_with("http://") || uri.starts_with("https://") {
        uri.to_string()
    } else if let Ok(base_url) = reqwest::Url::parse(base) {
        base_url.join(uri).map(|u| u.to_string()).unwrap_or_else(|_| uri.to_string())
    } else {
        uri.to_string()
    };
    format!("/api/stream?url={}", urlencoding::encode(&absolute))
}

#[allow(dead_code)]
fn unused(_: &Value) {}
