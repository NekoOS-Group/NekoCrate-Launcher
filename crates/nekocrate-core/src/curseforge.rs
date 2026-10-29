//! Application credential supplied by the release environment, never by the UI.
use anyhow::{Context, Result};

pub(crate) const UNAVAILABLE: &str = "此构建暂不支持 CurseForge，请使用官方发行版";
pub(crate) const ACCESS_DENIED: &str = "CurseForge 服务暂不可用，请稍后重试或更新启动器";

pub(crate) fn key() -> Result<&'static str> {
    // Rust tracks option_env! inputs, so restoring a Cargo cache cannot retain
    // an old credential after the build secret is changed or removed.
    build_key(option_env!("CURSEFORGE_API_KEY"))
}

fn build_key(value: Option<&str>) -> Result<&str> {
    value
        .map(str::trim)
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 4096
                && value.bytes().all(|byte| byte.is_ascii_graphic())
        })
        .context(UNAVAILABLE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_credential_is_validated_without_disclosing_it_in_errors() {
        assert_eq!(
            build_key(Some("  test-app-credential  ")).unwrap(),
            "test-app-credential"
        );
        for value in [
            None,
            Some(""),
            Some("   "),
            Some("private\ncredential"),
            Some("non-ascii-密钥"),
        ] {
            assert_eq!(build_key(value).unwrap_err().to_string(), UNAVAILABLE);
        }
        assert_eq!(
            build_key(Some(&"x".repeat(4097))).unwrap_err().to_string(),
            UNAVAILABLE
        );
    }
}
