use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[schemars(deny_unknown_fields)]
pub struct DevelopmentMailPolicy {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub bindings: BTreeMap<String, DevelopmentMailValue>,
}

impl DevelopmentMailPolicy {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.enabled.is_none() && self.bindings.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum DevelopmentMailValue {
    SmtpHost,
    SmtpPort,
    SmtpUrl,
    UiUrl,
}

impl DevelopmentMailValue {
    #[must_use]
    pub const fn value(self) -> &'static str {
        match self {
            Self::SmtpHost => "mailpit",
            Self::SmtpPort => "1025",
            Self::SmtpUrl => "smtp://mailpit:1025",
            Self::UiUrl => "https://mail.mm.test",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MailBindingProvenance {
    InjectedDefault,
    InjectedConfigured,
    PreservedProjectValue,
    PreservedComposeValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MailBindingManifest {
    pub key: String,
    pub value: DevelopmentMailValue,
    pub provenance: MailBindingProvenance,
}

#[must_use]
pub fn configured_bindings(
    policy: &DevelopmentMailPolicy,
) -> BTreeMap<String, (DevelopmentMailValue, bool)> {
    if !policy.bindings.is_empty() {
        return policy
            .bindings
            .iter()
            .map(|(key, value)| (key.clone(), (*value, true)))
            .collect();
    }
    [
        ("MM_MAIL_SMTP_HOST", DevelopmentMailValue::SmtpHost),
        ("MM_MAIL_SMTP_PORT", DevelopmentMailValue::SmtpPort),
        ("MM_MAIL_SMTP_URL", DevelopmentMailValue::SmtpUrl),
        ("MM_MAIL_UI_URL", DevelopmentMailValue::UiUrl),
    ]
    .into_iter()
    .map(|(key, value)| (key.to_owned(), (value, false)))
    .collect()
}
