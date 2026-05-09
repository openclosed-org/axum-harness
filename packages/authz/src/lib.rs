//! authz — Authorization abstraction layer.
//!
//! Provides OpenFGA-compatible authorization model DSL, port trait (AuthzPort),
//! and adapter implementations (mock for dev, OpenFGA for prod).

pub mod mock;
pub mod model;
pub mod openfga;
pub mod ports;

pub use mock::MockAuthzAdapter;
pub use model::{AuthorizationModel, RelationDefinition, TypeDefinition};
pub use openfga::{OpenFgaAdapter, OpenFgaConfig};
pub use ports::{AuthzCheck, AuthzDecision, AuthzError, AuthzPort, AuthzTuple, AuthzTupleKey};

#[cfg(test)]
mod mock_test;
