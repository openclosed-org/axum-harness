//! Framework-neutral bot channel adapter seams.
//!
//! Bot channels translate channel-specific commands into application use-case
//! requests. They do not own business rules and do not call Telegram or Discord
//! provider APIs directly.

use async_trait::async_trait;
use commercial::{
    CapabilityKey, CommercialSubject, CommercialTenant, EntitlementDecision, EntitlementError,
    EntitlementResolver, QuotaDecision, QuotaLedger, QuotaLedgerError,
};
use security_audit::{AuditEvent, AuditOutcome, AuditSink};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BotChannelKind {
    Telegram,
    Discord,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BotChannelMode {
    Disabled,
    LocalMock,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixtureBotUpdate {
    pub channel: BotChannelKind,
    pub tenant: CommercialTenant,
    pub subject: CommercialSubject,
    pub channel_user_id: String,
    pub text: String,
    pub request_id: Option<String>,
}

impl FixtureBotUpdate {
    pub fn new(
        channel: BotChannelKind,
        tenant: impl Into<String>,
        subject: impl Into<String>,
        channel_user_id: impl Into<String>,
        text: impl Into<String>,
    ) -> Self {
        Self {
            channel,
            tenant: CommercialTenant::new(tenant.into()),
            subject: CommercialSubject::new(subject.into()),
            channel_user_id: channel_user_id.into(),
            text: text.into(),
            request_id: None,
        }
    }

    pub fn request_id(mut self, request_id: impl Into<String>) -> Self {
        self.request_id = Some(request_id.into());
        self
    }
}

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

#[derive(Debug, thiserror::Error)]
pub enum BotAdapterError {
    #[error("bot command is empty")]
    EmptyCommand,
}

pub struct FixtureCommandAdapter {
    channel: BotChannelKind,
}

impl FixtureCommandAdapter {
    pub fn new(channel: BotChannelKind) -> Self {
        Self { channel }
    }

    pub fn map_update(
        &self,
        update: FixtureBotUpdate,
    ) -> Result<Option<BotCommand>, BotAdapterError> {
        if update.channel != self.channel {
            return Ok(None);
        }

        let text = update.text.trim();
        if text.is_empty() {
            return Ok(None);
        }

        let mut parts = text.split_whitespace();
        let Some(raw_command) = parts.next() else {
            return Ok(None);
        };
        if !raw_command.starts_with('/') {
            return Ok(None);
        }

        let command = normalize_command(raw_command).ok_or(BotAdapterError::EmptyCommand)?;
        Ok(Some(BotCommand {
            channel: update.channel,
            tenant: update.tenant,
            subject: update.subject,
            channel_user_id: update.channel_user_id,
            command,
            args: parts.map(ToOwned::to_owned).collect(),
            request_id: update.request_id,
        }))
    }
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
                .map(|route| (normalize_route_key(&route.command), route))
                .collect(),
        }
    }

    pub fn route(&self, command: &str) -> Option<&BotCommandRoute> {
        self.routes.get(&normalize_route_key(command))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BotUseCaseRequest {
    pub use_case: String,
    pub tenant: CommercialTenant,
    pub subject: CommercialSubject,
    pub channel: BotChannelKind,
    pub command: String,
    pub args: Vec<String>,
    pub request_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BotUseCaseResponse {
    pub message: String,
}

impl BotUseCaseResponse {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BotUseCaseError {
    #[error("use case rejected command: {0}")]
    Rejected(String),
}

#[async_trait]
pub trait BotUseCasePort: Send + Sync {
    async fn execute(
        &self,
        request: BotUseCaseRequest,
    ) -> Result<BotUseCaseResponse, BotUseCaseError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BotCommandOutcome {
    SkippedDisabled,
    Rejected { reason: String },
    Executed { response: BotUseCaseResponse },
}

#[derive(Debug, thiserror::Error)]
pub enum BotCommandError {
    #[error("bot command has no route: {0}")]
    UnknownCommand(String),
    #[error("entitlement check failed: {0}")]
    Entitlement(#[from] EntitlementError),
    #[error("quota operation failed: {0}")]
    Quota(#[from] QuotaLedgerError),
    #[error("audit append failed: {0}")]
    Audit(String),
    #[error("use case failed: {0}")]
    UseCase(#[from] BotUseCaseError),
}

pub struct GuardedBotCommandDispatcher {
    mode: BotChannelMode,
    router: BotCommandRouter,
    use_cases: Arc<dyn BotUseCasePort>,
    entitlement: Arc<dyn EntitlementResolver>,
    quota: Arc<dyn QuotaLedger>,
    audit: Arc<dyn AuditSink>,
}

impl GuardedBotCommandDispatcher {
    pub fn new(
        mode: BotChannelMode,
        router: BotCommandRouter,
        use_cases: Arc<dyn BotUseCasePort>,
        entitlement: Arc<dyn EntitlementResolver>,
        quota: Arc<dyn QuotaLedger>,
        audit: Arc<dyn AuditSink>,
    ) -> Self {
        Self {
            mode,
            router,
            use_cases,
            entitlement,
            quota,
            audit,
        }
    }

    pub async fn dispatch(
        &self,
        command: BotCommand,
    ) -> Result<BotCommandOutcome, BotCommandError> {
        if self.mode == BotChannelMode::Disabled {
            return Ok(BotCommandOutcome::SkippedDisabled);
        }

        let Some(route) = self.router.route(&command.command).cloned() else {
            self.audit_command(&command, None, AuditOutcome::Denied, "unknown command")
                .await?;
            return Err(BotCommandError::UnknownCommand(command.command));
        };

        match self
            .entitlement
            .check(
                &command.tenant,
                &command.subject,
                &route.required_capability,
            )
            .await?
        {
            EntitlementDecision::Allowed => {}
            EntitlementDecision::Denied { reason } => {
                self.audit_command(&command, Some(&route), AuditOutcome::Denied, &reason)
                    .await?;
                return Ok(BotCommandOutcome::Rejected { reason });
            }
        }

        let reservation = match self
            .quota
            .reserve(
                &command.tenant,
                &command.subject,
                &route.required_capability,
                route.quota_quantity,
            )
            .await?
        {
            QuotaDecision::Reserved(reservation) => reservation,
            QuotaDecision::Denied { reason } => {
                self.audit_command(&command, Some(&route), AuditOutcome::Denied, &reason)
                    .await?;
                return Ok(BotCommandOutcome::Rejected { reason });
            }
        };

        let request = BotUseCaseRequest {
            use_case: route.use_case.clone(),
            tenant: command.tenant.clone(),
            subject: command.subject.clone(),
            channel: command.channel,
            command: command.command.clone(),
            args: command.args.clone(),
            request_id: command.request_id.clone(),
        };

        match self.use_cases.execute(request).await {
            Ok(response) => {
                self.quota.commit(&reservation.reservation_id).await?;
                self.audit_command(&command, Some(&route), AuditOutcome::Succeeded, "executed")
                    .await?;
                Ok(BotCommandOutcome::Executed { response })
            }
            Err(error) => {
                self.quota.release(&reservation.reservation_id).await?;
                self.audit_command(
                    &command,
                    Some(&route),
                    AuditOutcome::Failed,
                    &error.to_string(),
                )
                .await?;
                Err(BotCommandError::UseCase(error))
            }
        }
    }

    async fn audit_command(
        &self,
        command: &BotCommand,
        route: Option<&BotCommandRoute>,
        outcome: AuditOutcome,
        reason: &str,
    ) -> Result<(), BotCommandError> {
        let event = AuditEvent::new(
            "bot.command",
            "bot-command",
            command.command.clone(),
            outcome,
        )
        .actor(command.subject.as_str())
        .tenant(command.tenant.as_str())
        .request(command.request_id.clone(), None)
        .metadata(serde_json::json!({
            "channel": channel_name(command.channel),
            "channel_user_id": command.channel_user_id,
            "command": command.command,
            "use_case": route.map(|route| route.use_case.as_str()),
            "required_capability": route.map(|route| route.required_capability.as_str()),
            "reason": reason,
        }));

        self.audit
            .append(event)
            .await
            .map_err(|error| BotCommandError::Audit(error.to_string()))
    }
}

fn normalize_command(raw: &str) -> Option<String> {
    let without_slash = raw.strip_prefix('/')?;
    let without_mention = without_slash.split('@').next().unwrap_or(without_slash);
    let normalized = normalize_route_key(without_mention);
    (!normalized.is_empty()).then_some(normalized)
}

fn normalize_route_key(command: &str) -> String {
    command.trim().trim_start_matches('/').to_ascii_lowercase()
}

fn channel_name(channel: BotChannelKind) -> &'static str {
    match channel {
        BotChannelKind::Telegram => "telegram",
        BotChannelKind::Discord => "discord",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use commercial::{InMemoryQuotaLedger, StaticEntitlementResolver};
    use security_audit::InMemoryAuditSink;
    use tokio::sync::Mutex;

    #[derive(Default)]
    struct RecordingUseCases {
        calls: Mutex<Vec<BotUseCaseRequest>>,
        fail: bool,
    }

    #[async_trait]
    impl BotUseCasePort for RecordingUseCases {
        async fn execute(
            &self,
            request: BotUseCaseRequest,
        ) -> Result<BotUseCaseResponse, BotUseCaseError> {
            self.calls.lock().await.push(request);
            if self.fail {
                Err(BotUseCaseError::Rejected("use case rejected".to_string()))
            } else {
                Ok(BotUseCaseResponse::new("counter incremented"))
            }
        }
    }

    impl RecordingUseCases {
        async fn calls(&self) -> Vec<BotUseCaseRequest> {
            self.calls.lock().await.clone()
        }
    }

    #[test]
    fn telegram_fixture_maps_command_text_to_channel_neutral_command() {
        let adapter = FixtureCommandAdapter::new(BotChannelKind::Telegram);

        let command = adapter
            .map_update(FixtureBotUpdate::new(
                BotChannelKind::Telegram,
                "tenant-a",
                "user-a",
                "telegram-user-1",
                "/counter_increment@demo_bot now",
            ))
            .unwrap()
            .unwrap();

        assert_eq!(command.channel, BotChannelKind::Telegram);
        assert_eq!(command.command, "counter_increment");
        assert_eq!(command.args, vec!["now"]);
    }

    #[test]
    fn discord_fixture_maps_command_text_to_channel_neutral_command() {
        let adapter = FixtureCommandAdapter::new(BotChannelKind::Discord);

        let command = adapter
            .map_update(FixtureBotUpdate::new(
                BotChannelKind::Discord,
                "tenant-a",
                "user-a",
                "discord-user-1",
                "/counter_increment",
            ))
            .unwrap()
            .unwrap();

        assert_eq!(command.channel, BotChannelKind::Discord);
        assert_eq!(command.command, "counter_increment");
    }

    #[tokio::test]
    async fn disabled_mode_skips_without_calling_use_case_or_audit() {
        let use_cases = Arc::new(RecordingUseCases::default());
        let audit = InMemoryAuditSink::shared();
        let dispatcher = dispatcher(
            BotChannelMode::Disabled,
            use_cases.clone(),
            Arc::new(StaticEntitlementResolver::allow_list(["counter.write"])),
            Arc::new(InMemoryQuotaLedger::default()),
            audit.clone(),
        );

        let outcome = dispatcher.dispatch(counter_command()).await.unwrap();

        assert_eq!(outcome, BotCommandOutcome::SkippedDisabled);
        assert!(use_cases.calls().await.is_empty());
        assert!(audit.events().await.is_empty());
    }

    #[tokio::test]
    async fn entitlement_denial_prevents_use_case_and_records_audit() {
        let use_cases = Arc::new(RecordingUseCases::default());
        let audit = InMemoryAuditSink::shared();
        let dispatcher = dispatcher(
            BotChannelMode::LocalMock,
            use_cases.clone(),
            Arc::new(StaticEntitlementResolver::allow_list(["counter.read"])),
            Arc::new(InMemoryQuotaLedger::default()),
            audit.clone(),
        );

        let outcome = dispatcher.dispatch(counter_command()).await.unwrap();

        assert!(matches!(outcome, BotCommandOutcome::Rejected { .. }));
        assert!(use_cases.calls().await.is_empty());
        let events = audit.events().await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].outcome, AuditOutcome::Denied);
        assert_eq!(events[0].metadata["required_capability"], "counter.write");
    }

    #[tokio::test]
    async fn quota_denial_prevents_use_case_and_records_audit() {
        let use_cases = Arc::new(RecordingUseCases::default());
        let audit = InMemoryAuditSink::shared();
        let quota = Arc::new(InMemoryQuotaLedger::default());
        quota
            .set_limit(
                &CommercialTenant::new("tenant-a"),
                &CapabilityKey::new("counter.write"),
                0,
            )
            .await;
        let dispatcher = dispatcher(
            BotChannelMode::LocalMock,
            use_cases.clone(),
            Arc::new(StaticEntitlementResolver::allow_list(["counter.write"])),
            quota,
            audit.clone(),
        );

        let outcome = dispatcher.dispatch(counter_command()).await.unwrap();

        assert!(matches!(outcome, BotCommandOutcome::Rejected { .. }));
        assert!(use_cases.calls().await.is_empty());
        assert_eq!(audit.events().await[0].outcome, AuditOutcome::Denied);
    }

    #[tokio::test]
    async fn allowed_command_calls_use_case_commits_quota_and_records_audit() {
        let use_cases = Arc::new(RecordingUseCases::default());
        let audit = InMemoryAuditSink::shared();
        let quota = Arc::new(InMemoryQuotaLedger::default());
        let tenant = CommercialTenant::new("tenant-a");
        let capability = CapabilityKey::new("counter.write");
        quota.set_limit(&tenant, &capability, 1).await;
        let dispatcher = dispatcher(
            BotChannelMode::LocalMock,
            use_cases.clone(),
            Arc::new(StaticEntitlementResolver::allow_list(["counter.write"])),
            quota.clone(),
            audit.clone(),
        );

        let outcome = dispatcher.dispatch(counter_command()).await.unwrap();

        assert!(matches!(outcome, BotCommandOutcome::Executed { .. }));
        let calls = use_cases.calls().await;
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].use_case, "counter.increment");
        assert_eq!(quota.committed_usage(&tenant, &capability).await, 1);
        assert_eq!(audit.events().await[0].outcome, AuditOutcome::Succeeded);
    }

    fn dispatcher(
        mode: BotChannelMode,
        use_cases: Arc<dyn BotUseCasePort>,
        entitlement: Arc<dyn EntitlementResolver>,
        quota: Arc<dyn QuotaLedger>,
        audit: Arc<dyn AuditSink>,
    ) -> GuardedBotCommandDispatcher {
        GuardedBotCommandDispatcher::new(
            mode,
            BotCommandRouter::new([BotCommandRoute::new(
                "counter_increment",
                "counter.increment",
                "counter.write",
            )]),
            use_cases,
            entitlement,
            quota,
            audit,
        )
    }

    fn counter_command() -> BotCommand {
        BotCommand {
            channel: BotChannelKind::Telegram,
            tenant: CommercialTenant::new("tenant-a"),
            subject: CommercialSubject::new("user-a"),
            channel_user_id: "telegram-user-1".to_string(),
            command: "counter_increment".to_string(),
            args: Vec::new(),
            request_id: Some("request-1".to_string()),
        }
    }
}
