//! 设置项持久化 + UC Cookie 的 AES-256-GCM 加密存储。
//! 密钥：%APPDATA%/uc-drive2/.secret（首次随机 32 字节生成，随用户数据走，卸载即失）。
//! 密文格式：`v1:{iv_b64}:{tag_b64}:{data_b64}`（与既有格式完全兼容，可互相解密）。

use rusqlite::Connection;

use super::util::now_iso;

const SECRET_FILE_NAME: &str = ".secret";

fn secret_file(data_dir: &std::path::Path) -> std::path::PathBuf {
    data_dir.join(SECRET_FILE_NAME)
}

fn load_key(data_dir: &std::path::Path) -> [u8; 32] {
    let path = secret_file(data_dir);
    if let Ok(bytes) = std::fs::read(&path) {
        if bytes.len() == 32 {
            let mut key = [0u8; 32];
            key.copy_from_slice(&bytes);
            return key;
        }
    }
    use rand::RngCore;
    let mut key = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut key);
    let _ = std::fs::write(&path, key);
    key
}

fn b64_encode(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(data)
}

fn b64_decode(s: &str) -> Result<Vec<u8>, ()> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(s)
        .map_err(|_| ())
}

/// AES-256-GCM 加密，输出 v1:iv:tag:data 格式（失败返回空串不会发生——输入总是有效）
pub fn encrypt_secret(data_dir: &std::path::Path, plain: &str) -> String {
    use aes_gcm::aead::Aead;
    use aes_gcm::{Aes256Gcm, KeyInit, Nonce};

    let key = load_key(data_dir);
    let cipher = match Aes256Gcm::new_from_slice(&key) {
        Ok(c) => c,
        Err(_) => return String::new(),
    };
    let mut iv = [0u8; 12];
    use rand::Rng;
    rand::thread_rng().fill(&mut iv);
    let nonce = Nonce::from_slice(&iv);
    match cipher.encrypt(nonce, plain.as_bytes()) {
        // aes-gcm crate 输出 ciphertext||tag（tag 固定 16 字节，在末尾）
        Ok(mut ct) => {
            if ct.len() < 16 {
                return String::new();
            }
            let tag = ct.split_off(ct.len() - 16);
            format!(
                "v1:{}:{}:{}",
                b64_encode(&iv),
                b64_encode(&tag),
                b64_encode(&ct)
            )
        }
        Err(_) => String::new(),
    }
}

/// 解密（格式错误/密钥不符/密文损坏 → 空串）
pub fn decrypt_secret(data_dir: &std::path::Path, payload: &str) -> String {
    use aes_gcm::aead::Aead;
    use aes_gcm::{Aes256Gcm, KeyInit, Nonce};

    let parts: Vec<&str> = payload.split(':').collect();
    if parts.len() != 4 || parts[0] != "v1" {
        return String::new();
    }
    let (Ok(iv), Ok(tag), Ok(data)) = (
        b64_decode(parts[1]),
        b64_decode(parts[2]),
        b64_decode(parts[3]),
    ) else {
        return String::new();
    };
    if iv.len() != 12 || tag.len() != 16 {
        return String::new();
    }
    let key = load_key(data_dir);
    let cipher = match Aes256Gcm::new_from_slice(&key) {
        Ok(c) => c,
        Err(_) => return String::new(),
    };
    let mut ct_and_tag = data;
    ct_and_tag.extend_from_slice(&tag);
    match cipher.decrypt(Nonce::from_slice(&iv), ct_and_tag.as_ref()) {
        Ok(plain) => String::from_utf8_lossy(&plain).to_string(),
        Err(_) => String::new(),
    }
}

// ---------- settings 表 ----------

pub fn get_setting(db: &Connection, key: &str, default: &str) -> String {
    db.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
        r.get::<_, String>(0)
    })
    .unwrap_or_else(|_| default.to_string())
}

pub fn set_setting(db: &Connection, key: &str, value: &str) {
    let _ = db.execute(
        "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        rusqlite::params![key, value, now_iso()],
    );
}

pub fn delete_setting(db: &Connection, key: &str) {
    let _ = db.execute("DELETE FROM settings WHERE key = ?1", [key]);
}

// ---------- UC Cookie 读写（加密存储） ----------

pub fn get_uc_cookie(db: &Connection, data_dir: &std::path::Path) -> String {
    let enc = get_setting(db, "uc_cookie", "");
    if enc.is_empty() {
        String::new()
    } else {
        decrypt_secret(data_dir, &enc)
    }
}

pub fn set_uc_cookie(db: &Connection, data_dir: &std::path::Path, cookie: &str) {
    set_setting(db, "uc_cookie", &encrypt_secret(data_dir, cookie.trim()));
}

pub fn has_uc_cookie(db: &Connection) -> bool {
    !get_setting(db, "uc_cookie", "").is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 加解密往返与格式() {
        let dir = tempfile::tempdir().unwrap();
        let secret = "SUBID=xxx; SNUID=yyy; cookie2=zzz";
        set_uc_cookie(&open_test_db(&dir), &dir.path(), secret);
        // 直接调底层：加密 → 格式校验 → 解密还原
        let enc = encrypt_secret(dir.path(), secret);
        assert!(enc.starts_with("v1:"));
        assert_eq!(enc.split(':').count(), 4);
        assert!(!enc.contains("SUBID"), "密文不应包含明文");
        assert_eq!(decrypt_secret(dir.path(), &enc), secret);
    }

    #[test]
    fn 损坏密文返回空串() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(decrypt_secret(dir.path(), "v1:aaaa"), "");
        assert_eq!(decrypt_secret(dir.path(), "v1:aaa:bbb:ccc"), "");
    }

    #[test]
    fn 兼容旧格式密文() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(SECRET_FILE_NAME),
            (0u8..32).collect::<Vec<_>>(),
        )
        .unwrap();
        let payload = "v1:AAECAwQFBgcICQoL:oE2bu1oeIKbccbqm+TUg2A==:K2exeqac73jiLvzi1A==";
        assert_eq!(decrypt_secret(dir.path(), payload), "legacy-cookie");
    }

    fn open_test_db(dir: &tempfile::TempDir) -> Connection {
        let conn = Connection::open(dir.path().join("t.db")).unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL DEFAULT '', updated_at TEXT NOT NULL);",
        )
        .unwrap();
        conn
    }
}
