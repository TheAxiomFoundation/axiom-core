use axiom_core::{Error, Result, bundle, engine_identity, execution, parse_json};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    env,
    fs::{self, OpenOptions},
    io::{self, Read, Write},
    path::Path,
};

const MAX_INPUT_BYTES: u64 = 16 * 1024 * 1024;
const HELP: &str = "axiom-core — unsigned development bundles over the real Axiom engine\n\nCommands:\n  build --spec FILE --out FILE\n  verify --bundle FILE --expect SHA256\n  run --bundle FILE --expect SHA256 [--request FILE]\n  capabilities\n\nrun reads native engine request JSON from stdin when --request is omitted.\nReceipts contain private query/trace data. Hash verification is not authentication.\n";

fn read_text(path: Option<&str>) -> Result<String> {
    let mut bytes = Vec::new();
    match path {
        Some(path) => fs::File::open(path)
            .map_err(|e| Error::new("io_error", e.to_string()))?
            .take(MAX_INPUT_BYTES + 1)
            .read_to_end(&mut bytes),
        None => io::stdin()
            .take(MAX_INPUT_BYTES + 1)
            .read_to_end(&mut bytes),
    }
    .map_err(|e| Error::new("io_error", e.to_string()))?;
    if bytes.len() as u64 > MAX_INPUT_BYTES {
        return Err(Error::new("input_too_large", "input exceeds 16 MiB"));
    }
    String::from_utf8(bytes).map_err(|e| Error::new("invalid_utf8", e.to_string()))
}

fn options(args: &[String], allowed: &[&str]) -> Result<BTreeMap<String, String>> {
    let mut result = BTreeMap::new();
    for pair in args.chunks(2) {
        if pair.len() != 2 || !allowed.contains(&pair[0].as_str()) {
            return Err(Error::new(
                "invalid_arguments",
                "unknown option or missing option value",
            ));
        }
        if result.insert(pair[0].clone(), pair[1].clone()).is_some() {
            return Err(Error::new(
                "invalid_arguments",
                format!("duplicate option {}", pair[0]),
            ));
        }
    }
    Ok(result)
}

fn required<'a>(options: &'a BTreeMap<String, String>, name: &str) -> Result<&'a str> {
    options
        .get(name)
        .map(String::as_str)
        .ok_or_else(|| Error::new("invalid_arguments", format!("required option: {name}")))
}

fn capabilities() -> Result<Value> {
    Ok(json!({
        "format":"axiom/capabilities/v0",
        "engine":engine_identity()?,
        "assurance":"development_unsigned",
        "supported":["explicit_module_closure","expected_bundle_digest","offline_rebuild_verification","native_rule_pins","complete_native_traces","strict_request_fields","valid_intervals"],
        "unsupported":["knowledge_time_selection","signed_admission","legal_validation","static_scalar_type_soundness","checked_arithmetic_guarantee","hostile_program_sandbox","durable_jobs","publication"]
    }))
}

fn command(args: &[String]) -> Result<Value> {
    let Some(name) = args.first() else {
        return Err(Error::new("invalid_arguments", HELP));
    };
    match name.as_str() {
        "capabilities" if args.len() == 1 => capabilities(),
        "build" => {
            let opts = options(&args[1..], &["--spec", "--out"])?;
            let spec: bundle::BuildSpec =
                parse_json(&read_text(Some(required(&opts, "--spec")?))?)?;
            let built = bundle::build(&spec)?;
            let bytes = serde_json::to_vec_pretty(&built)
                .map_err(|e| Error::new("serialization", e.to_string()))?;
            // The emitted newline counts toward the same limit used by verify
            // and run. Reject before creating a file that this CLI cannot read.
            if bytes.len() as u64 >= MAX_INPUT_BYTES {
                return Err(Error::new(
                    "bundle_too_large",
                    "serialized bundle exceeds the 16 MiB reader limit",
                ));
            }
            let destination = Path::new(required(&opts, "--out")?);
            // Never overwrite an existing object. A failed/truncated write
            // cannot pass digest verification, and the parent directory must
            // already exist so the CLI doesn't create arbitrary trees.
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(destination)
                .map_err(|e| Error::new("io_error", e.to_string()))?;
            file.write_all(&bytes)
                .and_then(|_| file.write_all(b"\n"))
                .and_then(|_| file.sync_all())
                .map_err(|e| Error::new("io_error", e.to_string()))?;
            Ok(
                json!({"ok":true,"bundle_sha256":built.manifest_sha256,"artifact_sha256":built.manifest.artifact_sha256,"engine":built.manifest.engine,"assurance":built.manifest.assurance}),
            )
        }
        "verify" | "run" => {
            let allowed = if name == "run" {
                vec!["--bundle", "--expect", "--request"]
            } else {
                vec!["--bundle", "--expect"]
            };
            let opts = options(&args[1..], &allowed)?;
            let verified = bundle::load(
                &read_text(Some(required(&opts, "--bundle")?))?,
                required(&opts, "--expect")?,
            )?;
            if name == "verify" {
                Ok(
                    json!({"ok":true,"bundle_sha256":verified.digest(),"artifact_sha256":verified.manifest().artifact_sha256,"engine":verified.manifest().engine,"assurance":verified.manifest().assurance}),
                )
            } else {
                let request = read_text(opts.get("--request").map(String::as_str))?;
                serde_json::to_value(execution::execute(&verified, &request)?)
                    .map_err(|e| Error::new("serialization", e.to_string()))
            }
        }
        _ => Err(Error::new("invalid_arguments", HELP)),
    }
}

fn main() {
    let args: Vec<_> = env::args().skip(1).collect();
    if args.as_slice() == ["--help"] || args.as_slice() == ["-h"] {
        print!("{HELP}");
        return;
    }
    match command(&args) {
        Ok(value) => println!(
            "{}",
            serde_json::to_string_pretty(&value).expect("Value always serializes")
        ),
        Err(error) => {
            eprintln!("{}", json!({"ok":false,"error":error}));
            std::process::exit(1);
        }
    }
}
