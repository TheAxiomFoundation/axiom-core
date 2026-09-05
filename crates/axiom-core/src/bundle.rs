//! Explicit source closure -> real compiler -> immutable development bundle.
use crate::{
    EngineIdentity, Error, Result, canonical_json, digest, engine_identity, parse_json, sha256,
};
use axiom_rules_engine::{
    compile::{CompileOptions, CompiledProgramArtifact},
    source::{ModuleSource, SourceError},
};
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
};

pub const BUILD_FORMAT: &str = "axiom/build-spec/v0";
pub const BUNDLE_FORMAT: &str = "axiom/development-bundle/v0";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildSpec {
    pub format: String,
    pub root: String,
    pub modules: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BundleManifest {
    pub format: String,
    pub assurance: String,
    pub root: String,
    pub engine: EngineIdentity,
    pub compile_options: BTreeMap<String, bool>,
    pub source_hashes: BTreeMap<String, String>,
    pub source_closure_sha256: String,
    pub artifact_sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bundle {
    pub manifest: BundleManifest,
    pub manifest_sha256: String,
    pub modules: BTreeMap<String, String>,
    /// Exact UTF-8 bytes written by the real compiler's serializer.
    pub artifact_json: String,
}

/// The only facade handle accepted by execution. Its fields are private so
/// every public construction crosses expected-digest and real-loader checks.
pub struct VerifiedBundle {
    manifest: BundleManifest,
    digest: String,
    artifact: CompiledProgramArtifact,
}

impl VerifiedBundle {
    pub fn manifest(&self) -> &BundleManifest {
        &self.manifest
    }
    pub fn digest(&self) -> &str {
        &self.digest
    }
    pub fn artifact(&self) -> &CompiledProgramArtifact {
        &self.artifact
    }
}

struct ExplicitSource<'a> {
    modules: &'a BTreeMap<String, String>,
    loaded: RefCell<BTreeSet<String>>,
}

impl ModuleSource for ExplicitSource<'_> {
    fn load(&self, target: &str) -> std::result::Result<Option<String>, SourceError> {
        self.loaded.borrow_mut().insert(target.to_owned());
        Ok(self.modules.get(target).cloned())
    }
}

fn options() -> CompileOptions {
    CompileOptions {
        strict_namespaces: true,
        strict_relation_entities: true,
    }
}

fn option_manifest() -> BTreeMap<String, bool> {
    BTreeMap::from([
        ("strict_namespaces".into(), true),
        ("strict_relation_entities".into(), true),
    ])
}

fn compile(root: &str, modules: &BTreeMap<String, String>) -> Result<CompiledProgramArtifact> {
    let source = ExplicitSource {
        modules,
        loaded: RefCell::default(),
    };
    let artifact =
        CompiledProgramArtifact::from_rulespec_with_source_and_options(root, &source, options())
            .map_err(|error| Error::new("compile_error", error.to_string()))?;
    let loaded = source.loaded.into_inner();
    let unused: Vec<_> = modules
        .keys()
        .filter(|key| !loaded.contains(*key))
        .cloned()
        .collect();
    if !unused.is_empty() {
        return Err(Error::new(
            "unused_modules",
            format!(
                "modules outside the resolved closure: {}",
                unused.join(", ")
            ),
        ));
    }
    Ok(artifact)
}

fn source_hashes(modules: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    modules
        .iter()
        .map(|(name, bytes)| (name.clone(), sha256(bytes.as_bytes())))
        .collect()
}

fn closure_hash(root: &str, sources: &BTreeMap<String, String>) -> Result<String> {
    digest(&serde_json::json!({"root":root,"source_hashes":sources}))
}

