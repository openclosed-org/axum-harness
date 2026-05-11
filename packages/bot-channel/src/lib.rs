//! Framework-neutral bot channel adapter seams.
//!
//! Bot channels translate channel-specific commands into application use-case
//! requests. They do not own business rules and do not call Telegram or Discord
//! provider APIs directly.

pub mod adapter;
pub mod dispatch;
pub mod route;
pub mod use_case;

pub use adapter::{
    BotAdapterError, BotChannelKind, BotChannelMode, FixtureBotUpdate, FixtureCommandAdapter,
};
pub use dispatch::{BotCommandError, BotCommandOutcome, GuardedBotCommandDispatcher};
pub use route::{BotCommand, BotCommandRoute, BotCommandRouter};
pub use use_case::{BotUseCaseError, BotUseCasePort, BotUseCaseRequest, BotUseCaseResponse};

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
    use async_trait::async_trait;
    use commercial::{
        CapabilityKey, CommercialSubject, CommercialTenant, EntitlementResolver,
        InMemoryQuotaLedger, QuotaLedger, StaticEntitlementResolver,
    };
    use security_audit::{AuditOutcome, AuditSink, InMemoryAuditSink};
    use std::sync::Arc;
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
