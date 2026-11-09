// douban.rs —— 对应 豆瓣推荐.js：豆瓣做首页推荐与筛选（m.douban.com/rexxar API）

use crate::cache;
use axum::extract::Query;
use axum::response::IntoResponse;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::time::Duration;

const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

fn client() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .expect("构建豆瓣 HTTP 客户端失败")
    })
}

fn classes() -> Value {
    json!([
        {"type_id": "movie", "type_name": "选电影"},
        {"type_id": "tv", "type_name": "选剧集"},
        {"type_id": "show", "type_name": "选综艺"},
        {"type_id": "movie_filter", "type_name": "电影筛选"},
        {"type_id": "tv_filter", "type_name": "电视剧筛选"},
        {"type_id": "show_filter", "type_name": "综艺筛选"},
    ])
}

fn years_full() -> Vec<(&'static str, &'static str)> {
    vec![
        ("全部", ""), ("2026", "2026"), ("2025", "2025"), ("2024", "2024"), ("2023", "2023"),
        ("2022", "2022"), ("2021", "2021"), ("2020", "2020"), ("2019", "2019"),
        ("2020年代", "2020年代"), ("2010年代", "2010年代"), ("2000年代", "2000年代"),
        ("90年代", "90年代"), ("80年代", "80年代"), ("70年代", "70年代"), ("60年代", "60年代"), ("更早", "更早"),
    ]
}

fn regions_full() -> Vec<&'static str> {
    vec![
        "全部", "华语", "欧美", "国外", "韩国", "日本", "中国大陆", "中国香港", "中国台湾",
        "美国", "英国", "泰国", "意大利", "法国", "德国", "西班牙", "俄罗斯", "瑞典",
        "巴西", "丹麦", "印度", "加拿大", "爱尔兰", "澳大利亚",
    ]
}

fn sort_filter() -> Value {
    json!([{"key": "sort", "name": "排序", "init": "U", "value": [
        {"name": "热度", "value": "U"}, {"name": "评分", "value": "S"}, {"name": "时间", "value": "R"},
    ]}])
}

fn opts(pairs: &[(&str, &str)]) -> Value {
    Value::Array(pairs.iter().map(|(n, v)| json!({"name": n, "value": v})).collect())
}

