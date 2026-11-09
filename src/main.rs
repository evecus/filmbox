// main.rs —— FilmBox 纯 Rust 影视站入口
// 豆瓣做首页推荐，瓜子做搜索与播放源，全部内嵌网页面板，单二进制运行

mod cache;
mod crypto;
mod douban;
mod guazi;
mod proxy;
mod vod;

use axum::response::{Html, IntoResponse};
use axum::routing::get;
use axum::Router;

const INDEX_HTML: &str = include_str!("../static/index.html");
const SEARCH_HTML: &str = include_str!("../static/search.html");
const PLAY_HTML: &str = include_str!("../static/play.html");
const APP_CSS: &str = include_str!("../static/app.css");

async fn index_page() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn search_page() -> Html<&'static str> {
    Html(SEARCH_HTML)
}

async fn play_page() -> Html<&'static str> {
    Html(PLAY_HTML)
}

async fn css() -> impl IntoResponse {
    ([(axum::http::header::CONTENT_TYPE, "text/css; charset=utf-8")], APP_CSS)
}

async fn health() -> impl IntoResponse {
    axum::Json(serde_json::json!({"ok": true, "version": env!("CARGO_PKG_VERSION")}))
}

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/", get(index_page))
        .route("/search.html", get(search_page))
        .route("/play.html", get(play_page))
        .route("/app.css", get(css))
        .route("/api/health", get(health))
        .route("/api/douban/home", get(douban::home))
        .route("/api/douban/category", get(douban::category))
        .route("/api/guazi/category", get(guazi::category))
        .route("/api/guazi/search", get(guazi::search))
        .route("/api/guazi/detail", get(guazi::detail))
        .route("/api/guazi/play", get(guazi::play))
        .route("/api/guazi/debug", get(guazi::debug))
        .route("/api/vod/search", get(vod::search))
        .route("/api/vod/detail", get(vod::detail))
        .route("/api/vod/play", get(vod::play))
        .route("/api/img", get(proxy::img))
        .route("/api/stream", get(proxy::stream));

    let port: u16 = std::env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(8021);
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port))
        .await
        .expect("端口绑定失败");

    println!("FilmBox 已启动: http://0.0.0.0:{port}");
    println!("  首页(豆瓣推荐): /   搜索(瓜子): /search.html   播放: /play.html");
    axum::serve(listener, app).await.expect("服务运行失败");
}
