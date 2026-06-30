use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    English,
    Chinese,
}

impl Language {
    pub fn detect() -> Self {
        if let Ok(locale) = std::env::var("ICODING_LANG")
            && !locale.trim().is_empty()
        {
            return Self::from_locale(&locale);
        }

        for key in ["LC_ALL", "LC_MESSAGES", "LANG"] {
            if let Ok(locale) = std::env::var(key)
                && !locale.trim().is_empty()
                && !is_neutral_locale(&locale)
            {
                return Self::from_locale(&locale);
            }
        }

        system_locale()
            .as_deref()
            .map(Self::from_locale)
            .unwrap_or(Self::English)
    }

    pub fn from_locale(locale: &str) -> Self {
        let locale = locale.trim().to_ascii_lowercase();
        if locale == "zh" || locale.starts_with("zh-") || locale.starts_with("zh_") {
            Self::Chinese
        } else {
            Self::English
        }
    }

    pub fn select(self, english: &'static str, chinese: &'static str) -> &'static str {
        match self {
            Self::English => english,
            Self::Chinese => chinese,
        }
    }
}

fn is_neutral_locale(locale: &str) -> bool {
    matches!(
        locale.trim().to_ascii_lowercase().as_str(),
        "c" | "posix" | "c.utf-8" | "c.utf8"
    )
}

#[cfg(target_os = "macos")]
fn system_locale() -> Option<String> {
    command_output("/usr/bin/defaults", &["read", "-g", "AppleLocale"])
}

#[cfg(target_os = "windows")]
fn system_locale() -> Option<String> {
    command_output(
        "powershell.exe",
        &["-NoProfile", "-Command", "(Get-Culture).Name"],
    )
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn system_locale() -> Option<String> {
    command_output("locale", &["charmap"])
}

fn command_output(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8(output.stdout).ok()?;
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_chinese_locale_variants() {
        assert_eq!(Language::from_locale("zh-CN"), Language::Chinese);
        assert_eq!(Language::from_locale("zh_TW.UTF-8"), Language::Chinese);
    }

    #[test]
    fn defaults_non_chinese_locales_to_english() {
        assert_eq!(Language::from_locale("en-US"), Language::English);
        assert_eq!(Language::from_locale("C"), Language::English);
        assert_eq!(Language::from_locale(""), Language::English);
    }
}