fn filters() -> Value {
    let regions_movie: Vec<&str> = vec!["全部", "华语", "欧美", "韩国", "日本"];
    let genres_movie: Vec<&str> = vec![
        "全部", "喜剧", "爱情", "动作", "科幻", "动画", "悬疑", "犯罪", "惊悚", "冒险", "音乐",
        "历史", "奇幻", "恐怖", "战争", "传记", "歌舞", "武侠", "情色", "灾难", "西部", "纪录片", "短片",
    ];
    let genres_tv: Vec<&str> = vec![
        "全部", "喜剧", "爱情", "悬疑", "动画", "武侠", "古装", "家庭", "犯罪", "科幻", "恐怖",
        "历史", "战争", "动作", "冒险", "传记", "剧情", "奇幻", "惊悚", "灾难", "歌舞", "音乐",
    ];
    let genres_show: Vec<&str> = vec!["全部", "真人秀", "脱口秀", "音乐", "歌舞"];
    let platforms: Vec<(&str, &str)> = vec![
        ("全部", ""), ("腾讯视频", "腾讯视频"), ("爱奇艺", "爱奇艺"), ("优酷", "优酷"),
        ("湖南卫视", "湖南卫视"), ("Netflix", "Netflix"), ("HBO", "HBO"), ("BBC", "BBC"),
        ("NHK", "NHK"), ("CBS", "CBS"), ("NBC", "NBC"), ("tvN", "tvN"),
    ];

    let mut region_movie_vals: Vec<Value> = Vec::new();
    let mut region_full_vals: Vec<Value> = Vec::new();
    for r in &regions_movie {
        region_movie_vals.push(json!({"name": r, "value": if *r == "全部" { "" } else { *r }}));
    }
    for r in regions_full() {
        region_full_vals.push(json!({"name": r, "value": if r == "全部" { "" } else { r }}));
    }

    json!({
        "movie": [
            {"key": "category", "name": "类型", "init": "热门", "value": opts(&[
                ("热门", "热门"), ("最新", "最新"), ("豆瓣高分", "豆瓣高分"), ("冷门佳片", "冷门佳片"),
            ])},
            {"key": "type", "name": "地区", "init": "全部", "value": opts(&[
                ("全部", "全部"), ("华语", "华语"), ("欧美", "欧美"), ("韩国", "韩国"), ("日本", "日本"),
            ])},
        ],
        "tv": [
            {"key": "type", "name": "类型", "init": "tv", "value": opts(&[
                ("综合", "tv"), ("国产剧", "tv_domestic"), ("欧美剧", "tv_american"),
                ("日剧", "tv_japanese"), ("韩剧", "tv_korean"), ("动漫", "tv_animation"), ("纪录片", "tv_documentary"),
            ])},
        ],
        "show": [
            {"key": "type", "name": "类型", "init": "show", "value": opts(&[
                ("综合", "show"), ("国内", "show_domestic"), ("国外", "show_foreign"),
            ])},
        ],
        "movie_filter": [
            {"key": "genre", "name": "类型", "init": "", "value": genres_movie.iter().map(|g| json!({"name": g, "value": if *g == "全部" {""} else {*g}})).collect::<Vec<_>>()},
            {"key": "region", "name": "地区", "init": "", "value": region_movie_vals},
            {"key": "year", "name": "年代", "init": "", "value": opts(&years_full())},
            sort_filter(),
        ],
        "tv_filter": [
            {"key": "genre", "name": "类型", "init": "", "value": genres_tv.iter().map(|g| json!({"name": g, "value": if *g == "全部" {""} else {*g}})).collect::<Vec<_>>()},
            {"key": "region", "name": "地区", "init": "", "value": region_full_vals},
            {"key": "year", "name": "年代", "init": "", "value": opts(&years_full())},
            {"key": "platform", "name": "平台", "init": "", "value": opts(&platforms)},
            sort_filter(),
        ],
        "show_filter": [
            {"key": "genre", "name": "类型", "init": "", "value": genres_show.iter().map(|g| json!({"name": g, "value": if *g == "全部" {""} else {*g}})).collect::<Vec<_>>()},
            {"key": "region", "name": "地区", "init": "", "value": region_full_vals},
            {"key": "year", "name": "年代", "init": "", "value": opts(&years_full())},
            {"key": "platform", "name": "平台", "init": "", "value": opts(&platforms)},
            sort_filter(),
        ],
    })
}

/// 尽力获取腾讯视频轮播 Banner（对应 home() 中 banner 逻辑，失败不影响首页）
async fn fetch_banner() -> Vec<Value> {
    let url = "https://pbaccess.video.qq.com/trpc.vector_layout.page_view.PageService/getPage?video_appid=3000010&vversion_platform=2";
    let body = json!({
        "page_params": {
            "page_type": "channel", "page_id": "100113", "scene": "channel",
            "new_mark_label_enabled": "1", "vl_to_mvl": "",
            "free_watch_trans_info": "{\"ad_frequency_control_time_list\":{}}",
            "ad_exp_ids": "100000", "skip_privacy_types": "0", "support_click_scan": "1",
        },
        "page_bypass_params": {
            "params": {
                "platform_id": "2", "caller_id": "3000010", "data_mode": "default",
                "user_mode": "default", "specified_strategy": "", "page_type": "channel",
                "page_id": "100113", "scene": "channel", "new_mark_label_enabled": "1",
            },
            "scene": "channel", "app_version": "", "abtest_bypass_id": "aa836e91e1411155",
        },
        "page_context": null,
    });

    let mut banner = Vec::new();
    let Ok(resp) = client()
        .post(url)
        .header("User-Agent", UA)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
    else {
        return banner;
    };
    let Ok(text) = resp.text().await else { return banner };
    let Ok(data) = serde_json::from_str::<Value>(&text) else { return banner };

    'outer: for card_list in data.pointer("/data/CardList").and_then(Value::as_array).unwrap_or(&vec![]) {
        if card_list["type"] == "pc_carousel" {
            let Some(cards) = card_list.pointer("/children_list/list/cards").and_then(Value::as_array) else {
                continue;
            };
            for card in cards {
                let Some(bg) = card["params"]["pic_ori_2880x900"].as_str() else { continue };
                let topic = card["params"]["topic_label"].as_str().unwrap_or("");
                let actors = match topic.split_once(" / ") {
                    Some((_, rest)) if !rest.is_empty() => rest.to_string(),
                    _ => topic.to_string(),
                };
                banner.push(json!({
                    "title": card["params"]["title"].as_str().or(card["params"]["title_pc"].as_str()).unwrap_or(""),
                    "subtitle": card["params"]["stitle_pc"].as_str().or(card["params"]["material_video_subtitle"].as_str()).unwrap_or(""),
                    "backgroundImage": bg,
                    "genre": card["params"]["main_genre"].as_str().unwrap_or(""),
                    "actors": actors,
                    "description": card["params"]["rec_normal_reason"].as_str().unwrap_or(""),
                }));
                if banner.len() >= 5 {
                    break 'outer;
                }
            }
        }
    }
    banner
}

