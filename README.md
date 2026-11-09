# FilmBox — 纯 Rust 影视站

豆瓣做首页推荐，**瓜子 / 如意 / 热播 / 荐片** 四源聚合搜索与播放，内置网页面板。**单二进制、零 Node.js / Python 依赖**，是 `豆瓣推荐.js` + `瓜子.js` + `如意资源.js` + `热播.js` + `荐片.js` 五个 OmniBox 爬虫脚本的纯 Rust 原生重写。

## 功能对照

| 原 JS 脚本 | Rust 重写 | 说明 |
|---|---|---|
| 豆瓣推荐.js `home/category` | `src/douban.rs` | 6 分类（选电影/剧集/综艺 + 三种筛选）、筛选条件全量移植、腾讯 Banner |
| 瓜子.js `search/category` | `src/guazi.rs` | 标题精确过滤搜索、分类浏览、年份/地区/排序 |
| 瓜子.js `detail/play` | `src/guazi.rs` | 详情汇聚、多集选集、分辨率优先级、取真实播放地址 |
| 瓜子.js `rsaTool/cryptoTool` | `src/crypto.rs` | AES-128-CBC + MD5 签名 + RSA 裸解密（NO_PADDING），纯 Rust 实现 |
| 如意资源.js v1.0.2 | `src/vod.rs` | MacCMS 采集接口（3 域名自动切换）、批量封面补全、直连 m3u8/mp4 |
| 热播.js v1.0.3 | `src/vod.rs` | multipart 表单 + MD5 签名、多线路解析接口调用（失败回退直链） |
| 荐片.js v1.1.1 | `src/vod.rs` | H5 接口、VIP/FTP 线路屏蔽、播放头伪装 |
| OmniBox `baseURL` 图片代理 | `src/proxy.rs` | `/api/img`（豆瓣防盗链 Referer）+ `/api/stream`（视频流代理、Range 透传、m3u8 重写、按源自定义 UA/Referer） |
| OmniBox 播放界面 | `static/*.html` | 深色网页面板：首页推荐 → 四源聚合匹配 → 选集 → 播放（hls.js 走 CDN） |

## 快速开始

```bash
cargo build --release
PORT=8021 ./target/release/filmbox
```

浏览器打开 `http://127.0.0.1:8021`

- `/` 豆瓣首页（分类 Tab + 筛选 + 海报网格），点卡片自动去四源聚合匹配
- `/search.html?q=xxx` 聚合搜索（可选 单源：瓜子/如意/热播/荐片）
- `/play.html?src=源&id=xx` 播放页（选集 + 瓜子分辨率切换）

## 环境变量（可选）

| 变量 | 说明 |
|---|---|
| `PORT` | 监听端口，默认 8021 |
| `REBANG_HOST` | 热播 API 域名覆盖（逗号分隔多个，如 `http://v.rbotv.cn,http://tt.ysdqjs.cn`；默认内置 3 个自动探活） |
| `JP_HOST` | 荐片 API 域名覆盖（荐片域名频繁更换，若内置域名失效，可抓包 APP 取新域名填入，如 `JP_HOST=https://h5.xxxxx.com`） |

## HTTP API

| 接口 | 说明 |
|---|---|
| `GET /api/douban/home` | 首页：分类 + 筛选定义 + 剧集推荐 + Banner |
| `GET /api/douban/category?cat=tv&page=1&filters={"type":"tv_domestic"}` | 豆瓣分类（movie/tv/show/movie_filter/tv_filter/show_filter） |
| `GET /api/vod/search?src=all&wd=片名&page=1` | **聚合搜索**（src=all 或 guazi/ruyi/rebang/jianpian，结果带 source 字段） |
| `GET /api/vod/detail?src=ruyi&id=123` | 非瓜子源详情（episodes 每项带不透明 q 参数） |
| `GET /api/vod/play?src=rebang&q=...` | 非瓜子源播放解析（热播解析接口失败自动回退直链） |
| `GET /api/guazi/search?wd=片名&page=1` | 瓜子搜索（标题精确过滤） |
| `GET /api/guazi/category?tid=1&page=1` | 瓜子站内分类 |
| `GET /api/guazi/detail?vod_id=123` | 详情 + 选集列表 |
| `GET /api/guazi/play?param=...&res=1080` | 取播放地址 |
| `GET /api/img?url=` | 图片代理（自动带 Referer） |
| `GET /api/stream?url=&ua=&referer=` | 视频流代理（UA 伪装 + Range + m3u8 重写，ua/referer 可按源覆盖） |

## 关键实现（对照 瓜子.js）

- **请求加密**：`request_key = AES-128-CBC(JSON参数)`，签名 `MD5("token_id=,token=...,request_key=...,time=...,keys=...*&zvdvdvddbfikkkumtmdwqppp?|4Y!s!2br")`
- **响应解密**：`data.keys` 用内置 1024 位私钥按 128 字节分块做 RSA 裸解密（`c^d mod n` 后按 PKCS#1 去填充），得到本轮 AES key/iv，再解密 `data.response_key`
- **分辨率优先级**：8K > 4K > 1440 > 1080 > 720 > 超清 > 高清 > 标清
- **播放 UA**：`com.android.chrome/5.0.6 (Linux;Android 9)  AndroidXMedia3/1.9.2`

## 已知限制

- 瓜子 API 的 token 为脚本内置硬编码值，若失效需从 瓜子.js 同步更新（`src/guazi.rs` 顶部 `TOKEN`）
- 沙箱（海外 IP）实测瓜子接口返回 `403 forbidden`（地域/IP 风控）；加密管线已通过单元测试，在国内网络环境应正常
- 未实现原 JS 的 TMDB 刮削与弹幕匹配（OmniBox 专有 SDK 能力），详情直接使用瓜子自身数据
- m3u8 的 `EXT-X-KEY`/分片地址已做代理重写，但未处理跨域证书播放（DRM）

## 项目结构

```
filmbox/
├── Cargo.toml
├── src/
│   ├── main.rs     # axum 路由 + 静态页内嵌
│   ├── douban.rs   # 豆瓣 rexxar API（首页/分类/筛选/Banner）
│   ├── guazi.rs    # 瓜子加密 API 客户端
│   ├── crypto.rs   # AES / MD5 / RSA 裸解密
│   ├── cache.rs    # 内存 TTL 缓存
│   └── proxy.rs    # 图片/视频流代理 + m3u8 重写
└── static/         # 内嵌网页面板（无构建步骤）
```
