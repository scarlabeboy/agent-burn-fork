#![forbid(unsafe_code)]

macro_rules! redacted {
    ($($t:ty),+ $(,)?) => {$ (
        impl std::fmt::Debug for $t {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(concat!(stringify!($t), "([redacted])"))
            }
        }
    )+};
}

mod refs;
pub use abf_observation::{EnvironmentKind, OpaqueRef, SourceRunStatus};
pub use refs::*;
mod model;
pub use model::*;
mod finalize;
mod reconcile;
pub use finalize::finalize;
mod diagnostics;
pub use diagnostics::*;
mod evidence;
pub use evidence::DeliveryEvidence;
mod membership;
pub use membership::*;
mod projection;
pub use projection::*;
