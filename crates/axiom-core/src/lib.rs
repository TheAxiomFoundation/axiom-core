//! Unsigned development bundles and strict execution over the pinned Axiom engine.
pub mod bundle;
pub mod execution;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, io::Read};

pub const ENGINE_REVISION: &str = "d142c645917817cf590e036fb99f99b2d4780e1a";
pub const WIRE_VERSION: &str = "axiom/execution/v0";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EngineIdentity {
    pub repository: String,
    pub revision: String,
    pub version: String,
    pub artifact_format_version: u32,
    pub dependency_lock_sha256: String,
    pub execution_host_sha256: String,
}

/// Conservative identity of this local execution host. Exact executable bytes
/// are deliberately required; this is neither remote attestation nor a promise
/// of identical binaries across builds, machines, profiles, or test harnesses.
pub fn engine_identity() -> Result<EngineIdentity> {
    let executable = std::env::current_exe()
        .map_err(|error| Error::new("engine_identity_unavailable", error.to_string()))?;
    Ok(EngineIdentity {
        repository: "https://github.com/TheAxiomFoundation/axiom-rules-engine".into(),
        revision: ENGINE_REVISION.into(),
        version: axiom_rules_engine::ENGINE_VERSION.into(),
        artifact_format_version: axiom_rules_engine::compile::ARTIFACT_FORMAT_VERSION,
        dependency_lock_sha256: sha256(include_bytes!("../../../Cargo.lock")),
        execution_host_sha256: executable_sha256(&executable)?,
    })
}

fn executable_sha256(path: &std::path::Path) -> Result<String> {
    let file = std::fs::File::open(path).map_err(|error| {
        Error::new(
            "engine_identity_unavailable",
            format!("cannot open execution host {}: {error}", path.display()),
        )
    })?;
    hash_reader(file).map_err(|error| {
        Error::new(
            "engine_identity_unavailable",
            format!("cannot hash execution host {}: {error}", path.display()),
        )
    })
}

fn hash_reader(mut reader: impl Read) -> std::io::Result<String> {
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => hasher.update(&buffer[..count]),
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[derive(Debug, Serialize)]
pub struct Error {
    pub code: String,
    pub message: String,
}

impl Error {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;

pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// v0 canonical JSON: recursively sorted object keys, compact UTF-8 JSON.
/// Manifests contain strings/integer metadata; executable numerics use the
/// pinned engine's JSON representation. This is a versioned local convention.
pub fn canonical_json<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    fn sorted(value: serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Object(values) => {
                let values: BTreeMap<_, _> = values
                    .into_iter()
                    .map(|(key, value)| (key, sorted(value)))
                    .collect();
                serde_json::Value::Object(values.into_iter().collect())
            }
            serde_json::Value::Array(values) => {
                serde_json::Value::Array(values.into_iter().map(sorted).collect())
            }
            other => other,
        }
    }
    let value = serde_json::to_value(value)
        .map_err(|error| Error::new("serialization", error.to_string()))?;
    serde_json::to_vec(&sorted(value))
        .map_err(|error| Error::new("serialization", error.to_string()))
}

pub fn digest<T: Serialize>(value: &T) -> Result<String> {
    Ok(sha256(&canonical_json(value)?))
}

/// Validate the original JSON bytes before any map conversion can discard
/// duplicate keys. All consumers share this boundary, including nested maps.
pub fn validate_json(raw: &str) -> Result<()> {
    use serde::de::{self, MapAccess, SeqAccess, Visitor};
    use std::collections::BTreeSet;

    struct Unique;
    impl<'de> Deserialize<'de> for Unique {
        fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
            struct Check;
            impl<'de> Visitor<'de> for Check {
                type Value = Unique;
                fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                    f.write_str("JSON without duplicate object keys")
                }
                fn visit_map<A: MapAccess<'de>>(
                    self,
                    mut map: A,
                ) -> std::result::Result<Unique, A::Error> {
                    let mut keys = BTreeSet::new();
                    while let Some(key) = map.next_key::<String>()? {
                        if !keys.insert(key.clone()) {
                            return Err(de::Error::custom(format!("duplicate JSON key: {key}")));
                        }
                        map.next_value::<Unique>()?;
                    }
                    Ok(Unique)
                }
                fn visit_seq<A: SeqAccess<'de>>(
                    self,
                    mut seq: A,
                ) -> std::result::Result<Unique, A::Error> {
                    while seq.next_element::<Unique>()?.is_some() {}
                    Ok(Unique)
                }
                fn visit_bool<E: de::Error>(self, _: bool) -> std::result::Result<Unique, E> {
                    Ok(Unique)
                }
                fn visit_i64<E: de::Error>(self, _: i64) -> std::result::Result<Unique, E> {
                    Ok(Unique)
                }
                fn visit_u64<E: de::Error>(self, _: u64) -> std::result::Result<Unique, E> {
                    Ok(Unique)
                }
                fn visit_f64<E: de::Error>(self, _: f64) -> std::result::Result<Unique, E> {
                    Ok(Unique)
                }
                fn visit_str<E: de::Error>(self, _: &str) -> std::result::Result<Unique, E> {
                    Ok(Unique)
                }
                fn visit_none<E: de::Error>(self) -> std::result::Result<Unique, E> {
                    Ok(Unique)
                }
                fn visit_unit<E: de::Error>(self) -> std::result::Result<Unique, E> {
                    Ok(Unique)
                }
            }
            d.deserialize_any(Check)
        }
    }
    let mut deserializer = serde_json::Deserializer::from_str(raw);
    Unique::deserialize(&mut deserializer)
        .map_err(|error| Error::new("invalid_json", error.to_string()))?;
    deserializer
        .end()
        .map_err(|error| Error::new("invalid_json", error.to_string()))
}

pub fn parse_json<T: serde::de::DeserializeOwned>(raw: &str) -> Result<T> {
    validate_json(raw)?;
    serde_json::from_str(raw).map_err(|error| Error::new("invalid_json", error.to_string()))
}

#[cfg(test)]
mod identity_tests {
    use super::*;

    #[test]
    fn identity_binds_embedded_lock_and_readable_execution_host() {
        let identity = engine_identity().unwrap();
        assert_eq!(
            identity.dependency_lock_sha256,
            sha256(include_bytes!("../../../Cargo.lock"))
        );
        assert_eq!(identity.execution_host_sha256.len(), 64);
        assert!(
            identity
                .execution_host_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        );
    }

    #[test]
    fn streaming_hash_propagates_read_errors_without_partial_digest() {
        struct PartialThenError(bool);
        impl Read for PartialThenError {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                if self.0 {
                    return Err(std::io::Error::other("synthetic read failure"));
                }
                self.0 = true;
                buffer[..3].copy_from_slice(b"abc");
                Ok(3)
            }
        }
        assert_eq!(hash_reader(&b"abc"[..]).unwrap(), sha256(b"abc"));
        assert!(hash_reader(PartialThenError(false)).is_err());
    }

    #[test]
    fn missing_execution_host_fails_closed() {
        let nonexistent = std::env::current_exe().unwrap().join("not-a-directory");
        assert_eq!(
            executable_sha256(&nonexistent).unwrap_err().code,
            "engine_identity_unavailable"
        );
    }
}
