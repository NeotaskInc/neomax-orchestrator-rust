use neomax_core::providers::ProviderProfile;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum DetectedAuth {
    OAuth,
    ApiKey,
    Device,
    Unknown,
}

impl DetectedAuth {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::OAuth => "oauth",
            Self::ApiKey => "api-key",
            Self::Device => "device",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ManagedProfile {
    pub(crate) profile: ProviderProfile,
    pub(crate) auth: Option<DetectedAuth>,
}

impl ManagedProfile {
    pub(crate) fn authenticated(&self) -> bool {
        self.auth.is_some()
    }

    pub(crate) fn account(&self) -> &str {
        &self.profile.account
    }
}
