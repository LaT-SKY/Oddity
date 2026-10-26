//! 官方公告聚合。
//!
//! 页面由官方服务端生成，启动器只提取必要的元数据和跳转链接，避免在
//! WebView 中直接加载第三方页面。每个来源独立失败，调用方仍可展示其他来源。

use std::io::Read;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{distribution::Distribution, Error, Result, USER_AGENT};

pub const OFFICIAL_ANNOUNCEMENT_URL: &str = "https://yh.wanmei.com/news/gamebroad/index.html";
pub const OFFICIAL_LAUNCHER_URL: &str = "https://yh.wanmei.com/launcher/launcher_ob2.html?expand=0";
pub const OFFICIAL_SWIPER_URL: &str =
    "https://static.games.wanmei.com/public/commonData/gamesData/gameSwiper/yh-gameSwiper.js";
pub const BILIBILI_SPACE_URL: &str = "https://space.bilibili.com/3546636978489848/dynamic";
pub const BILIBILI_API_URL: &str =
    "https://api.bilibili.com/x/polymer/web-dynamic/desktop/v1/feed/space?host_mid=3546636978489848&features=itemOpusStyle";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BulletinKind {
    Bilibili,
    Preview,
    Maintenance,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BulletinItem {
    pub kind: BulletinKind,
    pub title: String,
    pub summary: String,
    pub url: String,
    pub image_url: Option<String>,
    pub published_at: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BulletinFeed {
    pub items: Vec<BulletinItem>,
    pub errors: Vec<String>,
}

const BROWSER_USER_AGENT: &str =
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 Chrome/131.0 Safari/537.36";

fn fetch_text(url: &str, limit: u64, referer: Option<&str>) -> Result<String> {
    let mut request = ureq::get(url)
        .header(
            "User-Agent",
            referer.map_or(USER_AGENT, |_| BROWSER_USER_AGENT),
        )
        .header("Accept", "text/html,application/json,*/*")
        .config()
        .timeout_global(Some(Duration::from_secs(15)))
        .build();
    if let Some(referer) = referer {
        request = request.header("Referer", referer);
    }
    let mut response = request
        .call()
        .map_err(|error| Error::Http(format!("GET {url}: {error}")))?;
    let mut body = String::new();
    response
        .body_mut()
        .as_reader()
        .take(limit)
        .read_to_string(&mut body)
        .map_err(|error| Error::Http(format!("read {url}: {error}")))?;
    Ok(body)
}

fn decode_entities(input: &str) -> String {
    input
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

fn strip_tags(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut in_tag = false;
    for ch in input.chars() {
        match ch {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => output.push(ch),
            _ => {}
        }
    }
    decode_entities(&output)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn class_text(block: &str, class_name: &str) -> String {
    let marker = format!("class=\"{class_name}\"");
    let Some(class_start) = block.find(&marker) else {
        return String::new();
    };
    let Some(start) = block[class_start..]
        .find('>')
        .map(|index| class_start + index + 1)
    else {
        return String::new();
    };
    let Some(end) = block[start..].find('<').map(|index| start + index) else {
        return String::new();
    };
    strip_tags(&block[start..end])
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let marker = format!("{name}=\"");
    let start = tag.find(&marker)? + marker.len();
    let end = tag[start..].find('"')? + start;
    Some(tag[start..end].to_string())
}

/// 解析公告列表页，按官方页面顺序保留最新的三条公告。
pub fn parse_announcement_html(html: &str) -> Vec<BulletinItem> {
    parse_official_list(
        html,
        "https://yh.wanmei.com",
        "/news/gamebroad/",
        BulletinKind::Maintenance,
    )
}

fn parse_official_list(
    html: &str,
    origin: &str,
    channel: &str,
    kind: BulletinKind,
) -> Vec<BulletinItem> {
    let mut items = Vec::new();
    let mut cursor = 0;
    while let Some(relative) = html[cursor..].find("<a ") {
        let start = cursor + relative;
        let Some(tag_end) = html[start..].find('>') else {
            break;
        };
        let tag_end = start + tag_end;
        let Some(close_relative) = html[tag_end..].find("</a>") else {
            break;
        };
        let end = tag_end + close_relative + 4;
        let tag = &html[start..=tag_end];
        let block = &html[start..end];
        let Some(href) = attr(tag, "href") else {
            cursor = end;
            continue;
        };
        if !href.contains(channel) {
            cursor = end;
            continue;
        }
        let title = class_text(block, "title");
        if title.is_empty() {
            cursor = end;
            continue;
        }
        items.push(BulletinItem {
            kind: kind.clone(),
            title,
            summary: class_text(block, "des"),
            url: if href.starts_with("https://") || href.starts_with("http://") {
                href
            } else {
                format!("{origin}/{}", href.trim_start_matches('/'))
            },
            image_url: None,
            published_at: class_text(block, "date"),
            source: "官方公告".to_string(),
        });
        if items.len() >= 3 {
            break;
        }
        cursor = end;
    }
    items
}

/// 解析 `yh-gameSwiper.js` 中的 `yh_data_data`，只取最新一张前瞻 banner。
pub fn parse_banner_script(script: &str) -> Result<Vec<BulletinItem>> {
    let start = script
        .find("var yh_data_data")
        .and_then(|index| script[index..].find('=').map(|offset| index + offset + 1))
        .ok_or_else(|| Error::Parse("yh-gameSwiper.js 缺少 yh_data_data".to_string()))?;
    let json = script[start..]
        .split_once(';')
        .map(|(value, _)| value.trim())
        .unwrap_or_else(|| script[start..].trim());
    let root: Value = serde_json::from_str(json)
        .map_err(|error| Error::Parse(format!("解析 yh_data_data 失败：{error}")))?;
    let Some(entry) = root.get("lb1").and_then(Value::as_array).and_then(|items| {
        items.iter().find(|item| {
            item.get("title")
                .and_then(Value::as_str)
                .is_some_and(|title| !title.is_empty())
                && item
                    .get("link")
                    .and_then(Value::as_str)
                    .is_some_and(|url| !url.is_empty())
        })
    }) else {
        return Ok(Vec::new());
    };
    let title = entry
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let url = entry
        .get("link")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if title.is_empty() || url.is_empty() {
        return Ok(Vec::new());
    }
    Ok(vec![BulletinItem {
        kind: BulletinKind::Preview,
        title: title.to_string(),
        summary: "版本前瞻回顾".to_string(),
        url: url.to_string(),
        image_url: entry
            .get("bigpic")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        published_at: String::new(),
        source: "版本前瞻".to_string(),
    }])
}

fn value_text(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn module_text(module: &Value) -> String {
    let text = value_text(module.get("text"));
    if !text.is_empty() {
        return text;
    }
    module
        .get("rich_text_nodes")
        .and_then(Value::as_array)
        .map(|nodes| {
            nodes
                .iter()
                .filter_map(|node| node.get("text").and_then(Value::as_str))
                .collect::<String>()
        })
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn split_dynamic_text(text: &str) -> (String, String) {
    let title = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("官方最新动态")
        .to_string();
    let summary = text
        .strip_prefix(&title)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(text)
        .to_string();
    (title, summary)
}

fn module<'a>(modules: &'a Value, name: &str) -> Option<&'a Value> {
    modules
        .get(name)
        .or_else(|| modules.as_array()?.iter().find_map(|item| item.get(name)))
}

fn contains_lottery(value: &Value) -> bool {
    match value {
        Value::Object(fields) => fields.iter().any(|(key, value)| {
            let key_is_lottery =
                key.eq_ignore_ascii_case("lottery") || key.eq_ignore_ascii_case("lottery_id");
            let value_is_lottery = value.as_str().is_some_and(|text| {
                text.eq_ignore_ascii_case("RICH_TEXT_NODE_TYPE_LOTTERY")
                    || text.contains("抽奖")
                    || text.contains("中奖")
            });
            key_is_lottery || value_is_lottery || contains_lottery(value)
        }),
        Value::Array(values) => values.iter().any(contains_lottery),
        _ => false,
    }
}

/// 解析 B 站空间动态 API。接口可能因风控返回错误码，此时返回可展示的解析错误。
pub fn parse_bilibili_json(body: &str) -> Result<Vec<BulletinItem>> {
    let root: Value = serde_json::from_str(body)
        .map_err(|error| Error::Parse(format!("解析 B 站动态失败：{error}")))?;
    let code = root.get("code").and_then(Value::as_i64).unwrap_or(-1);
    if code != 0 {
        let message = value_text(root.get("message"));
        return Err(Error::Parse(format!("B 站接口返回 {code}: {message}")));
    }
    let Some(items) = root
        .get("data")
        .and_then(|data| data.get("items"))
        .and_then(Value::as_array)
    else {
        return Err(Error::Parse("B 站接口缺少 data.items".to_string()));
    };
    let mut output = Vec::new();
    for item in items {
        if contains_lottery(item) {
            continue;
        }
        let modules = item.get("modules").unwrap_or(&Value::Null);
        let dynamic = module(modules, "module_dynamic").unwrap_or(&Value::Null);
        let archive = dynamic
            .get("major")
            .and_then(|major| major.get("archive"))
            .or_else(|| dynamic.get("dyn_archive"));
        let description = module(modules, "module_desc")
            .map(module_text)
            .filter(|text| !text.is_empty())
            .or_else(|| dynamic.get("desc").map(module_text))
            .unwrap_or_default();
        let archive_title = value_text(archive.and_then(|value| value.get("title")));
        let archive_description = value_text(archive.and_then(|value| value.get("desc")));
        let (title, summary) = if !archive_title.is_empty() {
            (
                archive_title,
                if archive_description.is_empty() {
                    description
                } else {
                    archive_description
                },
            )
        } else {
            split_dynamic_text(&description)
        };
        let id = value_text(item.get("id_str"));
        let url = if id.is_empty() {
            BILIBILI_SPACE_URL.to_string()
        } else {
            format!("https://t.bilibili.com/{id}")
        };
        let image_url = archive
            .and_then(|v| v.get("cover"))
            .and_then(Value::as_str)
            .or_else(|| {
                dynamic
                    .get("major")
                    .and_then(|major| major.get("draw"))
                    .and_then(|draw| draw.get("items"))
                    .and_then(Value::as_array)
                    .and_then(|items| items.first())
                    .and_then(|image| image.get("src"))
                    .and_then(Value::as_str)
            })
            .or_else(|| {
                dynamic
                    .get("dyn_draw")
                    .and_then(|draw| draw.get("items"))
                    .and_then(Value::as_array)
                    .and_then(|items| items.first())
                    .and_then(|image| image.get("src"))
                    .and_then(Value::as_str)
            })
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let author = module(modules, "module_author").unwrap_or(&Value::Null);
        let published_at = {
            let value = value_text(author.get("pub_time"));
            if value.is_empty() {
                value_text(author.get("pub_text"))
            } else {
                value
            }
        };
        let timestamp = author.get("pub_ts").and_then(Value::as_i64).unwrap_or(0);
        output.push((
            timestamp,
            BulletinItem {
                kind: BulletinKind::Bilibili,
                title,
                summary,
                url,
                image_url,
                published_at,
                source: "B站官方动态".to_string(),
            },
        ));
    }
    output.sort_by_key(|item| std::cmp::Reverse(item.0));
    Ok(output.into_iter().take(5).map(|(_, item)| item).collect())
}

/// 国际服公告与新闻使用国际服官网, 不读取国服社区动态.
pub fn fetch_for(distribution: Distribution) -> BulletinFeed {
    if distribution == Distribution::Cn {
        return fetch_cn();
    }
    let origin = "https://nte.perfectworld.com";
    let mut feed = BulletinFeed::default();
    for (channel, kind) in [
        ("gamenews", BulletinKind::Preview),
        ("gamebroad", BulletinKind::Maintenance),
    ] {
        let url = format!("{origin}/en/article/news/{channel}/index.html");
        match fetch_text(&url, 1024 * 1024, Some(&url)) {
            Ok(html) => feed.items.extend(parse_official_list(
                &html,
                origin,
                &format!("/en/article/news/{channel}/"),
                kind,
            )),
            Err(error) => feed.errors.push(format!("国际服官方公告: {error}")),
        }
    }
    feed
}

/// 并发拉取三个来源，输出顺序固定为 B 站动态、最新前瞻、停服公告。
fn fetch_cn() -> BulletinFeed {
    let bilibili = std::thread::spawn(|| {
        fetch_text(BILIBILI_API_URL, 2 * 1024 * 1024, Some(BILIBILI_SPACE_URL))
            .and_then(|body| parse_bilibili_json(&body))
    });
    let preview = std::thread::spawn(|| {
        fetch_text(OFFICIAL_SWIPER_URL, 256 * 1024, Some(OFFICIAL_LAUNCHER_URL))
            .and_then(|body| parse_banner_script(&body))
    });
    let maintenance = std::thread::spawn(|| {
        fetch_text(
            OFFICIAL_ANNOUNCEMENT_URL,
            1024 * 1024,
            Some(OFFICIAL_ANNOUNCEMENT_URL),
        )
        .map(|body| parse_announcement_html(&body))
    });

    let mut feed = BulletinFeed::default();
    fn collect(
        feed: &mut BulletinFeed,
        label: &str,
        result: std::thread::Result<Result<Vec<BulletinItem>>>,
    ) {
        match result {
            Ok(Ok(items)) => feed.items.extend(items),
            Ok(Err(error)) => feed.errors.push(format!("{label}：{error}")),
            Err(_) => feed.errors.push(format!("{label}：抓取线程异常退出")),
        }
    }
    collect(&mut feed, "B站动态", bilibili.join());
    collect(&mut feed, "版本前瞻", preview.join());
    collect(&mut feed, "停服公告", maintenance.join());
    feed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_announcement_list_and_limits_to_three() {
        let html = r#"<div class="listNews"><a href="/news/gamebroad/20260101/a.html"><div class="listItem"><h2 class="title">第一条</h2><div class="des">摘要&nbsp;内容</div><p class="date">2026-01-01</p></div></a><a href="/news/gamebroad/20260102/b.html"><div class="title">第二条</div><p class="date">2026-01-02</p></a><a href="/news/gamebroad/20260103/c.html"><h2 class="title">第三条</h2><p class="date">2026-01-03</p></a><a href="/news/gamebroad/20260104/d.html"><h2 class="title">第四条</h2></a></div>"#;
        let items = parse_announcement_html(html);
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].title, "第一条");
        assert_eq!(items[0].summary, "摘要 内容");
        assert_eq!(
            items[0].url,
            "https://yh.wanmei.com/news/gamebroad/20260101/a.html"
        );
    }

    #[test]
    fn global_announcements_keep_global_links_and_ignore_cn_entries() {
        let html = r#"<a href="/en/article/news/gamebroad/20261019/264665.html"><h2 class="title">Account Action Notice</h2><p class="date">2026-10-19</p></a><a href="https://yh.wanmei.com/news/gamebroad/a.html"><h2 class="title">国服公告</h2></a>"#;
        let items = parse_official_list(
            html,
            "https://nte.perfectworld.com",
            "/en/article/news/gamebroad/",
            BulletinKind::Maintenance,
        );
        assert_eq!(items.len(), 1);
        assert_eq!(
            items[0].url,
            "https://nte.perfectworld.com/en/article/news/gamebroad/20261019/264665.html"
        );
        assert_eq!(items[0].published_at, "2026-10-19");
    }

    #[test]
    fn parses_only_latest_banner() {
        let script = r#"var yh_data_data = {"lb1":[{"title":"最新前瞻","bigpic":"https://img.test/a.jpg","link":"https://yh.wanmei.com/a.html"},{"title":"旧前瞻","bigpic":"","link":"https://yh.wanmei.com/b.html"}]};"#;
        let items = parse_banner_script(script).expect("banner");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "最新前瞻");
    }

    #[test]
    fn parses_bilibili_archive_and_text_dynamic() {
        let body = r#"{"code":0,"data":{"items":[{"id_str":"123","modules":{"module_author":{"pub_time":"刚刚"},"module_dynamic":{"desc":{"text":"官方文字动态"}}}},{"id_str":"456","modules":{"module_author":{"pub_time":"昨天"},"module_dynamic":{"desc":{"text":"视频说明"},"major":{"archive":{"title":"视频标题","cover":"https://img.test/v.jpg"}}}}}]}}"#;
        let items = parse_bilibili_json(body).expect("dynamic");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].title, "官方文字动态");
        assert_eq!(items[0].summary, "官方文字动态");
        assert_eq!(items[1].title, "视频标题");
        assert_eq!(items[1].summary, "视频说明");
        assert_eq!(
            items[1].image_url.as_deref(),
            Some("https://img.test/v.jpg")
        );
    }

    #[test]
    fn parses_desktop_bilibili_module_arrays() {
        let body = r#"{"code":0,"data":{"items":[{"id_str":"789","modules":[{"module_author":{"pub_time":"今天"}},{"module_dynamic":{"desc":{"text":"桌面接口动态"}}}]}]}}"#;
        let items = parse_bilibili_json(body).expect("desktop dynamic");
        assert_eq!(items[0].title, "桌面接口动态");
        assert_eq!(items[0].summary, "桌面接口动态");
        assert_eq!(items[0].published_at, "今天");
    }

    #[test]
    fn parses_desktop_archive_fields() {
        let body = r#"{"code":0,"data":{"items":[{"id_str":"789","modules":[{"module_author":{"pub_text":"10月15日 · 投稿了视频"}},{"module_dynamic":{"dyn_archive":{"title":"视频标题","desc":"视频说明","cover":"https://img.test/v.jpg"}}}]}]}}"#;
        let items = parse_bilibili_json(body).expect("desktop archive");
        assert_eq!(items[0].title, "视频标题");
        assert_eq!(items[0].summary, "视频说明");
        assert_eq!(items[0].published_at, "10月15日 · 投稿了视频");
        assert_eq!(
            items[0].image_url.as_deref(),
            Some("https://img.test/v.jpg")
        );
    }

    #[test]
    fn filters_lottery_dynamics() {
        let body = r#"{"code":0,"data":{"items":[{"id_str":"1260576105436282904","type":"DYNAMIC_TYPE_FORWARD","modules":[{"module_desc":{"rich_text_nodes":[{"type":"RICH_TEXT_NODE_TYPE_LOTTERY","text":"互动抽奖"}]}},{"module_author":{"pub_text":"3天前"}}]},{"id_str":"789","modules":[{"module_author":{"pub_text":"今天"}},{"module_dynamic":{"desc":{"text":"普通动态"}}}]}]}}"#;
        let items = parse_bilibili_json(body).expect("dynamic");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].url, "https://t.bilibili.com/789");
    }

    #[test]
    fn parses_desktop_text_module_for_title_and_content() {
        let body = r##"{"code":0,"data":{"items":[{"id_str":"789","modules":[{"module_author":{"pub_text":"今天"}},{"module_desc":{"text":"#异环#\n感谢鉴定师们的一路同行~"}},{"module_dynamic":{"dyn_draw":{"items":[{"src":"https://img.test/draw.jpg"}]}}}]}]}}"##;
        let items = parse_bilibili_json(body).expect("dynamic");
        assert_eq!(items[0].title, "#异环#");
        assert_eq!(items[0].summary, "感谢鉴定师们的一路同行~");
        assert_eq!(
            items[0].image_url.as_deref(),
            Some("https://img.test/draw.jpg")
        );
    }
}
