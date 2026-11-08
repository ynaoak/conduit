//! Generating an SSH key pair for git hosting.
//!
//! The key is made here rather than in the tool window: a webview would
//! have to hand-roll the OpenSSH private key format, and the one thing
//! nobody should hand-roll is the container their private key lives in.
//! `ssh-key` writes exactly what `ssh-keygen` writes.

use std::path::PathBuf;
use std::time::Instant;

use rand_chacha::ChaCha20Rng;
use serde::Serialize;
use sha2::{Digest, Sha512};
use ssh_key::rand_core::{OsRng, RngCore, SeedableRng};
use ssh_key::private::RsaKeypair;
use ssh_key::{Algorithm, HashAlg, LineEnding, PrivateKey};
use tauri::{AppHandle, Window};
use tauri_plugin_dialog::DialogExt;

/// The tool window allowed to generate keys, alongside the launcher.
const TOOL_WORKFLOW: &str = "conduit.devtools";
const TOOL_HTML: &str = "sshkey.html";

/// RSA key size. 4096 is what hosting providers document for the case
/// where Ed25519 is not accepted; below 3072 is not worth offering.
const RSA_BITS: usize = 4096;

/// How many timing samples to take off the machine itself.
const JITTER_SAMPLES: usize = 4096;

/// Domain separator, so this construction can never collide with another
/// use of the same inputs.
const MIX_DOMAIN: &[u8] = b"conduit/ssh-key/v1";

#[derive(Serialize)]
pub struct SshKeyPair {
    /// `ssh-ed25519 AAAA... comment` — the one that goes in the host's
    /// "add an SSH key" box.
    pub public: String,
    /// OpenSSH private key, encrypted if a passphrase was given.
    pub private: String,
    /// `SHA256:...`, as `ssh-keygen -l` prints it, for comparing against
    /// what the host says it received.
    pub fingerprint: String,
    /// What was actually generated, since the request is a string.
    pub algorithm: String,
    /// Suggested file name (`id_ed25519`), matching ssh's own convention.
    pub suggested_name: String,
}

fn may_generate(label: &str) -> bool {
    label == "main"
        || label == crate::plugins::workflows::tool_window::window_label(TOOL_WORKFLOW, TOOL_HTML)
}

/// Ed25519 unless something explicitly asks for RSA.
///
/// Ed25519 is the default because it is small, fast and supported by every
/// current git host; RSA is here for the hosts and appliances that still
/// refuse anything else.
fn algorithm_for(kind: &str) -> Result<(Algorithm, &'static str), String> {
    match kind {
        "" | "ed25519" => Ok((Algorithm::Ed25519, "id_ed25519")),
        "rsa" => Ok((Algorithm::Rsa { hash: None }, "id_rsa")),
        other => Err(format!("unknown key type: {other}")),
    }
}

/// The randomness the key is built from: the operating system's generator
/// and the machine's own timing noise, hashed together.
///
/// What matters is that the OS generator is *in* there. Mixing can only
/// add to it, and it is also the only reason this is safe — a
/// construction that replaced OsRng with collected noise would be
/// strictly worse, however convincing the collecting looks.
///
/// So the jitter is not what makes this secure, and nothing here should
/// be read as claiming otherwise.
fn mixed_rng() -> ChaCha20Rng {
    let mut os_seed = [0u8; 32];
    OsRng.fill_bytes(&mut os_seed);

    let mut hasher = Sha512::new();
    hasher.update(MIX_DOMAIN);
    hasher.update(os_seed);
    hasher.update(timing_jitter());

    let digest = hasher.finalize();
    let mut seed = [0u8; 32];
    seed.copy_from_slice(&digest[..32]);
    ChaCha20Rng::from_seed(seed)
}

/// The machine's own noise: how long each read of the clock takes.
///
/// Successive readings differ by amounts nobody schedules — cache state,
/// interrupts, frequency scaling. Any single sample is weak, which is why
/// this is one input among three rather than the source.
fn timing_jitter() -> [u8; 64] {
    let mut hasher = Sha512::new();
    let start = Instant::now();
    for _ in 0..JITTER_SAMPLES {
        hasher.update(start.elapsed().as_nanos().to_le_bytes());
    }
    hasher.finalize().into()
}

