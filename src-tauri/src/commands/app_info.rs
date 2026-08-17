#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub platform: String,
}

fn build_app_info() -> AppInfo {
    AppInfo {
        name: "KeyForge".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        platform: std::env::consts::OS.to_string(),
    }
}

#[tauri::command]
pub fn get_app_info() -> AppInfo {
    build_app_info()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_info_has_expected_identity() {
        let info = build_app_info();
        assert_eq!(info.name, "KeyForge");
        assert!(!info.version.is_empty());
        assert!(!info.platform.is_empty());
    }
}
