use crate::{
    BotChannelMode, BotCommand, BotCommandRoute, BotCommandRouter, BotUseCaseError, BotUseCasePort,
    BotUseCaseRequest, BotUseCaseResponse,
};
use commercial::{
    EntitlementDecision, EntitlementError, EntitlementResolver, QuotaDecision, QuotaLedger,
    QuotaLedgerError,
};
use security_audit::{AuditEvent, AuditOutcome, AuditSink};
use std::sync::Arc;

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
            "channel": crate::channel_name(command.channel),
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
