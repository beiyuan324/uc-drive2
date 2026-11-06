use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;
use folder_drive_lib::backend::crypto::{decrypt_secret, encrypt_secret};

#[test]
fn os_randomness_preserves_existing_secret_format() {
    let dir = std::env::temp_dir().join(format!(
        "folder-drive-crypto-test-{}-{}",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos(),
    ));
    std::fs::create_dir(&dir).unwrap();
    std::fs::write(dir.join(".secret"), [0u8; 32]).unwrap();
    // Node crypto 独立生成的 AES-256-GCM 样本：零密钥、零 IV，沿用 v1:iv:tag:data。
    let legacy = "v1:AAAAAAAAAAAAAAAA:oYhgM12yeUBAcWaxnZHgfg==:KyL82OPZjfmgqGVvX08S";
    assert_eq!(decrypt_secret(&dir, legacy), "兼容旧格式");
    let first = encrypt_secret(&dir, "中文凭据; token=abc");
    let second = encrypt_secret(&dir, "中文凭据; token=abc");
    assert_ne!(first, second, "每次加密都必须生成新的 IV");
    for payload in [&first, &second] {
        let parts: Vec<_> = payload.split(':').collect();
        assert_eq!(parts.len(), 4);
        assert_eq!(parts[0], "v1");
        let decode = |text| base64::engine::general_purpose::STANDARD.decode(text).unwrap();
        assert_eq!(decode(parts[1]).len(), 12);
        assert_eq!(decode(parts[2]).len(), 16);
        assert_eq!(decrypt_secret(&dir, payload), "中文凭据; token=abc");
    }
    assert_eq!(std::fs::read(dir.join(".secret")).unwrap(), [0u8; 32]);
    std::fs::remove_file(dir.join(".secret")).unwrap();
    let generated = encrypt_secret(&dir, "新密钥");
    assert_eq!(std::fs::read(dir.join(".secret")).unwrap().len(), 32);
    assert_eq!(decrypt_secret(&dir, &generated), "新密钥");
    std::fs::remove_dir_all(dir).unwrap();
}
