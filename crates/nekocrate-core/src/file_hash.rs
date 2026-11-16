//! One streaming read for hashing and optional copy, with cooperative cancellation.
use anyhow::Result;
use ring::digest::{Context, SHA256};
use sha1::{Digest, Sha1};
use std::io::{Read, Write};
use tokio_util::sync::CancellationToken;

pub(crate) enum Algorithm {
    Sha1,
    Sha256,
    Both,
}
pub(crate) struct Hashes {
    pub sha1: String,
    pub sha256: String,
    pub bytes: u64,
}
impl Hashes {
    pub(crate) fn selected(self, sha256: bool) -> String {
        if sha256 { self.sha256 } else { self.sha1 }
    }
}
pub(crate) fn read(
    input: &mut dyn Read,
    mut output: Option<&mut dyn Write>,
    algorithm: Algorithm,
    cancel: &CancellationToken,
) -> Result<Hashes> {
    let mut sha1 = (!matches!(algorithm, Algorithm::Sha256)).then(Sha1::new);
    let mut sha256 = (!matches!(algorithm, Algorithm::Sha1)).then(|| Context::new(&SHA256));
    let mut buffer = vec![0; 256 * 1024];
    let mut total = 0;
    loop {
        crate::download::check_cancel(cancel)?;
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        let bytes = &buffer[..count];
        total += count as u64;
        if let Some(hash) = &mut sha1 {
            hash.update(bytes);
        }
        if let Some(hash) = &mut sha256 {
            hash.update(bytes);
        }
        if let Some(output) = &mut output {
            output.write_all(bytes)?;
        }
    }
    Ok(Hashes {
        bytes: total,
        sha1: sha1.map(|h| hex::encode(h.finalize())).unwrap_or_default(),
        sha256: sha256.map(|h| hex::encode(h.finish())).unwrap_or_default(),
    })
}
pub(crate) fn sha1(path: &std::path::Path, cancel: &CancellationToken) -> Result<String> {
    Ok(read(
        &mut std::fs::File::open(path)?,
        None,
        Algorithm::Sha1,
        cancel,
    )?
    .sha1)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn combined_hashes_copy_once_and_propagate_cancellation_and_read_failure() {
        let cancel = CancellationToken::new();
        let mut output = Vec::new();
        for (input, sha1, sha256) in [
            (
                b"abc".to_vec(),
                "a9993e364706816aba3e25717850c26c9cd0d89d",
                "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            ),
            (
                vec![b'a'; 1_000_000],
                "34aa973cd4c4daa4f61eeb2bdbad27316534016f",
                "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0",
            ),
        ] {
            for algorithm in [Algorithm::Sha1, Algorithm::Sha256, Algorithm::Both] {
                let has_sha1 = !matches!(algorithm, Algorithm::Sha256);
                let has_sha256 = !matches!(algorithm, Algorithm::Sha1);
                output.clear();
                let hashes =
                    read(&mut input.as_slice(), Some(&mut output), algorithm, &cancel).unwrap();
                assert_eq!(hashes.sha1, if has_sha1 { sha1 } else { "" });
                assert_eq!(hashes.sha256, if has_sha256 { sha256 } else { "" });
                assert_eq!(hashes.bytes, input.len() as u64);
                assert_eq!(output, input);
            }
        }
        struct Broken;
        impl Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("broken source"))
            }
        }
        assert!(read(&mut Broken, None, Algorithm::Both, &cancel).is_err());
        cancel.cancel();
        output.clear();
        assert!(
            read(
                &mut &b"abc"[..],
                Some(&mut output),
                Algorithm::Both,
                &cancel
            )
            .is_err()
        );
        assert!(output.is_empty());
    }
}
