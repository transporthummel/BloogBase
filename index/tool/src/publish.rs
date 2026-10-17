//! Builds `index.json` from the entries and signs it with minisign
//! (PLAN.md §10.1–10.2).

use std::io::Cursor;

use minisign::{KeyPair, PublicKey, SecretKey, SecretKeyBox};
use serde::Serialize;

use crate::entry::Entry;

/// Bumped only for changes old apps would misread; additions don't need it.
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Index<'a> {
    schema_version: u32,
    /// Unix seconds; apps never go back to an older index.
    generated_at: u64,
    mods: &'a [Entry],
}

pub fn index_json(entries: &[Entry], generated_at: u64) -> String {
    let mut sorted = entries.to_vec();
    sorted.sort_by(|a, b| a.id.cmp(&b.id));
    let index = Index {
        schema_version: SCHEMA_VERSION,
        generated_at,
        mods: &sorted,
    };
    serde_json::to_string_pretty(&index).expect("entries serialize") + "\n"
}

/// The `.minisig` file for `data`. The trusted comment repeats the
/// timestamp, so it is covered by the signature as well.
pub fn sign(data: &[u8], secret_key: &str, generated_at: u64) -> Result<String, String> {
    let sk = load_secret_key(secret_key)?;
    let pk = PublicKey::from_secret_key(&sk).map_err(|e| e.to_string())?;
    let sig = minisign::sign(
        Some(&pk),
        &sk,
        Cursor::new(data),
        Some(&format!(
            "bloogbase-index schema:{SCHEMA_VERSION} generated:{generated_at}"
        )),
        Some("signature of the BloogBase mod index"),
    )
    .map_err(|e| e.to_string())?;
    Ok(sig.into_string())
}

fn load_secret_key(text: &str) -> Result<SecretKey, String> {
    SecretKeyBox::from_string(text.trim())
        .and_then(|b| b.into_unencrypted_secret_key())
        .map_err(|e| format!("not an unencrypted minisign secret key: {e}"))
}

/// A new key pair: (secret key file, public key file, public key base64).
pub fn keygen() -> Result<(String, String, String), String> {
    let KeyPair { pk, sk } = KeyPair::generate_unencrypted_keypair().map_err(|e| e.to_string())?;
    let secret = sk
        .to_box(Some("BloogBase mod index secret key"))
        .map_err(|e| e.to_string())?
        .into_string();
    let public = pk.to_box().map_err(|e| e.to_string())?.into_string();
    Ok((secret, public, pk.to_base64()))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::entry::parse;

    #[test]
    fn signature_verifies_with_the_public_key() {
        let (secret, _, public) = keygen().unwrap();
        let data = b"{\"mods\":[]}";
        let sig = sign(data, &secret, 42).unwrap();
        let pk = minisign::PublicKey::from_base64(&public).unwrap();
        let sig_box = minisign::SignatureBox::from_string(&sig).unwrap();
        assert_eq!(
            sig_box.trusted_comment().unwrap(),
            "bloogbase-index schema:1 generated:42"
        );
        minisign::verify(&pk, &sig_box, Cursor::new(data), true, false, false).unwrap();
        let sig_box = minisign::SignatureBox::from_string(&sig).unwrap();
        assert!(
            minisign::verify(&pk, &sig_box, Cursor::new(b"tampered"), true, false, false).is_err()
        );
    }

    /// The app parses `src-tauri/tests/index-sample.json`; it has to be what
    /// this tool builds from `tests/sample`. Rewrite it with
    /// `BLOOGBASE_SAMPLE_UPDATE=1 cargo test`.
    #[test]
    fn app_sample_is_current() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut entries = Vec::new();
        let mut files: Vec<_> = std::fs::read_dir(dir.join("tests/sample"))
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        files.sort();
        for f in files {
            entries.push(parse(&std::fs::read_to_string(f).unwrap()).unwrap());
        }
        let json = index_json(&entries, 1_760_000_000);
        let sample = dir.join("../../src-tauri/tests/index-sample.json");
        if std::env::var_os("BLOOGBASE_SAMPLE_UPDATE").is_some() {
            std::fs::write(&sample, &json).unwrap();
        }
        let current = std::fs::read_to_string(&sample)
            .unwrap_or_default()
            .replace("\r\n", "\n");
        assert_eq!(current, json, "run with BLOOGBASE_SAMPLE_UPDATE=1");
    }
}
