#![no_std]
#![allow(unused)]

mod types;

pub use types::{
    Ctx, EnclaveBuffer, EnclaveError, UserSpaceBuffer, HealthCheckResult, OcallReturn
};

pub const PUBLIC_KEY_SIZE: usize = 32;
