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
    if !ciphertext.len().is_multiple_of(16) {
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
    fn key_material_truncates_long_app_ids() {
        let (key, _) = key_material("0123456789abcdefghij");
        assert_eq!(&key, b"0123456789abcdef");
    }

    #[test]
    fn round_trip() {
        let xml = b"<ResList version=\"1.4.16\" tag=\"baseTag\"/>\n";
        let encoded = encode(xml, "1289").unwrap();
        let decoded = decode(&encoded, "1289").unwrap();
        assert_eq!(decoded, xml);
        assert_eq!(
            decode_to_string(&encoded, "1289").unwrap(),
            "<ResList version=\"1.4.16\" tag=\"baseTag\"/>\n"
        );
    }

    /// 用正确的 AES 密钥加密任意字节，用来构造 zlib 层失败的样本。
    fn encrypt_raw(plain: &[u8], app_id: &str) -> Vec<u8> {
        let (key, iv) = key_material(app_id);
        Aes128CbcEnc::new(&key.into(), &iv.into()).encrypt_padded_vec_mut::<Pkcs7>(plain)
    }

    fn with_header(expected: u32, ciphertext: &[u8]) -> Vec<u8> {
        let mut out = MAGIC.to_vec();
        out.push(0);
        out.extend_from_slice(&expected.to_le_bytes());
        out.extend_from_slice(ciphertext);
        out
    }

    #[test]
    fn decode_rejects_malformed_containers() {
        let error = decode(b"tiny", "1289").expect_err("too small");
        assert!(error.to_string().contains("too small"), "{error}");

        let mut bad_magic = with_header(0, &encrypt_raw(b"", "1289"));
        bad_magic[..4].copy_from_slice(b"NOPE");
        let error = decode(&bad_magic, "1289").expect_err("magic");
        assert!(
            error.to_string().contains("bad PatcherXML0 magic"),
            "{error}"
        );

        let mut unaligned = encode(b"<ResList/>", "1289").unwrap();
        unaligned.push(0);
        let error = decode(&unaligned, "1289").expect_err("alignment");
        assert!(error.to_string().contains("not block aligned"), "{error}");
    }

    #[test]
    fn decode_rejects_a_wrong_expected_length() {
        let mut encoded = encode(b"<ResList/>", "1289").unwrap();
        encoded[12..16].copy_from_slice(&999u32.to_le_bytes());
        let error = decode(&encoded, "1289").expect_err("length");
        assert!(error.to_string().contains("length mismatch"), "{error}");
    }

    #[test]
    fn decode_reports_zlib_and_utf8_failures() {
        let garbage = with_header(64, &encrypt_raw(&[0u8; 64], "1289"));
        let error = decode(&garbage, "1289").expect_err("zlib");
        assert!(error.to_string().contains("zlib"), "{error}");

        let encoded = encode(&[0xff, 0xfe, 0xfd], "1289").expect("encode");
        let error = decode_to_string(&encoded, "1289").expect_err("utf8");
        assert!(error.to_string().contains("utf8"), "{error}");
    }

    fn zip_of(entries: &[(&str, Vec<u8>)]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, contents) in entries {
            writer
                .start_file(*name, zip::write::SimpleFileOptions::default())
                .expect("start entry");
            writer.write_all(contents).expect("write entry");
        }
        writer.finish().expect("finish zip").into_inner()
    }

    #[test]
    fn read_zip_manifests_accepts_encoded_and_plain_entries() {
        let encoded = encode(b"<ResList version=\"1\"/>", "1289").expect("encode");
        let zip = zip_of(&[
            ("ResList.bin", encoded),
            ("LastDiff.xml", b"<LastDiff/>".to_vec()),
            ("readme.txt", b"ignored".to_vec()),
        ]);
        let m = read_zip_manifests(&zip, "1289").expect("manifests");
        assert_eq!(m.res_list, "<ResList version=\"1\"/>");
        assert_eq!(m.diff_list.as_deref(), Some("<LastDiff/>"));
    }

    #[test]
    fn read_zip_manifests_allows_a_missing_diff() {
        let zip = zip_of(&[("reslist.xml", b"<ResList/>".to_vec())]);
        let m = read_zip_manifests(&zip, "1289").expect("manifests");
        assert_eq!(m.res_list, "<ResList/>");
        assert!(m.diff_list.is_none());
    }

    #[test]
    fn read_zip_manifests_rejects_bad_archives() {
        let error = read_zip_manifests(b"not a zip", "1289").expect_err("bad zip");
        assert!(error.to_string().contains("open ResList zip"), "{error}");

        let zip = zip_of(&[("LastDiff.bin", b"<LastDiff/>".to_vec())]);
        let error = read_zip_manifests(&zip, "1289").expect_err("no res list");
        assert!(error.to_string().contains("no ResList.bin"), "{error}");

        let zip = zip_of(&[("ResList.bin", vec![0xff, 0xfe])]);
        let error = read_zip_manifests(&zip, "1289").expect_err("bad encoding");
        assert!(
            error.to_string().contains("neither PatcherXML0 nor UTF-8"),
            "{error}"
        );
    }
}
