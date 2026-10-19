use std::io::{Cursor, Read, Write};

use cipher::block_padding::Pkcs7;
use cipher::{BlockDecryptMut, BlockEncryptMut, KeyIvInit};

use crate::{Error, Result};

type Aes128CbcDec = cbc::Decryptor<aes::Aes128>;
type Aes128CbcEnc = cbc::Encryptor<aes::Aes128>;

const MAGIC: &[u8; 11] = b"PatcherXML0";

#[derive(Debug, Clone)]
pub struct ZipManifests {
    pub res_list: String,
    pub diff_list: Option<String>,
}

pub fn key_material(app_id: &str) -> ([u8; 16], [u8; 16]) {
    let mut key = [b'0'; 16];
    let mut iv = [b'0'; 16];
    let source = format!("{app_id}@Patcher");
    let bytes = source.as_bytes();
    let n = bytes.len().min(16);
    key[..n].copy_from_slice(&bytes[..n]);
    let iv_source = b"PatcherSDK";
    let n = iv_source.len().min(16);
    iv[..n].copy_from_slice(&iv_source[..n]);
    (key, iv)
}

pub fn decode(bytes: &[u8], app_id: &str) -> Result<Vec<u8>> {
    if bytes.len() < 17 {
        return Err(Error::Parse("PatcherXML0 file is too small".into()));
    }
    if &bytes[..11] != MAGIC {
        return Err(Error::Parse("bad PatcherXML0 magic".into()));
    }
    let expected = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    let ciphertext = &bytes[16..];
    if ciphertext.len() % 16 != 0 {
        return Err(Error::Parse(
            "PatcherXML0 ciphertext is not block aligned".into(),
        ));
    }
    let (key, iv) = key_material(app_id);
    let mut buf = ciphertext.to_vec();
    let plain = Aes128CbcDec::new(&key.into(), &iv.into())
        .decrypt_padded_mut::<Pkcs7>(&mut buf)
        .map_err(|e| Error::Parse(format!("PatcherXML0 AES: {e}")))?;

    let mut decoder = flate2::read::ZlibDecoder::new(plain);
    let mut out = Vec::with_capacity(expected);
    decoder
        .by_ref()
        .take(64 * 1024 * 1024)
        .read_to_end(&mut out)
        .map_err(|e| Error::Parse(format!("PatcherXML0 zlib: {e}")))?;
    if out.len() != expected {
        return Err(Error::Parse(format!(
            "PatcherXML0 length mismatch: expected {expected}, got {}",
            out.len()
        )));
    }
    Ok(out)
}

pub fn decode_to_string(bytes: &[u8], app_id: &str) -> Result<String> {
    let plain = decode(bytes, app_id)?;
    String::from_utf8(plain).map_err(|e| Error::Parse(format!("PatcherXML0 utf8: {e}")))
}

pub fn encode(plain: &[u8], app_id: &str) -> Result<Vec<u8>> {
    let (key, iv) = key_material(app_id);
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    encoder
        .write_all(plain)
        .map_err(|e| Error::Parse(format!("PatcherXML0 zlib encode: {e}")))?;
    let compressed = encoder
        .finish()
        .map_err(|e| Error::Parse(format!("PatcherXML0 zlib encode: {e}")))?;
    let ciphertext =
        Aes128CbcEnc::new(&key.into(), &iv.into()).encrypt_padded_vec_mut::<Pkcs7>(&compressed);
    let mut out = Vec::with_capacity(16 + ciphertext.len());
    out.extend_from_slice(MAGIC);
    out.push(0);
    out.extend_from_slice(&(plain.len() as u32).to_le_bytes());
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

pub fn read_zip_manifests(zip_bytes: &[u8], app_id: &str) -> Result<ZipManifests> {
    let cursor = Cursor::new(zip_bytes);
    let mut archive =
        zip::ZipArchive::new(cursor).map_err(|e| Error::Parse(format!("open ResList zip: {e}")))?;
    let mut res_list = None;
    let mut diff_list = None;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| Error::Parse(format!("zip entry {i}: {e}")))?;
        let name = entry.name().to_ascii_lowercase();
        let is_res = name.ends_with("reslist.bin") || name.ends_with("reslist.xml");
        let is_diff = name.ends_with("lastdiff.bin") || name.ends_with("lastdiff.xml");
        if !is_res && !is_diff {
            continue;
        }
        let mut bytes = Vec::new();
        entry
            .read_to_end(&mut bytes)
            .map_err(|e| Error::Parse(format!("read zip entry {}: {e}", entry.name())))?;
        let text = if bytes.starts_with(MAGIC) {
            decode_to_string(&bytes, app_id)?
        } else {
            String::from_utf8(bytes).map_err(|e| {
                Error::Parse(format!(
                    "manifest {} is neither PatcherXML0 nor UTF-8: {e}",
                    entry.name()
                ))
            })?
        };
        if is_res {
            res_list = Some(text);
        } else {
            diff_list = Some(text);
        }
    }
    let res_list = res_list.ok_or_else(|| Error::Parse("ResList zip has no ResList.bin".into()))?;
    Ok(ZipManifests {
        res_list,
        diff_list,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_padding() {
        let (key, iv) = key_material("1289");
        assert_eq!(&key[..12], b"1289@Patcher");
        assert_eq!(&key[12..], b"0000");
        assert_eq!(&iv[..10], b"PatcherSDK");
        assert_eq!(&iv[10..], b"000000");
    }

    #[test]
    fn round_trip() {
        let xml = b"<ResList version=\"1.4.16\" tag=\"baseTag\"/>\n";
        let encoded = encode(xml, "1289").unwrap();
        let decoded = decode(&encoded, "1289").unwrap();
        assert_eq!(decoded, xml);
    }
}