/// 豆瓣图片走本站代理（防盗链）
fn proxied_pic(pic: &str) -> String {
    if pic.is_empty() {
        return String::new();
    }
    format!("/api/img?url={}", urlencoding::encode(pic))
}

fn map_item(item: &Value, type_id: &str, type_name: &str) -> Value {
    let subtitle = item["card_subtitle"].as_str().unwrap_or("");
    let vod_year = subtitle.split(' ').next().filter(|s| s.len() == 4 && s.chars().all(|c| c.is_ascii_digit())).unwrap_or("");
    let remarks = item["episodes_info"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .unwrap_or_else(|| if item["is_new"].as_bool().unwrap_or(false) { "新剧".into() } else { String::new() });
    let vod_subtitle: String = match subtitle.split_once(" / ") {
        Some((_, rest)) => rest.to_string(),
        None => subtitle.to_string(),
    };
    let pic = item["pic"]["large"].as_str().or(item["pic"]["normal"].as_str()).unwrap_or("");
    let id = item["id"].as_str().unwrap_or("").to_string();

    json!({
        "vod_id": id,
        "link": format!("https://movie.douban.com/subject/{}", id),
        "vod_name": item["title"].as_str().unwrap_or(""),
        "vod_pic": proxied_pic(pic),
        "type_id": type_id,
        "type_name": type_name,
        "vod_remarks": remarks,
        "vod_year": vod_year,
        "vod_douban_score": item["rating"]["value"].as_f64().map(|v| v.to_string()).unwrap_or_default(),
        "vod_subtitle": vod_subtitle,
    })
}

async fn douban_get(url: &str, referer: &str) -> anyhow::Result<Value> {
    if let Some(v) = cache::get(&format!("douban:{url}")) {
        return Ok(v);
    }
    let resp = client()
        .get(url)
        .header("User-Agent", UA)
        .header("Referer", referer)
        .header("Accept", "*/*")
        .send()
        .await?
        .error_for_status()?;
    let data: Value = resp.json().await?;
    cache::set(&format!("douban:{url}"), data.clone(), Duration::from_secs(120));
    Ok(data)
}

/// 分类请求逻辑，对应 category()
pub async fn category_inner(cat: &str, page: i64, filters: &Value) -> anyhow::Result<Value> {
    let limit = 20;
    let start = (page - 1) * limit;
    let f = |k: &str| filters[k].as_str().unwrap_or("").to_string();

    let (url, referer, type_name): (String, &str, &str) = match cat {
        "movie" => (
            format!(
                "https://m.douban.com/rexxar/api/v2/subject/recent_hot/movie?start={start}&limit={limit}&category={}&type={}",
                urlencoding::encode(&f("category").replace("", "")),
                urlencoding::encode(&f("type")),
            ),
            "https://movie.douban.com/explore",
            "选电影",
        ),
        "tv" | "show" => {
            let default_type = if cat == "tv" { "tv_domestic" } else { "show" };
            let t = if f("type").is_empty() { default_type.to_string() } else { f("type") };
            (
                format!(
                    "https://m.douban.com/rexxar/api/v2/subject/recent_hot/tv?start={start}&limit={limit}&category={cat}&type={}",
                    urlencoding::encode(&t),
                ),
                "https://movie.douban.com/tv/",
                if cat == "tv" { "选剧集" } else { "选综艺" },
            )
        }
        "movie_filter" => {
            let mut selected = serde_json::Map::new();
            if !f("genre").is_empty() { selected.insert("类型".into(), json!(f("genre"))); }
            if !f("region").is_empty() { selected.insert("地区".into(), json!(f("region"))); }
            let mut tags = vec![];
            for k in ["genre", "region", "year"] {
                if !f(k).is_empty() { tags.push(f(k)); }
            }
            (
                format!(
                    "https://m.douban.com/rexxar/api/v2/movie/recommend?refresh=0&start={start}&count={limit}&selected_categories={}&uncollect=false&score_range=0,10&tags={}&sort={}",
                    urlencoding::encode(&Value::Object(selected).to_string()),
                    urlencoding::encode(&tags.join(",")),
                    f("sort"),
                ),
                "https://movie.douban.com/explore",
                "电影筛选",
            )
        }
        "tv_filter" | "show_filter" => {
            let form = if cat == "tv_filter" { "电视剧" } else { "综艺" };
            let mut selected = serde_json::Map::new();
            selected.insert("形式".into(), json!(form));
            if !f("genre").is_empty() { selected.insert("类型".into(), json!(f("genre"))); }
            if !f("region").is_empty() { selected.insert("地区".into(), json!(f("region"))); }
            let mut tags = vec![];
            for k in ["genre", "region", "year", "platform"] {
                if !f(k).is_empty() { tags.push(f(k)); }
            }
            (
                format!(
                    "https://m.douban.com/rexxar/api/v2/tv/recommend?refresh=0&start={start}&count={limit}&selected_categories={}&uncollect=false&score_range=0,10&tags={}&sort={}",
                    urlencoding::encode(&Value::Object(selected).to_string()),
                    urlencoding::encode(&tags.join(",")),
                    f("sort"),
                ),
                "https://movie.douban.com/tv/",
                if cat == "tv_filter" { "电视剧筛选" } else { "综艺筛选" },
            )
        }
        _ => return Err(anyhow::anyhow!("未知的分类ID: {cat}")),
    };

    let data = douban_get(&url, referer).await?;
    let items = data["items"].as_array().cloned().unwrap_or_default();
    let list: Vec<Value> = items.iter().map(|it| map_item(it, cat, type_name)).collect();
    let total = data["total"].as_i64().or(data["count"].as_i64()).unwrap_or(list.len() as i64).max(0);
    Ok(json!({
        "page": page,
        "pagecount": if total == 0 { 1 } else { (total + limit - 1) / limit },
        "total": total,
        "list": list,
    }))
}

/// HTTP: /api/douban/home —— 首页：分类 + 筛选 + 剧集推荐 + Banner
pub async fn home() -> impl IntoResponse {
    let list = category_inner("tv", 1, &json!({})).await;
    axum::Json(json!({
        "class": classes(),
        "filters": filters(),
        "list": list.map(|v| v["list"].clone()).unwrap_or(json!([])),
        "banner": fetch_banner().await,
    }))
}

/// HTTP: /api/douban/category?cat=movie&page=1&filters={"category":"热门","type":"全部"}
pub async fn category(Query(q): Query<BTreeMap<String, String>>) -> impl IntoResponse {
    let cat = q.get("cat").cloned().unwrap_or_else(|| "movie".into());
    let page = q.get("page").and_then(|s| s.parse::<i64>().ok()).unwrap_or(1);
    let filters: Value = q
        .get("filters")
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or(json!({}));

    axum::Json(category_inner(&cat, page, &filters).await.unwrap_or_else(|e| {
        json!({"page": page, "pagecount": 0, "total": 0, "list": [], "error": e.to_string()})
    }))
}
