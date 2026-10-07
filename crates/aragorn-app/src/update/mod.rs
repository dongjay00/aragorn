mod decision;
mod error;
mod policy_doc;

pub use decision::{UpdateDecision, UpdatePolicy, UpdatePrefs, decide};
pub use error::UpdateError;
pub use policy_doc::{parse_policy, public_key_for, sign_policy, verify_and_parse};
