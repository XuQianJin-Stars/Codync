//! This terminal's interface language. The phrase table is the one the phone apps ship.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use crate::LockExt;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum AppLanguage {
    System,
    Zh,
    En,
}

impl AppLanguage {
    fn parse(raw: &str) -> Self {
        match raw {
            "zh" => Self::Zh,
            "en" => Self::En,
            _ => Self::System,
        }
    }

    fn stored(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Zh => "zh",
            Self::En => "en",
        }
    }

    fn next(self) -> Self {
        match self {
            Self::System => Self::Zh,
            Self::Zh => Self::En,
            Self::En => Self::System,
        }
    }
}

static PHRASES: LazyLock<HashMap<String, String>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../apps/ios/Kit/Sources/CodyncKit/Resources/zh-Hans.json"))
        .expect("zh-Hans.json is a string map")
});

static PREFERENCE: LazyLock<Mutex<AppLanguage>> = LazyLock::new(|| Mutex::new(read_preference()));

fn read_preference() -> AppLanguage {
    std::fs::read_to_string(crate::service::data_dir().join("tui-language"))
        .map_or(AppLanguage::System, |text| AppLanguage::parse(text.trim()))
}

fn system_is_chinese() -> bool {
    std::env::var("LC_ALL")
        .or_else(|_| std::env::var("LC_MESSAGES"))
        .or_else(|_| std::env::var("LANG"))
        .is_ok_and(|value| value.to_ascii_lowercase().starts_with("zh"))
}

fn uses_chinese() -> bool {
    match *PREFERENCE.locked() {
        AppLanguage::Zh => true,
        AppLanguage::En => false,
        AppLanguage::System => system_is_chinese(),
    }
}

/// "resets 3h 20m" / "resets in 2d 6h". Compact times become "3小时20分后重置".
pub(in crate::tui) fn reset_line(raw: &str) -> String {
    let Some(time) = raw.strip_prefix("resets in ").or_else(|| raw.strip_prefix("resets ")) else {
        return tr(raw).into_owned();
    };
    if !uses_chinese() {
        return raw.to_owned();
    }
    match compact_time(time) {
        Some(spoken) => format!("{spoken}{}", tr("after reset")),
        None => format!("{} {time}", tr("resets")),
    }
}

/// "3h 20m", "2d 6h", "45m". A clock time stays `None`.
fn compact_time(time: &str) -> Option<String> {
    let mut spoken = String::new();
    let mut matched = false;
    for token in time.split(' ') {
        let mut chars = token.chars();
        let unit = chars.next_back()?;
        let n: u32 = chars.as_str().parse().ok()?;
        let word = match unit {
            'd' => translate("day unit", true),
            'h' => translate("hour unit", true),
            'm' => translate("minute unit", true),
            _ => return None,
        };
        matched = true;
        if n == 0 {
            continue;
        }
        spoken.push_str(&n.to_string());
        spoken.push_str(word.as_ref());
    }
    if !matched {
        return None;
    }
    if spoken.is_empty() {
        spoken = format!("0{}", translate("minute unit", true));
    }
    Some(spoken)
}

/// An exact chrome phrase. Chat text that is not one of those phrases stays as written.
pub(in crate::tui) fn tr(text: &str) -> Cow<'_, str> {
    translate(text, uses_chinese())
}

/// One piece of a phrase key such as "Runs the official installer on {name}."
struct Piece {
    literal: Option<String>,
    token: Option<String>,
}

struct Template {
    key: String,
    pieces: Vec<Piece>,
    literal: usize,
}

/// Phrase keys that already contain `{token}` slots, longest fixed text first.
static TEMPLATES: LazyLock<Vec<Template>> = LazyLock::new(|| {
    let mut out = Vec::new();
    for key in PHRASES.keys() {
        let Some(pieces) = parse_template(key) else { continue };
        let literal: usize = pieces.iter().map(|piece| piece.literal.as_ref().map_or(0, String::len)).sum();
        if literal < 6 {
            continue;
        }
        out.push(Template { key: key.clone(), pieces, literal });
    }
    out.sort_by(|a, b| b.literal.cmp(&a.literal).then(b.key.len().cmp(&a.key.len())));
    out
});

/// `{name}` and the same shape. A `{` that is not a slot makes the key unusable as a template.
fn parse_template(template: &str) -> Option<Vec<Piece>> {
    let mut pieces = Vec::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        if open > 0 {
            pieces.push(Piece { literal: Some(rest[..open].to_owned()), token: None });
        }
        let close = rest[open..].find('}')?;
        let token = &rest[open..=open + close];
        let name = &token[1..token.len() - 1];
        if name.is_empty()
            || !name.chars().all(|c| c.is_ascii_alphanumeric())
            || pieces.last().is_some_and(|piece| piece.token.is_some())
        {
            return None;
        }
        pieces.push(Piece { literal: None, token: Some(token.to_owned()) });
        rest = &rest[open + close + 1..];
    }
    if !rest.is_empty() {
        pieces.push(Piece { literal: Some(rest.to_owned()), token: None });
    }
    pieces.iter().any(|piece| piece.token.is_some()).then_some(pieces)
}

/// A sentence the host or a view already filled in, matched back to its `{token}` phrase.
fn templated(text: &str) -> Option<String> {
    templated_at(text, 0)
}

