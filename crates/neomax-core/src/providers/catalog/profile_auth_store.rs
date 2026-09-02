use serde_json::Value;

use super::profile_auth_common::object_has_credential;
use super::types::AuthMethod;

enum DeclaredAuthMethod {
    Known(AuthMethod),
    Unknown,
}

pub(super) fn classify_store_entry(object: &serde_json::Map<String, Value>) -> Vec<AuthMethod> {
    let mut methods = Vec::new();
    if let Some(declared) = declared_auth_method(object) {
        match declared {
            DeclaredAuthMethod::Known(method) => {
                if object_has_payload(object, method) {
                    methods.push(method);
                }
            }
            DeclaredAuthMethod::Unknown => {}
        }
        return methods;
    }
    if object_has_any_credential(
        object,
        &["device_token", "deviceToken", "device_code", "deviceCode"],
    ) {
        methods.push(AuthMethod::Device);
    }
    if object_has_any_credential(
        object,
        &[
            "access_token",
            "accessToken",
            "refresh_token",
            "refreshToken",
            "access",
            "refresh",
        ],
    ) {
        methods.push(AuthMethod::OAuth);
    }
    if object_has_any_credential(
        object,
        &["api_key", "apiKey", "xai_api_key", "xaiApiKey", "key"],
    ) {
        methods.push(AuthMethod::ApiKey);
    }
    methods
}

fn declared_auth_method(object: &serde_json::Map<String, Value>) -> Option<DeclaredAuthMethod> {
    [
        "type",
        "method",
        "auth_mode",
        "authMode",
        "auth_type",
        "authType",
    ]
    .into_iter()
    .find_map(|key| {
        object.get(key).map(|value| {
            let Some(value) = value.as_str() else {
                return DeclaredAuthMethod::Unknown;
            };
            match value.to_ascii_lowercase().replace(['-', '_'], "").as_str() {
                "oauth" | "oidc" | "managedoauth" => DeclaredAuthMethod::Known(AuthMethod::OAuth),
                "device" | "deviceoauth" | "deviceauth" => {
                    DeclaredAuthMethod::Known(AuthMethod::Device)
                }
                "api" | "apikey" | "key" => DeclaredAuthMethod::Known(AuthMethod::ApiKey),
                _ => DeclaredAuthMethod::Unknown,
            }
        })
    })
}

fn object_has_payload(object: &serde_json::Map<String, Value>, method: AuthMethod) -> bool {
    let keys = match method {
        AuthMethod::OAuth => &[
            "access_token",
            "accessToken",
            "refresh_token",
            "refreshToken",
            "access",
            "refresh",
            "key",
        ][..],
        AuthMethod::Device => &[
            "device_token",
            "deviceToken",
            "device_code",
            "deviceCode",
            "access_token",
            "accessToken",
            "key",
        ][..],
        AuthMethod::ApiKey => &["api_key", "apiKey", "xai_api_key", "xaiApiKey", "key"][..],
        AuthMethod::LocalCredential => &[][..],
    };
    object_has_any_credential(object, keys)
}

fn object_has_any_credential(object: &serde_json::Map<String, Value>, keys: &[&str]) -> bool {
    object_has_credential(object, keys)
}
