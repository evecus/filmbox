// crypto.rs —— 对应 瓜子.js 中的 rsaTool / cryptoTool
// AES-128-CBC 加解密 + MD5 签名 + RSA 裸解密（RSA_NO_PADDING），纯 Rust 实现

use aes::Aes128;
use base64::Engine;
use cbc::cipher::{block_padding::Pkcs7, generic_array::GenericArray, BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use md5::{Digest, Md5};

type EncCbc = cbc::Encryptor<Aes128>;
type DecCbc = cbc::Decryptor<Aes128>;

/// MD5 小写十六进制
pub fn md5_hex(text: &str) -> String {
    let mut hasher = Md5::new();
    hasher.update(text.as_bytes());
    hex::encode(hasher.finalize())
}

/// AES-128-CBC 加密，输出大写 HEX（对应 cryptoTool.aesEncrypt）
pub fn aes_encrypt_hex(text: &str, key: &[u8], iv: &[u8]) -> anyhow::Result<String> {
    let enc = EncCbc::new(GenericArray::from_slice(key), GenericArray::from_slice(iv));
    let ct = enc.encrypt_padded_vec_mut::<Pkcs7>(text.as_bytes());
    Ok(hex::encode_upper(ct))
}

/// AES-128-CBC 解密（输入 HEX，对应 cryptoTool.aesDecrypt）
pub fn aes_decrypt_hex(text: &str, key: &[u8], iv: &[u8]) -> anyhow::Result<String> {
    let ct = hex::decode(text.trim())?;
    let dec = DecCbc::new(GenericArray::from_slice(key), GenericArray::from_slice(iv));
    let pt = dec
        .decrypt_padded_vec_mut::<Pkcs7>(&ct)
        .map_err(|e| anyhow::anyhow!("AES解密失败: {e}"))?;
    Ok(String::from_utf8_lossy(&pt).into_owned())
}

/// RSA 裸解密（RSA_NO_PADDING），按 256 字节分块，逐块 c^d mod n，
/// 再按 JS 逻辑去填充：从第 3 字节起找第一个 0x00，取其后的内容（对应 rsaTool.decode）
pub fn rsa_no_padding_decrypt(pem: &str, data_b64: &str) -> anyhow::Result<String> {
    use rsa::pkcs8::DecodePrivateKey;
    use rsa::traits::{PrivateKeyParts, PublicKeyParts};

    let key = rsa::RsaPrivateKey::from_pkcs8_pem(pem)
        .map_err(|e| anyhow::anyhow!("解析RSA私钥失败: {e}"))?;
    let n_len = (key.n().bits() + 7) / 8;
    let d = key.d();

    let cipher = base64::engine::general_purpose::STANDARD.decode(data_b64)?;
    let mut out: Vec<u8> = Vec::with_capacity(cipher.len());

    for chunk in cipher.chunks(n_len) {
        let c = rsa::BigUint::from_bytes_be(chunk);
        let m = c.modpow(d, key.n());
        let dec = m.to_bytes_be();
        let dec = dec.as_slice();

        // JS: realStart = decChunk.indexOf(0, 2)，取 0x00 之后的部分
        let real_start = dec.iter().skip(2).position(|&b| b == 0).map(|p| p + 2);
        let payload: &[u8] = match real_start {
            Some(i) => &dec[i + 1..],
            None => {
                // 兜底：去掉前导零
                let mut start = 0;
                while start < dec.len() && dec[start] == 0 {
                    start += 1;
                }
                &dec[start..]
            }
        };
        out.extend_from_slice(payload);
    }

    Ok(String::from_utf8_lossy(&out).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_md5() {
        assert_eq!(md5_hex("abc"), "900150983cd24fb0d6963f7d28e17f72");
    }

    #[test]
    fn test_aes_roundtrip() {
        let key = b"mvXBSW7ekreItNsT";
        let iv = b"2U3IrJL8szAKp0Fj";
        let ct = aes_encrypt_hex("hello 世界", key, iv).unwrap();
        let pt = aes_decrypt_hex(&ct, key, iv).unwrap();
        assert_eq!(pt, "hello 世界");
    }

    #[test]
    fn test_rsa_no_padding_decrypt() {
        // 用私钥派生公钥，按 PKCS#1 v1.5 加密，再用与瓜子服务端一致的裸解密还原
        use rsa::pkcs8::{EncodePrivateKey, DecodePrivateKey};
        use rsa::traits::PublicKeyParts;
        let mut rng = rand::thread_rng();
        let key = rsa::RsaPrivateKey::new(&mut rng, 1024).unwrap();
        let pem = key.to_pkcs8_pem(rsa::pkcs8::LineEnding::LF).unwrap().to_string();
        let pubkey = rsa::RsaPublicKey::from(&key);
        let msg = b"{\"key\":\"mvXBSW7ekreItNsT\",\"iv\":\"2U3IrJL8szAKp0Fj\"}";
        use rsa::Pkcs1v15Encrypt;
        use rsa::traits::PaddingScheme;
        let ct = Pkcs1v15Encrypt.encrypt(&mut rng, &pubkey, msg).unwrap();
        let b64 = base64::engine::general_purpose::STANDARD.encode(&ct);
        let pt = rsa_no_padding_decrypt(&pem, &b64).unwrap();
        assert_eq!(pt.as_bytes(), msg);
        let _ = key.n().bits(); // PublicKeyParts 已在作用域
    }
}
