use std::sync::LazyLock;

use regex::{Captures, Regex};
use serde::Serialize;

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Redactions {
    pub home: usize,
    pub phone: usize,
    pub credentials: usize,
    pub jwt: usize,
}

impl Redactions {
    pub(super) fn total(&self) -> usize {
        self.home + self.phone + self.credentials + self.jwt
    }
}

// 只按明确字段识别凭据, 保留资源哈希和普通标识符.
static HEADERS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?im)(\b(?:authorization|proxy-authorization|cookie|set-cookie)[ \t]*[:=][ \t]*)[^\r\n]+"#).unwrap()
});
const FIELD_NAMES: &str = r"(?:access|refresh|laohu|id|auth|session|user)[_-]?token|token|password|passwd|pwd|secret|client[_-]?secret|api[_-]?key|authorization|cookie";
static ESCAPED_FIELDS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r#"(?ix)(\b(?:{FIELD_NAMES})\\["'][ \t]*[:=][ \t]*\\["'])[^\r\n]+"#
    ))
    .unwrap()
});
static FIELDS: LazyLock<Regex> = LazyLock::new(|| {
    // 正在写入的末行可能缺少结束引号, 仍移除已经写出的凭据.
    Regex::new(&format!(r#"(?ix)(\b(?:{FIELD_NAMES})\b["']?[ \t]*[:=][ \t]*)(?:"(?:\\.|[^"\\\r\n])*\\?"?|'(?:\\.|[^'\\\r\n])*\\?'?|[^\s,;&"'{{}}\[\]]+)"#)).unwrap()
});
pub(super) static PENDING_FIELD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r#"(?ix)\b(?:{FIELD_NAMES})\b["']?[ \t]*[:=][ \t]*$"#
    ))
    .unwrap()
});
static JWT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\beyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\b").unwrap());
static PHONE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"1[3-9][0-9]{9}").unwrap());

pub(super) struct Redactor {
    homes: Vec<String>,
    pub counts: Redactions,
}

impl Redactor {
    pub fn new(home: Option<&str>) -> Self {
        let mut homes = Vec::new();
        if let Some(home) = home.filter(|home| home.starts_with('/') && *home != "/") {
            let home = home.trim_end_matches('/');
            for separator in ["\\\\", "\\", "/"] {
                let path = home.replace('/', separator);
                homes.push(format!("Z:{path}"));
                homes.push(format!("z:{path}"));
                homes.push(path);
            }
        }
        Self {
            homes,
            counts: Redactions::default(),
        }
    }

    pub fn text(&mut self, text: &str) -> String {
        let text = HEADERS.replace_all(text, |caps: &Captures<'_>| {
            self.counts.credentials += 1;
            format!("{}[凭据已移除]", &caps[1])
        });
        let text = ESCAPED_FIELDS.replace_all(&text, |caps: &Captures<'_>| {
            self.counts.credentials += 1;
            format!("{}[凭据已移除]", &caps[1])
        });
        let text = FIELDS.replace_all(&text, |caps: &Captures<'_>| {
            self.counts.credentials += 1;
            format!("{}[凭据已移除]", &caps[1])
        });
        let mut text = JWT
            .replace_all(&text, |_: &Captures<'_>| {
                self.counts.jwt += 1;
                "[令牌已移除]"
            })
            .into_owned();
        for home in &self.homes {
            let mut result = String::new();
            let mut copied = 0;
            for (start, _) in text.match_indices(home) {
                let end = start + home.len();
                let before = text[..start].chars().next_back();
                let after = text[end..].chars().next();
                // 防止把 /home/alice2 或路径中间的同名片段当成主目录.
                if (before.is_some_and(|c| c.is_alphanumeric() || matches!(c, '_' | '/' | '\\'))
                    && !text[..start].ends_with("file://"))
                    || after.is_some_and(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.'))
                {
                    continue;
                }
                result.push_str(&text[copied..start]);
                result.push_str("$HOME");
                copied = end;
                self.counts.home += 1;
            }
            result.push_str(&text[copied..]);
            text = result;
        }
        let mut result = String::new();
        let mut copied = 0;
        for found in PHONE.find_iter(&text) {
            if text[..found.start()]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
                || text[found.end()..]
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
            {
                continue;
            }
            result.push_str(&text[copied..found.start()]);
            result.push_str("[手机号已移除]");
            copied = found.end();
            self.counts.phone += 1;
        }
        result.push_str(&text[copied..]);
        result
    }
}
