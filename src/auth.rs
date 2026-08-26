use std::{
    env,
    ffi::{OsStr, OsString},
    fs,
    path::{Component, Path, PathBuf},
};

use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexAuth {
    pub access_token: String,
    pub account_id: Option<String>,
}

#[derive(Debug, Clone, Error)]
pub enum AuthLoadError {
    #[error("auth.json not found: {0}")]
    NotFound(PathBuf),
    #[error("failed to read auth.json: {0}")]
    Read(PathBuf),
    #[error("auth.json is not valid JSON: {0}")]
    InvalidJson(PathBuf),
    #[error("auth.json does not contain an access token")]
    MissingAccessToken,
}

pub fn default_auth_path() -> PathBuf {
    resolve_auth_path(
        env::var_os("CODEX_AUTH_JSON"),
        env::var_os("CODEX_HOME"),
        env::var_os("USERPROFILE").or_else(|| env::var_os("HOME")),
    )
}

pub fn load_codex_auth(path: Option<&Path>) -> Result<CodexAuth, AuthLoadError> {
    let auth_path = path.map(PathBuf::from).unwrap_or_else(default_auth_path);
    if !auth_path.exists() {
        return Err(AuthLoadError::NotFound(auth_path));
    }

    let text =
        fs::read_to_string(&auth_path).map_err(|_| AuthLoadError::Read(auth_path.clone()))?;
    let payload: Value =
        serde_json::from_str(&text).map_err(|_| AuthLoadError::InvalidJson(auth_path.clone()))?;
    let object = payload.as_object();
    let tokens = object
        .and_then(|value| value.get("tokens"))
        .and_then(Value::as_object);

    let access_token = first_string([
        tokens.and_then(|value| value.get("access_token")),
        tokens.and_then(|value| value.get("accessToken")),
        object.and_then(|value| value.get("access_token")),
        object.and_then(|value| value.get("accessToken")),
    ])
    .ok_or(AuthLoadError::MissingAccessToken)?;

    let account_id = first_string([
        tokens.and_then(|value| value.get("account_id")),
        tokens.and_then(|value| value.get("accountId")),
        object.and_then(|value| value.get("account_id")),
        object.and_then(|value| value.get("accountId")),
    ]);

    Ok(CodexAuth {
        access_token,
        account_id,
    })
}

fn resolve_auth_path(
    explicit: Option<OsString>,
    codex_home: Option<OsString>,
    user_home: Option<OsString>,
) -> PathBuf {
    let user_home = non_empty_os(user_home)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));

    if let Some(path) = non_empty_os(explicit) {
        return expand_user(PathBuf::from(path), &user_home);
    }

    if let Some(home) = non_empty_os(codex_home) {
        return expand_user(PathBuf::from(home), &user_home).join("auth.json");
    }

    user_home.join(".codex").join("auth.json")
}

fn non_empty_os(value: Option<OsString>) -> Option<OsString> {
    value.filter(|value| !value.is_empty())
}

fn expand_user(path: PathBuf, user_home: &Path) -> PathBuf {
    let mut components = path.components();
    match components.next() {
        Some(Component::Normal(component)) if component == OsStr::new("~") => {
            user_home.join(components.as_path())
        }
        _ => path,
    }
}

fn first_string<const N: usize>(values: [Option<&Value>; N]) -> Option<String> {
    values.into_iter().flatten().find_map(|value| {
        value
            .as_str()
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_owned)
    })
}

#[cfg(test)]
mod tests {
    use std::{ffi::OsString, fs, path::PathBuf};

    use super::{load_codex_auth, resolve_auth_path, AuthLoadError};

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "background_codex_quota_auth_{name}_{}.json",
            std::process::id()
        ))
    }

    #[test]
    fn loads_nested_snake_case_fields() {
        let path = temp_path("snake");
        fs::write(
            &path,
            r#"{"tokens":{"access_token":"access-1","account_id":"account-1"}}"#,
        )
        .unwrap();
        let auth = load_codex_auth(Some(&path)).unwrap();
        fs::remove_file(path).unwrap();
        assert_eq!(auth.access_token, "access-1");
        assert_eq!(auth.account_id.as_deref(), Some("account-1"));
    }

    #[test]
    fn loads_camel_case_fields_and_rejects_missing_token() {
        let path = temp_path("camel");
        fs::write(
            &path,
            r#"{"accessToken":"access-2","accountId":"account-2"}"#,
        )
        .unwrap();
        let auth = load_codex_auth(Some(&path)).unwrap();
        fs::remove_file(&path).unwrap();
        assert_eq!(auth.access_token, "access-2");
        assert_eq!(auth.account_id.as_deref(), Some("account-2"));

        let missing = temp_path("missing");
        fs::write(&missing, r#"{"tokens":{"account_id":"account-1"}}"#).unwrap();
        assert!(matches!(
            load_codex_auth(Some(&missing)),
            Err(AuthLoadError::MissingAccessToken)
        ));
        fs::remove_file(missing).unwrap();
    }

    #[test]
    fn reports_missing_invalid_and_unreadable_auth_files() {
        let missing = temp_path("not-found");
        assert!(matches!(
            load_codex_auth(Some(&missing)),
            Err(AuthLoadError::NotFound(_))
        ));

        let invalid = temp_path("invalid-json");
        fs::write(&invalid, b"{").unwrap();
        assert!(matches!(
            load_codex_auth(Some(&invalid)),
            Err(AuthLoadError::InvalidJson(_))
        ));
        fs::remove_file(invalid).unwrap();

        let directory = temp_path("directory");
        fs::create_dir(&directory).unwrap();
        assert!(matches!(
            load_codex_auth(Some(&directory)),
            Err(AuthLoadError::Read(_))
        ));
        fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn resolves_auth_paths_in_documented_order() {
        let explicit = temp_path("explicit");
        let codex_home = std::env::temp_dir().join(format!(
            "background_codex_quota_home_{}",
            std::process::id()
        ));
        let user_home = std::env::temp_dir().join(format!(
            "background_codex_quota_user_{}",
            std::process::id()
        ));

        assert_eq!(
            resolve_auth_path(
                Some(explicit.clone().into_os_string()),
                Some(codex_home.clone().into_os_string()),
                Some(user_home.clone().into_os_string()),
            ),
            explicit
        );
        assert_eq!(
            resolve_auth_path(
                None,
                Some(codex_home.clone().into_os_string()),
                Some(user_home.clone().into_os_string()),
            ),
            codex_home.join("auth.json")
        );
        assert_eq!(
            resolve_auth_path(None, None, Some(user_home.clone().into_os_string())),
            user_home.join(".codex").join("auth.json")
        );
        assert_eq!(
            resolve_auth_path(
                Some(OsString::from("~\\custom\\auth.json")),
                None,
                Some(user_home.clone().into_os_string()),
            ),
            user_home.join("custom").join("auth.json")
        );
    }
}
