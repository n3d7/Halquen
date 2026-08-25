use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustClass {
    UserExplicit,
    LocalVerified,
    UserConfirmedResult,
    UserBehaviour,
    AiInferred,
    PluginAsserted,
    ExternalContent,
}

impl TrustClass {
    pub fn independently_authorizes_procedural_memory(self) -> bool {
        matches!(self, Self::UserExplicit | Self::UserConfirmedResult)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_and_ai_content_are_not_authority() {
        assert!(!TrustClass::ExternalContent.independently_authorizes_procedural_memory());
        assert!(!TrustClass::AiInferred.independently_authorizes_procedural_memory());
        assert!(TrustClass::UserExplicit.independently_authorizes_procedural_memory());
    }
}