#[tauri::command]
pub async fn generate_ssh_key(
    window: Window,
    kind: String,
    comment: String,
    passphrase: String,
) -> Result<SshKeyPair, String> {
    if !may_generate(window.label()) {
        return Err("this window may not generate keys".into());
    }
    let (algorithm, suggested_name) = algorithm_for(&kind)?;

    // RSA-4096 takes seconds, and doing that on the UI thread would freeze
    // the window mid-click.
    let comment = comment.trim().to_string();
    let generated = tauri::async_runtime::spawn_blocking(move || {
        let mut rng = mixed_rng();
        // `PrivateKey::random` has no bit size to give it for RSA, so the
        // keypair is built first and wrapped afterwards.
        let mut key = match algorithm {
            Algorithm::Rsa { .. } => RsaKeypair::random(&mut rng, RSA_BITS).map(PrivateKey::from),
            other => PrivateKey::random(&mut rng, other),
        }
        .map_err(|e| format!("could not generate a key: {e}"))?;

        key.set_comment(&comment);

        let public = key
            .public_key()
            .to_openssh()
            .map_err(|e| format!("could not encode the public key: {e}"))?;
        let fingerprint = key.fingerprint(HashAlg::Sha256).to_string();
        let algorithm_name = key.algorithm().as_str().to_string();

        // Encrypt last: an encrypted key still exposes its public half and
        // its comment, but nothing here needs the plaintext afterwards.
        let private = if passphrase.is_empty() {
            key.to_openssh(LineEnding::LF)
        } else {
            key.encrypt(&mut rng, passphrase)
                .map_err(|e| format!("could not encrypt the key: {e}"))?
                .to_openssh(LineEnding::LF)
        }
        .map_err(|e| format!("could not encode the private key: {e}"))?;

        Ok::<_, String>((public, private.to_string(), fingerprint, algorithm_name))
    })
    .await
    .map_err(|e| format!("key generation did not finish: {e}"))??;

    let (public, private, fingerprint, algorithm) = generated;
    Ok(SshKeyPair {
        public,
        private,
        fingerprint,
        algorithm,
        suggested_name: suggested_name.to_string(),
    })
}

/// Ask where to put the key, then write the pair next to each other the
/// way ssh expects to find them.
///
/// The dialog is opened here rather than in the window because a tool
/// window is not granted the dialog plugin, and because the private key
/// has to be created with its permissions already set — which means the
/// same step has to own both the path and the write.
///
/// Returns the private key's path, or `None` if the dialog was dismissed.
#[tauri::command]
pub async fn save_ssh_key_dialog(
    app: AppHandle,
    window: Window,
    suggested_name: String,
    public: String,
    private: String,
) -> Result<Option<String>, String> {
    if !may_generate(window.label()) {
        return Err("this window may not write keys".into());
    }

    let mut dialog = app.dialog().file().set_file_name(&suggested_name);
    // ~/.ssh is where these belong, when it is already there — offering it
    // saves the navigation without creating anything on its own.
    if let Some(home) = dirs_next_home() {
        let ssh = home.join(".ssh");
        if ssh.is_dir() {
            dialog = dialog.set_directory(ssh);
        }
    }

    let Some(path) = dialog.blocking_save_file() else {
        return Ok(None);
    };
    let path = path
        .into_path()
        .map_err(|e| format!("could not resolve the chosen path: {e}"))?;

    write_private(&path, &private)?;
    let public_path = with_pub_extension(&path);
    std::fs::write(&public_path, public.trim_end().to_string() + "\n")
        .map_err(|e| format!("could not write {}: {e}", public_path.display()))?;

    Ok(Some(path.display().to_string()))
}

/// The home directory, without taking a dependency for one lookup.
fn dirs_next_home() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE").map(PathBuf::from)
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}

/// `id_ed25519` -> `id_ed25519.pub`. Appended rather than replacing an
/// extension: a key named `work.key` becomes `work.key.pub`, not
/// `work.pub`, which is what ssh would look for.
fn with_pub_extension(path: &std::path::Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".pub");
    PathBuf::from(name)
}

/// Write the private key with owner-only permissions.
///
/// ssh refuses to use a key others can read, and it is right to: creating
/// the file world-readable first and fixing it afterwards leaves a window
/// where the key is exposed, so the mode is set as the file is created.
#[cfg(unix)]
fn write_private(path: &std::path::Path, contents: &str) -> Result<(), String> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .map_err(|e| format!("could not write {}: {e}", path.display()))?;
    file.write_all(contents.as_bytes())
        .map_err(|e| format!("could not write {}: {e}", path.display()))
}

