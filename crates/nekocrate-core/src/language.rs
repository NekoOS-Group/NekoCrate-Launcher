pub fn system_locale() -> String {
    sys_locale::get_locale().unwrap_or_else(|| "en-US".into())
}

/// Only Simplified Chinese locales use Chinese; every other system locale uses English.
pub fn resolve(preference: &str, system: &str) -> &'static str {
    match preference {
        "zh-CN" => "zh-CN",
        "en-US" => "en-US",
        _ => {
            let normalized = system.replace('_', "-").to_ascii_lowercase();
            let parts: Vec<_> = normalized.split('-').collect();
            if parts.first() == Some(&"zh")
                && (parts.contains(&"hans")
                    || (!parts.contains(&"hant")
                        && !parts.iter().any(|p| ["tw", "hk", "mo"].contains(p))))
            {
                "zh-CN"
            } else {
                "en-US"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_language_falls_back_to_english_except_simplified_chinese() {
        for system in ["zh", "zh-CN", "zh_SG", "zh-Hans", "zh-Hans-HK"] {
            assert_eq!(resolve("system", system), "zh-CN");
            assert_eq!(resolve("en-US", system), "en-US");
        }
        for system in [
            "",
            "en-US",
            "de-DE",
            "ja-JP",
            "zh-TW",
            "zh-HK",
            "zh-MO",
            "zh-Hant-CN",
        ] {
            assert_eq!(resolve("system", system), "en-US");
            assert_eq!(resolve("zh-CN", system), "zh-CN");
        }
    }
}
