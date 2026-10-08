//! Local DM history on native: one directory per account under the app data dir, one file per
//! conversation plus the ordered list of open conversations. Contents are AES-256-CBC with a key
//! derived from the account address and a fresh IV per record, and names are hashes, so nothing
//! on disk is readable or attributable without knowing the account. That is obfuscation, not
//! secrecy: the account address is public.

use std::{
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    sync::Mutex,
};

use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use sha2::{Digest, Sha256};

use crate::DmHistoryEntry;

type Encryptor = cbc::Encryptor<aes::Aes256>;
type Decryptor = cbc::Decryptor<aes::Aes256>;

const CONVERSATIONS_FILE: &str = "conversations";

/// Serializes all history file access; appends from several tasks must not interleave.
static LOCK: Mutex<()> = Mutex::new(());

fn key(account: &str) -> [u8; 32] {
    Sha256::digest(account.to_lowercase().as_bytes()).into()
}

fn hashed_name(key: &[u8; 32], name: &str) -> String {
    let digest = Sha256::digest([key.as_slice(), name.to_lowercase().as_bytes()].concat());
    digest[..12].iter().map(|b| format!("{b:02x}")).collect()
}

fn account_dir(key: &[u8; 32]) -> Result<PathBuf, String> {
    let dirs = super::project_directories().ok_or("no project directories")?;
    Ok(dirs
        .data_dir()
        .join("dm")
        .join(hashed_name(key, CONVERSATIONS_FILE)))
}

fn encrypt_record(key: &[u8; 32], plain: &[u8]) -> String {
    let iv: [u8; 16] = rand::random();
    let cipher = Encryptor::new(key.into(), &iv.into()).encrypt_padded_vec_mut::<Pkcs7>(plain);
    BASE64.encode([iv.as_slice(), &cipher].concat())
}

fn decrypt_record(key: &[u8; 32], line: &str) -> Option<Vec<u8>> {
    let bytes = BASE64.decode(line.trim()).ok()?;
    let (iv, cipher) = bytes.split_at_checked(16)?;
    Decryptor::new(key.into(), iv.into())
        .decrypt_padded_vec_mut::<Pkcs7>(cipher)
        .ok()
}

fn read_records<T: serde::de::DeserializeOwned>(
    key: &[u8; 32],
    path: &PathBuf,
) -> Result<Vec<T>, String> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.to_string()),
    };
    Ok(BufReader::new(file)
        .lines()
        .map_while(Result::ok)
        .filter(|line| !line.is_empty())
        .filter_map(|line| decrypt_record(key, &line))
        .filter_map(|plain| serde_json::from_slice(&plain).ok())
        .collect())
}

fn append_record<T: serde::Serialize>(
    key: &[u8; 32],
    path: &PathBuf,
    record: &T,
) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let plain = serde_json::to_vec(record).map_err(|e| e.to_string())?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    writeln!(file, "{}", encrypt_record(key, &plain)).map_err(|e| e.to_string())
}

pub async fn dm_history_append(
    account: &str,
    partner: &str,
    entry: &DmHistoryEntry,
) -> Result<(), String> {
    let key = key(account);
    let path = account_dir(&key)?.join(hashed_name(&key, partner));
    let _guard = LOCK.lock().map_err(|e| e.to_string())?;
    append_record(&key, &path, entry)
}

pub async fn dm_history_read(account: &str, partner: &str) -> Result<Vec<DmHistoryEntry>, String> {
    let key = key(account);
    let path = account_dir(&key)?.join(hashed_name(&key, partner));
    let _guard = LOCK.lock().map_err(|e| e.to_string())?;
    read_records(&key, &path)
}

pub async fn dm_history_delete(account: &str, partner: &str) -> Result<(), String> {
    let key = key(account);
    let path = account_dir(&key)?.join(hashed_name(&key, partner));
    let _guard = LOCK.lock().map_err(|e| e.to_string())?;
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

/// The partners with stored DMs, oldest conversation first.
pub async fn dm_conversations_read(account: &str) -> Result<Vec<String>, String> {
    let key = key(account);
    let path = account_dir(&key)?.join(CONVERSATIONS_FILE);
    let _guard = LOCK.lock().map_err(|e| e.to_string())?;
    // the list is one record, rewritten whole; the last line wins
    Ok(read_records::<Vec<String>>(&key, &path)?
        .pop()
        .unwrap_or_default())
}

pub async fn dm_conversations_write(account: &str, partners: &[String]) -> Result<(), String> {
    let key = key(account);
    let path = account_dir(&key)?.join(CONVERSATIONS_FILE);
    let _guard = LOCK.lock().map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&path);
    append_record(&key, &path, &partners)
}