/// Windows has no mode bits; the file inherits the directory's ACL, and
/// OpenSSH for Windows checks ownership rather than a permission mask.
#[cfg(not(unix))]
fn write_private(path: &std::path::Path, contents: &str) -> Result<(), String> {
    std::fs::write(path, contents).map_err(|e| format!("could not write {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_launcher_and_the_key_tool_may_generate() {
        assert!(may_generate("main"));
        assert!(may_generate(
            &crate::plugins::workflows::tool_window::window_label(TOOL_WORKFLOW, TOOL_HTML)
        ));
        assert!(!may_generate(
            &crate::plugins::workflows::tool_window::window_label("imported.package", TOOL_HTML)
        ));
        assert!(!may_generate("tool-0000000000000000"));
    }

    #[test]
    fn ed25519_is_what_you_get_unless_you_ask_otherwise() {
        assert!(matches!(algorithm_for(""), Ok((Algorithm::Ed25519, "id_ed25519"))));
        assert!(matches!(
            algorithm_for("ed25519"),
            Ok((Algorithm::Ed25519, "id_ed25519"))
        ));
        assert!(matches!(algorithm_for("rsa"), Ok((Algorithm::Rsa { .. }, "id_rsa"))));
        assert!(algorithm_for("dsa").is_err());
    }

    /// ssh looks for `<key>.pub`, not for the name with its extension
    /// swapped — a key called `work.key` must not produce `work.pub`.
    /// Two keys in a row must not come out the same. The jitter alone
    /// would be a weak guarantee of that; OsRng is what actually provides
    /// it, which is the point of hashing it in.
    #[test]
    fn two_keys_in_a_row_differ() {
        let first = PrivateKey::random(&mut mixed_rng(), Algorithm::Ed25519).unwrap();
        let second = PrivateKey::random(&mut mixed_rng(), Algorithm::Ed25519).unwrap();
        assert_ne!(first.public_key(), second.public_key());
    }

    /// The machine's timing samples have to actually differ; a constant
    /// would contribute nothing while looking like it did.
    #[test]
    fn the_timing_samples_are_not_a_constant() {
        assert_ne!(timing_jitter(), timing_jitter());
    }

    #[test]
    fn the_public_key_sits_beside_the_private_one() {
        assert_eq!(
            with_pub_extension(std::path::Path::new("/home/me/.ssh/id_ed25519")),
            PathBuf::from("/home/me/.ssh/id_ed25519.pub")
        );
        assert_eq!(
            with_pub_extension(std::path::Path::new("work.key")),
            PathBuf::from("work.key.pub")
        );
    }

    /// The generated pair has to be the real thing: an OpenSSH private key
    /// that parses back, whose public half matches, with the comment and
    /// fingerprint ssh-keygen would report.
    #[test]
    fn a_generated_key_round_trips_as_openssh() {
        let mut key = PrivateKey::random(&mut OsRng, Algorithm::Ed25519).expect("generated");
        key.set_comment("me@example.com");

        let private = key.to_openssh(LineEnding::LF).expect("encoded");
        assert!(private.starts_with("-----BEGIN OPENSSH PRIVATE KEY-----"));

        let parsed = PrivateKey::from_openssh(private.as_bytes()).expect("parses back");
        assert_eq!(parsed.public_key(), key.public_key());
        assert_eq!(parsed.comment(), "me@example.com");

        let public = key.public_key().to_openssh().expect("encoded");
        assert!(public.starts_with("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5"), "{public}");
        assert!(public.ends_with(" me@example.com"), "{public}");

        assert!(key.fingerprint(HashAlg::Sha256).to_string().starts_with("SHA256:"));
    }

    /// A passphrase has to actually encrypt: the encrypted form must not
    /// open without it, and must open with it.
    #[test]
    fn a_passphrase_locks_the_private_key() {
        let key = PrivateKey::random(&mut OsRng, Algorithm::Ed25519).expect("generated");
        let locked = key.encrypt(&mut OsRng, "correct horse").expect("encrypted");
        let armored = locked.to_openssh(LineEnding::LF).expect("encoded");

        let parsed = PrivateKey::from_openssh(armored.as_bytes()).expect("parses");
        assert!(parsed.is_encrypted());
        assert!(parsed.decrypt("wrong").is_err());

        let opened = parsed.decrypt("correct horse").expect("opens");
        assert_eq!(opened.public_key(), key.public_key());
    }
}
