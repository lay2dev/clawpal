use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use ed25519_dalek::{pkcs8::DecodePrivateKey, pkcs8::EncodePrivateKey, pkcs8::EncodePublicKey};
use ed25519_dalek::{Signer, SigningKey};
use rand_core::OsRng;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::error::Error;

#[derive(Debug, Clone, PartialEq)]
pub struct DeviceIdentity {
    pub device_id: String,
    pub public_key_pem: String,
    pub private_key_pem: String,
}

pub fn generate_device_identity() -> Result<DeviceIdentity, Error> {
    let signing_key = SigningKey::generate(&mut OsRng);
    let verify_key = signing_key.verifying_key();
    let private_key_der = signing_key
        .to_pkcs8_der()
        .map_err(|err| Error::Crypto(err.to_string()))?;
    let public_key_der = verify_key
        .to_public_key_der()
        .map_err(|err| Error::Crypto(err.to_string()))?;
    Ok(DeviceIdentity {
        device_id: Uuid::new_v4().to_string(),
        public_key_pem: encode_pem("PUBLIC KEY", public_key_der.as_bytes()),
        private_key_pem: encode_pem("PRIVATE KEY", private_key_der.as_bytes()),
    })
}

pub fn build_device_auth_payload_v3(
    device_id: &str,
    client_id: &str,
    client_mode: &str,
    role: &str,
    scopes: &[String],
    signed_at_ms: u64,
    token: Option<&str>,
    nonce: &str,
    platform: &str,
    device_family: Option<&str>,
) -> Value {
    let mut payload = json!({
        "v": 3,
        "deviceId": device_id,
        "clientId": client_id,
        "clientMode": client_mode,
        "role": role,
        "scopes": scopes,
        "signedAtMs": signed_at_ms,
        "nonce": nonce,
        "platform": platform,
    });
    if let Some(token) = token {
        payload["token"] = Value::String(token.to_string());
    }
    if let Some(device_family) = device_family {
        payload["deviceFamily"] = Value::String(device_family.to_string());
    }
    payload
}

pub fn sign_device_payload(private_key_pem: &str, payload: &Value) -> Result<String, Error> {
    let private_key_der = decode_pem("PRIVATE KEY", private_key_pem)?;
    let signing_key = SigningKey::from_pkcs8_der(&private_key_der)
        .map_err(|err| Error::Crypto(err.to_string()))?;
    let payload_bytes = serde_json::to_vec(payload)?;
    let signature = signing_key.sign(&payload_bytes);
    Ok(URL_SAFE_NO_PAD.encode(signature.to_bytes()))
}

fn encode_pem(label: &str, der: &[u8]) -> String {
    let body = base64::engine::general_purpose::STANDARD.encode(der);
    let mut pem = String::new();
    pem.push_str(&format!("-----BEGIN {label}-----\n"));
    for chunk in body.as_bytes().chunks(64) {
        pem.push_str(&String::from_utf8_lossy(chunk));
        pem.push('\n');
    }
    pem.push_str(&format!("-----END {label}-----\n"));
    pem
}

fn decode_pem(label: &str, pem: &str) -> Result<Vec<u8>, Error> {
    let begin = format!("-----BEGIN {label}-----");
    let end = format!("-----END {label}-----");
    let body = pem
        .lines()
        .filter(|line| *line != begin && *line != end)
        .collect::<String>();
    base64::engine::general_purpose::STANDARD
        .decode(body)
        .map_err(|err| Error::Crypto(err.to_string()))
}
