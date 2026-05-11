//! Notification and outbound webhook capability ports.
//!
//! This crate is framework-neutral. Business use cases depend on these ports;
//! provider SDKs, HTTP entrypoints, and long-running loops belong in adapters,
//! servers, or workers.

pub mod delivery;
pub mod inbound;
pub mod notification;
pub mod outbox;
pub mod retry;
pub mod webhook;

pub use delivery::{DeliveryError, DeliveryWorker, NoopOutboundTransport, OutboundTransport};
pub use inbound::{
    InMemoryInboundWebhookLedger, InboundWebhookEvent, InboundWebhookLedger,
    InboundWebhookLedgerError,
};
pub use notification::{
    NoopNotificationPort, NotificationChannel, NotificationError, NotificationMessage,
    NotificationMode, NotificationOutcome, NotificationPort, NotificationStatus,
    OutboxNotificationPort,
};
pub use outbox::{
    DeliveryJob, DeliveryJobStatus, DeliveryKind, DeliveryOutbox, DeliveryOutboxError,
    DeliveryTarget, InMemoryDeliveryOutbox,
};
pub use retry::RetryPolicy;
pub use webhook::{
    HmacSha256WebhookSigner, InMemoryWebhookRegistry, OutboundWebhookEvent, OutboundWebhookPort,
    OutboxWebhookPublisher, WebhookEndpoint, WebhookRegistry, WebhookRegistryError,
};
