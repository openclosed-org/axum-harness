//! Composition aliases owned by the Web BFF bootstrap.

use counter_service::contracts::service::CounterService;
use std::sync::Arc;
use tenant_service::application::TenantServiceTrait;
use user_service::ports::{TenantRepository, UserRepository, UserTenantRepository};

pub type CounterServiceHandle = Arc<dyn CounterService>;
pub type TenantServiceHandle = Arc<dyn TenantServiceTrait>;
pub type UserProfileRepositoryHandle = Arc<dyn UserRepository>;
pub type UserTenantRepositoryHandle = Arc<dyn UserTenantRepository>;
pub type UserTenantInfoRepositoryHandle = Arc<dyn TenantRepository>;
