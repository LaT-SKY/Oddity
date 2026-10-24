//! 本地背景素材服务。
//!
//! WebKitGTK 对 blob 视频的取帧会周期性停滞, 这里用只监听回环地址的
//! HTTP 服务按字节范围提供官方素材, 前端直接把 URL 交给 video/img。

use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Default)]
struct Files {
    token: String,
    generation: u64,
    video: Option<PathBuf>,
    image: Option<PathBuf>,
}

/// 只服务当前解析到的背景文件。
pub struct BackgroundServer {
    port: u16,
    files: Arc<Mutex<Files>>,
}

impl BackgroundServer {
    pub fn start() -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("绑定背景素材服务监听端口失败");
        let port = listener
            .local_addr()
            .expect("读取背景素材服务端口失败")
            .port();
        let files = Arc::new(Mutex::new(Files {
            token: make_token(),
            ..Default::default()
        }));
        let shared = Arc::clone(&files);
        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let files = Arc::clone(&shared);
                thread::spawn(move || {
                    let _ = serve(stream, &files);
                });
            }
        });
        Self { port, files }
    }

    /// 更新要提供的文件, 返回带令牌和版本号的 URL。
    pub fn update(
        &self,
        video: Option<PathBuf>,
        image: Option<PathBuf>,
    ) -> (Option<String>, Option<String>) {
        let mut files = self.files.lock().unwrap_or_else(|error| error.into_inner());
        files.video = video;
        files.image = image;
        files.generation = files.generation.wrapping_add(1);
        let token = files.token.clone();
        let generation = files.generation;
        let base = format!("http://127.0.0.1:{}", self.port);
        let url = |name: &str| format!("{base}/{name}?token={token}&v={generation}");
        (
            files.video.as_ref().map(|_| url("video")),
            files.image.as_ref().map(|_| url("image")),
        )
    }
}

fn make_token() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("{:x}{:x}", std::process::id(), nanos)
}

fn serve(mut stream: TcpStream, files: &Arc<Mutex<Files>>) -> io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request = String::new();
    if reader.read_line(&mut request)? == 0 {
        return Ok(());
    }
    let mut parts = request.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let target = parts.next().unwrap_or("").to_string();

    let mut range = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 || line == "\r\n" || line == "\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("range") {
                range = Some(value.trim().to_string());
            }
        }
    }

    if method != "GET" && method != "HEAD" {
        return write_text(&mut stream, 405, "Method Not Allowed", "Method Not Allowed");
    }

    let (path, query) = target.split_once('?').unwrap_or((target.as_str(), ""));
    let (file, token) = {
        let files = files.lock().unwrap_or_else(|error| error.into_inner());
        let file = match path {
            "/video" => files.video.clone(),
            "/image" => files.image.clone(),
            _ => None,
        };
        (file, files.token.clone())
    };
    if !query_has_token(query, &token) {
        return write_text(&mut stream, 403, "Forbidden", "Forbidden");
    }
    let Some(path) = file else {
        return write_text(&mut stream, 404, "Not Found", "Not Found");
    };
    let length = std::fs::metadata(&path)?.len();
    let mime = mime_for(&path);
    let has_range = range.is_some();
    let (start, end) = match range.as_deref() {
        Some(value) => match parse_range(value, length) {
            Some(range) => range,
            None => {
                return write_text(
                    &mut stream,
                    416,
                    "Range Not Satisfiable",
                    "Range Not Satisfiable",
                )
            }
        },
        None => (0, length.saturating_sub(1)),
    };
    let status = if has_range { 206 } else { 200 };
    let count = end.saturating_sub(start).saturating_add(1);
    write!(
        stream,
        "HTTP/1.1 {status} {}\r\nContent-Type: {mime}\r\nContent-Length: {count}\r\nAccept-Ranges: bytes\r\nAccess-Control-Allow-Origin: *\r\nCache-Control: no-store\r\nConnection: close\r\n",
        if status == 206 { "Partial Content" } else { "OK" }
    )?;
    if status == 206 {
        write!(stream, "Content-Range: bytes {start}-{end}/{length}\r\n")?;
    }
    write!(stream, "\r\n")?;
    if method == "HEAD" {
        return stream.flush();
    }
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(start))?;
    io::copy(&mut file.take(count), &mut stream)?;
    stream.flush()
}

fn query_has_token(query: &str, token: &str) -> bool {
    query
        .split('&')
        .filter_map(|part| part.strip_prefix("token="))
        .any(|value| value == token)
}

fn parse_range(value: &str, length: u64) -> Option<(u64, u64)> {
    if length == 0 {
        return None;
    }
    let spec = value.strip_prefix("bytes=")?.split(',').next()?.trim();
    let (start, end) = spec.split_once('-')?;
    let (start, end) = if start.is_empty() {
        let suffix: u64 = end.parse().ok()?;
        (length.saturating_sub(suffix), length - 1)
    } else {
        let start: u64 = start.parse().ok()?;
        let end = if end.is_empty() {
            length - 1
        } else {
            end.parse().ok()?
        };
        (start, end)
    };
    (start < length && end >= start).then_some((start, end.min(length - 1)))
}

fn mime_for(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .as_deref()
    {
        Some("mp4") => "video/mp4",
        Some("webm") => "video/webm",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        _ => "application/octet-stream",
    }
}

fn write_text(stream: &mut TcpStream, status: u16, reason: &str, body: &str) -> io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )?;
    stream.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::TempDir;

    fn url_query(url: &str) -> String {
        url.split_once('?').expect("query").1.to_string()
    }

    #[test]
    fn parses_byte_ranges() {
        assert_eq!(parse_range("bytes=2-5", 10), Some((2, 5)));
        assert_eq!(parse_range("bytes=2-", 10), Some((2, 9)));
        assert_eq!(parse_range("bytes=-3", 10), Some((7, 9)));
        assert_eq!(parse_range("bytes=8-20", 10), Some((8, 9)));
        assert_eq!(parse_range("bytes=20-", 10), None);
        assert_eq!(parse_range("items=0-1", 10), None);
        assert_eq!(parse_range("bytes=0-1", 0), None);
    }

    #[test]
    fn serves_ranges_and_checks_the_token() {
        let dir = TempDir::new("background-server");
        let file = dir.write("bg.mp4", b"0123456789");
        let server = BackgroundServer::start();
        let (video, image) = server.update(Some(file), None);
        assert!(image.is_none());
        let url = video.expect("video url");
        let query = url_query(&url);
        let port = url
            .strip_prefix("http://127.0.0.1:")
            .and_then(|rest| rest.split('/').next())
            .and_then(|port| port.parse::<u16>().ok())
            .expect("port");

        let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
        write!(
            stream,
            "GET /video?{query} HTTP/1.1\r\nHost: 127.0.0.1\r\nRange: bytes=2-5\r\n\r\n"
        )
        .expect("request");
        let mut response = String::new();
        stream.read_to_string(&mut response).expect("response");
        assert!(response.starts_with("HTTP/1.1 206"), "{response}");
        assert!(
            response.contains("Content-Range: bytes 2-5/10"),
            "{response}"
        );
        assert!(response.ends_with("2345"), "{response}");

        let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
        write!(
            stream,
            "GET /video?token=wrong HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n"
        )
        .expect("request");
        let mut response = String::new();
        stream.read_to_string(&mut response).expect("response");
        assert!(response.starts_with("HTTP/1.1 403"), "{response}");
    }
}
