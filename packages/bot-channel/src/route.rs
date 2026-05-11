use crate::BotChannelKind;
use commercial::{CapabilityKey, CommercialSubject, CommercialTenant};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BotCommand {
    pub channel: BotChannelKind,
    pub tenant: CommercialTenant,
    pub subject: CommercialSubject,
    pub channel_user_id: String,
    pub command: String,
    pub args: Vec<String>,
    pub request_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BotCommandRoute {
    pub command: String,
    pub use_case: String,
    pub required_capability: CapabilityKey,
    pub quota_quantity: u64,
}

impl BotCommandRoute {
    pub fn new(
        command: impl Into<String>,
        use_case: impl Into<String>,
        required_capability: impl Into<String>,
    ) -> Self {
        Self {
            command: command.into(),
            use_case: use_case.into(),
            required_capability: CapabilityKey::new(required_capability.into()),
            quota_quantity: 1,
        }
    }

    pub fn quota_quantity(mut self, quantity: u64) -> Self {
        self.quota_quantity = quantity;
        self
    }
}

#[derive(Debug, Clone, Default)]
pub struct BotCommandRouter {
    routes: HashMap<String, BotCommandRoute>,
}

impl BotCommandRouter {
    pub fn new(routes: impl IntoIterator<Item = BotCommandRoute>) -> Self {
        Self {
            routes: routes
                .into_iter()
                .map(|route| (crate::normalize_route_key(&route.command), route))
                .collect(),
        }
    }

    pub fn route(&self, command: &str) -> Option<&BotCommandRoute> {
        self.routes.get(&crate::normalize_route_key(command))
    }
}
