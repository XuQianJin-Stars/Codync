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
pub(in crate::tui) fn tr<'a>(text: &'a str) -> Cow<'a, str> {
    translate(text, uses_chinese())
}

fn translate<'a>(text: &'a str, chinese: bool) -> Cow<'a, str> {
    if !chinese {
        return Cow::Borrowed(text);
    }
    PHRASES.get(text).map_or(Cow::Borrowed(text), |hit| Cow::Owned(hit.clone()))
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
