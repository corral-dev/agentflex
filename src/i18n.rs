//! User-facing text. The terminal UI follows the process locale: Simplified
//! Chinese when it is a Chinese locale, English for every other language.
//!
//! Product terms stay in English on purpose (owner decision 2026-11-08):
//! `CODING WEEK`, `tokens`, and `AGENTS` read as the card's own vocabulary.
//! Everything else is translated in the way Chinese developer tools actually
//! phrase it, not word for word.

use std::env;

use crate::stats::Period;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    En,
    Zh,
}

impl Lang {
    /// `LC_ALL`, `LC_MESSAGES`, `LANG`, `LANGUAGE`, then the system language.
    /// An explicit `LANG=zh_CN.UTF-8` therefore forces Chinese even when the
    /// OS language is English, which also makes the localized card easy to
    /// inspect.
    pub fn detect() -> Lang {
        for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
            if let Some(value) = env::var_os(var) {
                if let Some(lang) = parse_locale(&value.to_string_lossy()) {
                    return lang;
                }
            }
        }
        if let Some(value) = env::var_os("LANGUAGE") {
            for part in value.to_string_lossy().split(':') {
                if let Some(lang) = parse_locale(part) {
                    return lang;
                }
            }
        }
        sys_locale::get_locale().as_deref().and_then(parse_locale).unwrap_or(Lang::En)
    }
}

/// Read the language subtag of a POSIX or BCP-47 tag. `C`/`POSIX` and empty
/// values mean "not set", so detection can continue with the system locale;
/// any other language we do not translate falls back to English.
fn parse_locale(tag: &str) -> Option<Lang> {
    let tag = tag.trim();
    if tag.is_empty() {
        return None;
    }
    let base = tag.split(['.', '@']).next().unwrap_or(tag);
    let language = base.split(['_', '-']).next().unwrap_or(base).to_ascii_lowercase();
    match language.as_str() {
        "c" | "posix" => None,
        "zh" => Some(Lang::Zh),
        "en" => Some(Lang::En),
        _ if !language.is_empty() && language.bytes().all(|b| b.is_ascii_alphabetic()) => Some(Lang::En),
        _ => None,
    }
}

/// (label, value, compact value, detail) stat columns, in card order.
pub fn stat_labels(lang: Lang) -> [&'static str; 3] {
    match lang {
        Lang::En => ["SESSIONS", "TOKENS / DAY", "PEAK PARALLEL"],
        Lang::Zh => ["会话数", "日均 tokens", "峰值并发"],
    }
}

pub fn agents_detail(lang: Lang, agents: usize) -> String {
    match lang {
        Lang::En => format!("{agents} agent{}", if agents == 1 { "" } else { "s" }),
        Lang::Zh => format!("{agents} 个 agent"),
    }
}

pub const AGENTS_LABEL: &str = "AGENTS";

pub fn models_label(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "MODELS",
        Lang::Zh => "模型",
    }
}

/// Model legend item for everything outside the top three.
pub fn other_model(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "other",
        Lang::Zh => "其他",
    }
}

pub fn other_runtime(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "Other",
        Lang::Zh => "其他",
    }
}

pub fn unknown_model(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "unknown",
        Lang::Zh => "未知",
    }
}

pub fn activity_label(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "ACTIVITY",
        Lang::Zh => "活跃时段",
    }
}

pub fn busiest_label(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "busiest",
        Lang::Zh => "最忙",
    }
}

/// Weekday names for the heatmap (Monday first) and the peak detail.
pub fn weekday(lang: Lang, monday_index: usize) -> &'static str {
    match lang {
        Lang::En => ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"][monday_index % 7],
        Lang::Zh => ["周一", "周二", "周三", "周四", "周五", "周六", "周日"][monday_index % 7],
    }
}

/// Two hero lines for a period without any session. The English text names
/// the period; Chinese relies on the period label already on the title row.
pub fn empty_lines(lang: Lang, period: Period) -> [String; 2] {
    match lang {
        Lang::En => [
            "No Claude Code, Codex, OpenCode, Pi or Antigravity".to_string(),
            match period {
                Period::Week => "sessions found in the last 7 days.",
                Period::Month => "sessions found in the last 30 days.",
                Period::All => "sessions found anywhere in your history.",
            }
            .to_string(),
        ],
        Lang::Zh => ["没有找到 Claude Code、Codex、OpenCode、".to_string(), "Pi 或 Antigravity 的会话记录。".to_string()],
    }
}

pub fn scanning_history(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "scanning history",
        Lang::Zh => "正在扫描历史记录",
    }
}

pub fn reading_sessions(lang: Lang, done: &str, total: &str) -> String {
    match lang {
        Lang::En => format!("reading {done} / {total} sessions"),
        Lang::Zh => format!("正在读取 {done} / {total} 个会话"),
    }
}

/// Key hints under the card: key in gold, label dim.
pub fn keys(lang: Lang) -> [(&'static str, &'static str); 3] {
    match lang {
        Lang::En => [("s", "save image"), ("r", "replay"), ("q", "quit")],
        Lang::Zh => [("s", "保存图片"), ("r", "重播"), ("q", "退出")],
    }
}

pub fn saved(lang: Lang, path: &str) -> String {
    match lang {
        Lang::En => format!("Saved {path}"),
        Lang::Zh => format!("已保存到 {path}"),
    }
}

pub fn save_failed(lang: Lang, error: &str) -> String {
    match lang {
        Lang::En => format!("Could not save the image: {error}"),
        Lang::Zh => format!("图片保存失败：{error}"),
    }
}

pub fn small_terminal(lang: Lang, w: usize, h: usize) -> String {
    match lang {
        Lang::En => format!("Enlarge the terminal to at least {w}x{h} to see the card"),
        Lang::Zh => format!("请把终端窗口调大到至少 {w}x{h}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_tags_pick_chinese_or_english() {
        assert_eq!(parse_locale("zh_CN.UTF-8"), Some(Lang::Zh));
        assert_eq!(parse_locale("zh-Hans-CN"), Some(Lang::Zh));
        assert_eq!(parse_locale("en_US.UTF-8"), Some(Lang::En));
        assert_eq!(parse_locale("fr_FR.UTF-8"), Some(Lang::En));
        assert_eq!(parse_locale("C.UTF-8"), None);
        assert_eq!(parse_locale("POSIX"), None);
        assert_eq!(parse_locale(""), None);
    }

    #[test]
    fn chinese_copy_is_complete() {
        assert_eq!(stat_labels(Lang::Zh), ["会话数", "日均 tokens", "峰值并发"]);
        assert_eq!(keys(Lang::Zh).map(|k| k.1), ["保存图片", "重播", "退出"]);
        assert_eq!(weekday(Lang::Zh, 1), "周二");
        assert_eq!(empty_lines(Lang::Zh, Period::Week).len(), 2);
    }
}
