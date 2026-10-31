//! This module holds the host a scaffolded repository names.
//! It composes the origin URL from that host and an owner.

/// One host feeds the origin and image URLs because the tool has no second
/// forge model.
pub const HOST: &str = "github.com";

pub fn origin(host: &str, owner: &str) -> String {
    format!("https://{host}/{owner}")
}