pub fn build(spec: &BuildSpec) -> Result<Bundle> {
    if spec.format != BUILD_FORMAT {
        return Err(Error::new(
            "unsupported_format",
            format!("expected {BUILD_FORMAT}"),
        ));
    }
    let artifact = compile(&spec.root, &spec.modules)?;
    let artifact_json = String::from_utf8(canonical_json(&artifact)?)
        .map_err(|error| Error::new("serialization", error.to_string()))?;
    let source_hashes = source_hashes(&spec.modules);
    let manifest = BundleManifest {
        format: BUNDLE_FORMAT.into(),
        assurance: "development_unsigned".into(),
        root: spec.root.clone(),
        engine: engine_identity()?,
        compile_options: option_manifest(),
        source_closure_sha256: closure_hash(&spec.root, &source_hashes)?,
        source_hashes,
        artifact_sha256: sha256(artifact_json.as_bytes()),
    };
    Ok(Bundle {
        manifest_sha256: digest(&manifest)?,
        manifest,
        modules: spec.modules.clone(),
        artifact_json,
    })
}

pub fn verify(bundle: &Bundle, expected_digest: &str) -> Result<VerifiedBundle> {
    if expected_digest.len() != 64
        || !expected_digest
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        return Err(Error::new(
            "invalid_digest",
            "expected bundle digest must be 64 lowercase hexadecimal characters",
        ));
    }
    if digest(&bundle.manifest)? != expected_digest || bundle.manifest_sha256 != expected_digest {
        return Err(Error::new(
            "bundle_digest_mismatch",
            "manifest differs from caller's expected bundle digest",
        ));
    }
    let manifest = &bundle.manifest;
    if manifest.format != BUNDLE_FORMAT || manifest.assurance != "development_unsigned" {
        return Err(Error::new(
            "unsupported_format",
            "only unsigned development bundles are supported",
        ));
    }
    if manifest.engine != engine_identity()? {
        return Err(Error::new(
            "engine_mismatch",
            "bundle requires a different engine identity",
        ));
    }
    if manifest.compile_options != option_manifest() {
        return Err(Error::new(
            "compile_options_mismatch",
            "bundle requires unsupported compile options",
        ));
    }
    let hashes = source_hashes(&bundle.modules);
    if hashes != manifest.source_hashes
        || closure_hash(&manifest.root, &hashes)? != manifest.source_closure_sha256
    {
        return Err(Error::new(
            "source_digest_mismatch",
            "source bytes or closure differ from the manifest",
        ));
    }
    if sha256(bundle.artifact_json.as_bytes()) != manifest.artifact_sha256 {
        return Err(Error::new(
            "artifact_digest_mismatch",
            "executable bytes differ from the manifest",
        ));
    }
    // The native validated loader rechecks the format, graph, provenance
    // structure, effective ranges and derived metadata. Never bypass it by
    // deserializing the public artifact struct directly.
    crate::validate_json(&bundle.artifact_json)?;
    let artifact =
        CompiledProgramArtifact::from_json_str_with_options(&bundle.artifact_json, options())
            .map_err(|error| Error::new("invalid_artifact", error.to_string()))?;
    // Rebuild from the exact supplied closure. This initial small-bundle
    // verifier checks the source->executable relationship, not just hashes
    // supplied by the bundle author. It is not legal/source authentication.
    let rebuilt = compile(&manifest.root, &bundle.modules)?;
    if canonical_json(&rebuilt)? != bundle.artifact_json.as_bytes() {
        return Err(Error::new(
            "rebuild_mismatch",
            "executable does not match a rebuild of the source closure",
        ));
    }
    Ok(VerifiedBundle {
        manifest: manifest.clone(),
        digest: expected_digest.into(),
        artifact,
    })
}