fn templated_at(text: &str, depth: u8) -> Option<String> {
    for template in TEMPLATES.iter() {
        let Some(caps) = captures(text, &template.pieces) else { continue };
        let Some(phrase) = PHRASES.get(&template.key) else { continue };
        let mut result = phrase.clone();
        for (token, value) in caps {
            let shown = if let Some(hit) = PHRASES.get(&value) {
                hit.clone()
            } else if depth == 0 {
                templated_at(&value, 1).unwrap_or(value)
            } else {
                value
            };
            result = result.replace(&token, &shown);
        }
        return Some(result);
    }
    None
}

fn captures(text: &str, pieces: &[Piece]) -> Option<Vec<(String, String)>> {
    let mut pos = 0;
    let mut caps = Vec::new();
    for (index, piece) in pieces.iter().enumerate() {
        if let Some(literal) = &piece.literal {
            if !text[pos..].starts_with(literal) {
                return None;
            }
            pos += literal.len();
            continue;
        }
        let token = piece.token.as_deref()?;
        let end = if let Some(next) = pieces.get(index + 1).and_then(|next| next.literal.as_deref()) {
            let rel = text[pos..].find(next)?;
            if rel == 0 {
                return None;
            }
            pos + rel
        } else if pos >= text.len() {
            return None;
        } else {
            text.len()
        };
        caps.push((token.to_owned(), text[pos..end].to_owned()));
        pos = end;
    }
    (pos == text.len()).then_some(caps)
}

fn translate(text: &str, chinese: bool) -> Cow<'_, str> {
    if !chinese {
        return Cow::Borrowed(text);
    }
    if let Some(hit) = PHRASES.get(text).cloned().or_else(|| templated(text)) {
        return Cow::Owned(hit);
    }
    let trimmed = text.trim();
    if trimmed == text {
        return Cow::Borrowed(text);
    }
    PHRASES.get(trimmed).cloned().or_else(|| templated(trimmed)).map_or(Cow::Borrowed(text), Cow::Owned)
}

/// The choice shown beside the palette row.
pub(in crate::tui) fn language_title() -> Cow<'static, str> {
    match *PREFERENCE.locked() {
        AppLanguage::Zh => Cow::Borrowed("中文"),
        AppLanguage::En => Cow::Borrowed("English"),
        AppLanguage::System => translate("Follow system", uses_chinese()),
    }
}

/// Follow system, then Chinese, then English, then back. The choice is kept for the next run.
pub(in crate::tui) fn cycle_language() -> Cow<'static, str> {
    let next = PREFERENCE.locked().next();
    *PREFERENCE.locked() = next;
    let dir = crate::service::data_dir();
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(dir.join("tui-language"), next.stored());
    language_title()
}

#[cfg(test)]
mod tests {
    use super::{AppLanguage, translate};

    #[test]
    fn chinese_replaces_a_known_phrase_and_leaves_the_rest() {
        assert_eq!(translate("Pair a computer", true).as_ref(), "配对一台电脑");
        assert_eq!(translate("Pair a computer", false).as_ref(), "Pair a computer");
        assert_eq!(translate("Not a phrase", true).as_ref(), "Not a phrase");
        assert_eq!(translate("Follow system", true).as_ref(), "跟随系统");
        assert_eq!(translate("Writing a reply…", true).as_ref(), "正在写回复…");
        assert_eq!(translate("Downloading Codebuddy Code…", true).as_ref(), "正在下载 Codebuddy Code…");
        assert_eq!(translate("Needs approval: Bash", true).as_ref(), "需要批准：Bash");
        assert_eq!(translate("Storage", true).as_ref(), "存储");
        assert_eq!(translate("New skill", true).as_ref(), "新建技能");
        assert_eq!(translate("Add connector", true).as_ref(), "添加连接器");
        assert_eq!(translate("Name your group", true).as_ref(), "给群聊起个名字");
        assert_eq!(translate("Nothing installed", true).as_ref(), "还没有安装");
        assert_eq!(translate("No setup needed.", true).as_ref(), "无需设置。");
        assert_eq!(
            translate("Runs the official installer on this Mac.", true).as_ref(),
            "在 这台 Mac 上运行官方安装程序。"
        );
        assert_eq!(
            translate(
                "Claude Code isn't set up on this computer. Install Claude Code and run `claude auth login`.",
                true
            )
            .as_ref(),
            "Claude Code 还没有在这台电脑上设置好。安装 Claude Code，然后运行 `claude auth login`。"
        );
        assert_eq!(super::compact_time("3h 20m").as_deref(), Some("3小时20分"));
        assert_eq!(super::compact_time("2d 6h").as_deref(), Some("2天6小时"));
        assert_eq!(super::compact_time("Sep 26"), None);
    }

    #[test]
    fn a_missing_choice_follows_the_computer() {
        assert_eq!(AppLanguage::parse(""), AppLanguage::System);
        assert_eq!(AppLanguage::parse("zh"), AppLanguage::Zh);
        assert_eq!(AppLanguage::parse("en"), AppLanguage::En);
        assert_eq!(AppLanguage::System.next(), AppLanguage::Zh);
        assert_eq!(AppLanguage::En.next(), AppLanguage::System);
    }
}
