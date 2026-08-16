//! Which way this copy of conduit was distributed.
//!
//! Baked in at build time by `build.rs` from `CONDUIT_RELEASE_CHANNEL`, so a
//! binary cannot be talked into changing its mind. The distinction exists for
//! one reason: **only the downloaded copy updates itself.** A Store copy that
//! also self-updated would leave the installed version and the version the
//! Store believes it shipped drifting apart, with the user in between.

/// `"download"` (GitHub Releases) or `"store"` (Microsoft Store).
pub const CHANNEL: &str = env!("CONDUIT_RELEASE_CHANNEL");

/// Whether this build is allowed to update itself.
pub fn self_updates() -> bool {
    CHANNEL == "download"
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Building without the variable set must produce the downloadable
    /// build — the Store copy is the one that has to be asked for
    /// explicitly, so that forgetting the flag never ships a self-updating
    /// binary to the Store.
    #[test]
    fn the_default_build_is_the_downloadable_one() {
        if std::env::var("CONDUIT_RELEASE_CHANNEL").is_err() {
            assert_eq!(CHANNEL, "download");
            assert!(self_updates());
        }
    }

    #[test]
    fn only_the_download_channel_self_updates() {
        assert_eq!(self_updates(), CHANNEL == "download");
    }
}