pub fn load(raw: &str, expected_digest: &str) -> Result<VerifiedBundle> {
    let bundle: Bundle = parse_json(raw)?;
    verify(&bundle, expected_digest)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> BuildSpec {
        parse_json(include_str!("../../../fixtures/synthetic-household.json")).unwrap()
    }

    #[test]
    fn repeat_builds_are_identical_and_verify_offline() {
        let first = build(&fixture()).unwrap();
        let second = build(&fixture()).unwrap();
        assert_eq!(
            canonical_json(&first).unwrap(),
            canonical_json(&second).unwrap()
        );
        let checked = verify(&first, &first.manifest_sha256).unwrap();
        assert_eq!(checked.manifest().source_hashes.len(), 2);
    }

    #[test]
    fn executable_and_source_tampering_are_rejected() {
        let original = build(&fixture()).unwrap();
        let mut changed = original.clone();
        changed.artifact_json.push(' ');
        assert_eq!(
            verify(&changed, &original.manifest_sha256)
                .err()
                .unwrap()
                .code,
            "artifact_digest_mismatch"
        );
        let mut changed = original.clone();
        changed
            .modules
            .values_mut()
            .next()
            .unwrap()
            .push_str("\n# mutation\n");
        assert_eq!(
            verify(&changed, &original.manifest_sha256)
                .err()
                .unwrap()
                .code,
            "source_digest_mismatch"
        );
    }

    #[test]
    fn expected_digest_is_external_to_the_mutable_bundle() {
        let original = build(&fixture()).unwrap();
        let mut changed = original.clone();
        changed.manifest.engine.revision = "0".repeat(40);
        changed.manifest_sha256 = digest(&changed.manifest).unwrap();
        assert_eq!(
            verify(&changed, &original.manifest_sha256)
                .err()
                .unwrap()
                .code,
            "bundle_digest_mismatch"
        );
        assert_eq!(
            verify(&changed, &changed.manifest_sha256)
                .err()
                .unwrap()
                .code,
            "engine_mismatch"
        );
    }

    #[test]
    fn execution_host_digest_mismatch_rejects_even_with_recomputed_manifest_hash() {
        let mut changed = build(&fixture()).unwrap();
        let current = changed.manifest.engine.execution_host_sha256.clone();
        changed.manifest.engine.execution_host_sha256 = if current == "0".repeat(64) {
            "1".repeat(64)
        } else {
            "0".repeat(64)
        };
        changed.manifest_sha256 = digest(&changed.manifest).unwrap();
        assert_eq!(
            verify(&changed, &changed.manifest_sha256)
                .err()
                .unwrap()
                .code,
            "engine_mismatch"
        );
    }

    #[test]
    fn transitive_source_changes_change_identity() {
        let first = build(&fixture()).unwrap();
        let mut spec = fixture();
        spec.modules
            .get_mut("zz:policies/parameters")
            .unwrap()
            .push_str("\n# exact source revision changes\n");
        let second = build(&spec).unwrap();
        assert_ne!(first.manifest_sha256, second.manifest_sha256);
        assert_ne!(
            first.manifest.source_closure_sha256,
            second.manifest.source_closure_sha256
        );
        assert_eq!(
            first.manifest.artifact_sha256,
            second.manifest.artifact_sha256
        );
    }

    #[test]
    fn rehashing_changed_source_cannot_relabel_an_old_executable() {
        let mut changed = build(&fixture()).unwrap();
        let parameters = changed.modules.get_mut("zz:policies/parameters").unwrap();
        *parameters = parameters.replace("200", "201");
        changed.manifest.source_hashes = source_hashes(&changed.modules);
        changed.manifest.source_closure_sha256 =
            closure_hash(&changed.manifest.root, &changed.manifest.source_hashes).unwrap();
        changed.manifest_sha256 = digest(&changed.manifest).unwrap();
        // Even a caller accepting the new manifest cannot execute the stale
        // artifact under the changed source identity.
        assert_eq!(
            verify(&changed, &changed.manifest_sha256)
                .err()
                .unwrap()
                .code,
            "rebuild_mismatch"
        );
    }

    #[test]
    fn missing_and_unused_modules_fail() {
        let mut spec = fixture();
        spec.modules.remove("zz:policies/parameters");
        assert_eq!(build(&spec).err().unwrap().code, "compile_error");
        let mut spec = fixture();
        spec.modules.insert(
            "zz:policies/unused".into(),
            "format: rulespec/v1\nrules: []\n".into(),
        );
        assert_eq!(build(&spec).err().unwrap().code, "unused_modules");
    }

    #[test]
    fn duplicate_keys_and_trailing_json_fail() {
        assert!(
            parse_json::<BuildSpec>(r#"{"format":"a","format":"b","root":"x","modules":{}}"#)
                .is_err()
        );
        assert!(
            parse_json::<BuildSpec>(r#"{"format":"a","root":"x","modules":{"x":"a","x":"b"}}"#)
                .is_err()
        );
        assert!(parse_json::<BuildSpec>(r#"{"format":"a","root":"x","modules":{}} {}"#).is_err());
    }
}
