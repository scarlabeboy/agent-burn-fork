//! In-memory fictional authority experiment. No native backend or S2 conversion.
//!
//! Selection and root fields are private, including their allocation identity.
//! ```compile_fail
//! use abf_windows_confinement_fixture::Selection;
//! let forged = Selection {};
//! ```
//! ```compile_fail
//! use abf_windows_confinement_fixture::FakeRoot;
//! let forged = FakeRoot {};
//! ```
//! ```compile_fail
//! use abf_windows_confinement_fixture::Object;
//! let forged = Object(0);
//! ```
//! No generic plan-to-authority conversion exists. A downstream plan can be
//! passed as a claim only in its own crate; it cannot satisfy this signature.
//! ```compile_fail
//! use abf_windows_confinement_fixture::{FakeRoot, Selection};
//! fn promote(plan: impl Sized) -> FakeRoot { plan.into() }
//! ```
//! ```compile_fail
//! use abf_windows_confinement_fixture::{Driver, FakeWorld, Binding, Limits};
//! struct FixturePlan;
//! fn use_s2(plan: &FixturePlan, world: FakeWorld, binding: Binding) {
//!     Driver::new(world, Some(plan), binding, Limits::default(), true, false);
//! }
//! ```
#![forbid(unsafe_code)]
mod driver;
mod model;
pub use driver::*;
pub use model::*;
