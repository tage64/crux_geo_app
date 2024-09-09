//! Functions to handle persistant (local) storage.
//!
//! > LocalStorage stores data in the browser with no expiration time. Access is given to all pages
//! from the same origin (e.g., all pages from “https://example.com” share the same origin). While
//! data doesn’t expire the user can view, modify and delete all data stored. Browsers allow 5MB of
//! data to be stored.

use base64::prelude::*;
use codee::{Decoder, Encoder};
use leptos::signal_prelude::*;
use leptos_use::storage::use_local_storage;

/// A base64 encoder/decoder.
///
/// The builtin base64-type in codee doesn't suffice as it must be built on a bytes
/// encoder/decoder, but we are encoding/decoding from bytes directly.
struct Base64Codee;

enum Never {}

impl Encoder<Vec<u8>> for Base64Codee {
    type Encoded = String;
    type Error = Never;
    fn encode(val: &Vec<u8>) -> Result<Self::Encoded, Self::Error> {
        Ok(BASE64_STANDARD.encode(val))
    }
}

impl Decoder<Vec<u8>> for Base64Codee {
    type Encoded = str;
    type Error = base64::DecodeError;
    fn decode(val: &Self::Encoded) -> Result<Vec<u8>, Self::Error> {
        BASE64_STANDARD.decode(val)
    }
}

/// Get a value from persistant storage.
pub fn get(key: impl AsRef<str>) -> Option<Vec<u8>> {
    let (get_signal, _, _) = use_local_storage::<_, Base64Codee>(key);
    let value = get_signal.get();
    if value.is_empty() { None } else { Some(value) }
}

/// Set a value to persistant storage.
pub fn set(key: impl AsRef<str>, value: Vec<u8>) {
    let (_, set_signal, _) = use_local_storage::<_, Base64Codee>(key);
    set_signal.set(value);
}

/// Delete from persistant storage.
pub fn delete(key: impl AsRef<str>) {
    let (_, _, delete_fn) = use_local_storage::<_, Base64Codee>(key);
    delete_fn();
}
