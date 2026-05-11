use crate::BotCommand;
use commercial::{CommercialSubject, CommercialTenant};

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

        let command = crate::normalize_command(raw_command).ok_or(BotAdapterError::EmptyCommand)?;
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
