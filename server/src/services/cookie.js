/**
 * 设置项持久化（含 UC Cookie 的 AES-256-GCM 加密存储）。
 * 密钥：%APPDATA%/uc-drive2/.secret（首次随机生成，随用户数据走，卸载即失）。
 */
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import { DATA_DIR } from '../config.js';

const SECRET_FILE = path.join(DATA_DIR, '.secret');

function loadKey() {
  if (fs.existsSync(SECRET_FILE)) return fs.readFileSync(SECRET_FILE);
  const key = crypto.randomBytes(32);
  fs.writeFileSync(SECRET_FILE, key, { mode: 0o600 });
  return key;
}

export function encryptSecret(plain) {
  const key = loadKey();
  const iv = crypto.randomBytes(12);
  const cipher = crypto.createCipheriv('aes-256-gcm', key, iv);
  const enc = Buffer.concat([cipher.update(plain, 'utf8'), cipher.final()]);
  return `v1:${iv.toString('base64')}:${cipher.getAuthTag().toString('base64')}:${enc.toString('base64')}`;
}

export function decryptSecret(payload) {
  try {
    const [v, ivB64, tagB64, dataB64] = String(payload).split(':');
    if (v !== 'v1') throw new Error('bad format');
    const key = loadKey();
    const decipher = crypto.createDecipheriv('aes-256-gcm', key, Buffer.from(ivB64, 'base64'));
    decipher.setAuthTag(Buffer.from(tagB64, 'base64'));
    return Buffer.concat([decipher.update(Buffer.from(dataB64, 'base64')), decipher.final()]).toString('utf8');
  } catch {
    return '';
  }
}

export function getSetting(db, key, def = '') {
  const row = db.prepare('SELECT value FROM settings WHERE key = ?').get(key);
  return row ? row.value : def;
}

export function setSetting(db, key, value) {
  db.prepare(`
    INSERT INTO settings (key, value, updated_at) VALUES (?, ?, ?)
    ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at
  `).run(key, String(value), new Date().toISOString());
}

export function deleteSetting(db, key) {
  db.prepare('DELETE FROM settings WHERE key = ?').run(key);
}

/** UC Cookie 读写（加密存储） */
export function getUcCookie(db) {
  const enc = getSetting(db, 'uc_cookie');
  return enc ? decryptSecret(enc) : '';
}

export function setUcCookie(db, cookie) {
  setSetting(db, 'uc_cookie', encryptSecret(cookie.trim()));
}

export function hasUcCookie(db) {
  return Boolean(getSetting(db, 'uc_cookie'));
}
