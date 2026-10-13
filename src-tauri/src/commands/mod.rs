use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: &'static str,
    pub version: &'static str,
    pub read_only: bool,
}

#[tauri::command]
pub fn app_info() -> AppInfo {
    AppInfo { name: "DB-BUTURE", version: env!("CARGO_PKG_VERSION"), read_only: true }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_is_read_only_by_default() {
        assert!(app_info().read_only);
    }
}
