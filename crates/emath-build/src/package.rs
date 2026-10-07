//! Package build orchestration: compose, stage, cargo, verify.

use super::{
    ARTIFACT_MANIFEST_SCHEMA, ArtifactClass, ArtifactManifest, BTreeMap, BackendInput,
    BackendOutput, BuildError, BuildOptions, BuildReport, COMPILER_DESCRIPTOR, ClaimVerdict,
    Component, CrateProfile, Diagnostics, EVIDENCE_BUNDLE_SCHEMA, EvidenceBundleRecord,
    EvidenceClaim, EvidenceLevel, Path, PathBuf, PlanRecord, ResolutionPlan, SOURCE_MAP_SCHEMA,
    SchemaId, SourceMap, SourceMapEntry, StagedFile, content_id_of_str, generated_crate_target_dir,
    manifest_identity, plan_to_record, publish, required_artifact_paths, stage, verify_artifact,
    write_artifact_manifest, write_evidence_bundle, write_resolution_plan, write_source_map,
};

/// Artifact pipeline over an already-elaborated package
/// (programmatic models and macro-expanded sources use this exact path:
/// same schema/sema/plan/artifact flow as `.emath` text).
pub fn build_package(
    package: &emath_ir::SemanticPackage,
    source_name: &str,
    diagnostics: &Diagnostics,
    plans: &[ResolutionPlan],
    target_dir: &Path,
    options: BuildOptions,
) -> Result<BuildReport, BuildError> {
    std::fs::create_dir_all(target_dir).map_err(|error| {
        BuildError::Io(format!("cannot create {}: {error}", target_dir.display()))
    })?;
    let package_id = package.content_id();
    let crate_name = package
        .identity
        .as_ref()
        .map_or_else(|| "package".to_string(), |id| id.name.clone());
    let backend = BackendInput {
        package,
        crate_name: crate_name.clone(),
        version: "0.1.0".to_string(),
    };
    let output = backend
        .generate()
        .map_err(|error| BuildError::Backend(error.to_string()))?;

    // Profile validation on the build path: the generated module must be
    // safe (E-CODEGEN-002) and every public item source-anchored
    // (E-CODEGEN-004) before any artifact is staged.
    let profile = CrateProfile::Library;
    if let Some(problem) = profile.validate(&output.module).first() {
        return Err(BuildError::Backend(format!(
            "generated module failed profile validation: {}: {problem:?}",
            problem.code()
        )));
    }

    // --- verification lane -----------------------------------------------
    let verify_ok = if options.verify_generated_crate {
        match verify_crate(&target_dir.join("verify"), &output) {
            Ok(()) => true,
            Err(error) => return Err(BuildError::VerifyFailed(error)),
        }
    } else {
        false
    };

    let meta = ComposeMeta {
        source_name,
        crate_name: &crate_name,
        package_id: &package_id,
    };
    let artifact = compose_artifact(&meta, package, plans, &output, diagnostics, verify_ok)?;

    // The one identity is the manifest-body hash (`manifest_identity`),
    // frozen over the *resolved* manifest (every file fingerprint and
    // referenced-document id filled in), so the independent checker
    // recomputes the exact same value from the published document
    // The content fingerprint computed by `stage` only names
    // the staging union and is never advertised as the artifact identity.
    let mut manifest = artifact.manifest.clone();
    let manifest_files = stage_files(&output, &artifact, &mut manifest);
    let staging = stage(&manifest_files, None)?;
    let artifact_id = manifest.artifact_id.clone();
    if manifest_identity(&manifest) != artifact_id {
        return Err(BuildError::Io(
            "internal: artifact identity did not recompute after manifest staging".to_string(),
        ));
    }
    let destination = publish(target_dir, &artifact_id, &manifest_files)?;
    verify_artifact(&destination, &staging)?;

    // Compiled function-spec probe (emath-bta82): a SIBLING of the
    // published artifact, written and compiled after publish so the
    // artifact's staged file set (its identity) is untouched.
    let probe_binary = match options.bin_entrypoint.as_deref() {
        Some(entrypoint) => Some(
            super::probe::emit_compiled_probe(
                package,
                &crate_name,
                &destination,
                entrypoint,
                target_dir,
            )?
            .binary_path,
        ),
        None => None,
    };

    Ok(BuildReport {
        artifact_dir: destination,
        package_id,
        artifact_id,
        crate_name,
        plan_ids: plans.iter().map(|p| p.plan_id.0.clone()).collect(),
        assumptions: output.assumptions.clone(),
        exports: artifact.manifest.public_exports.clone(),
        refusal_codes: Vec::new(),
        probe_binary,
    })
}

#[derive(Clone)]
pub(super) struct ComposedArtifact {
    manifest: ArtifactManifest,
    source_map: SourceMap,
    plans: Vec<PlanRecord>,
    evidence: EvidenceBundleRecord,
}

/// Identity/name context shared by the artifact documents.
pub(super) struct ComposeMeta<'a> {
    source_name: &'a str,
    crate_name: &'a str,
    package_id: &'a emath_core::ContentId,
}

/// Build the four durable JSON documents from the pipeline outputs.
pub(super) fn compose_artifact(
    meta: &ComposeMeta<'_>,
    package: &emath_ir::SemanticPackage,
    plans: &[ResolutionPlan],
    output: &BackendOutput,
    diagnostics: &Diagnostics,
    verification_ran: bool,
) -> Result<ComposedArtifact, BuildError> {
    let declaration = package
        .declarations
        .first()
        .ok_or_else(|| BuildError::Backend("package has no declarations".to_string()))?;

    let target = package.goals.first().map_or_else(
        || emath_ir::TargetProfile {
            family: "rust-library".to_string(),
            triple: None,
            features: vec![],
        },
        |goal| goal.requirements.target.clone(),
    );
    // Goal requirement vs what this native path actually delivers.
    // Admission is E1 (sema/admit); cargo-test verification is E3. A
    // not-run verification claim must not advertise E3 (overclaim across
    // build→checker→evidence). Manifest `evidence_level` records delivered
    // strength, never an unmet goal ceiling.
    let required_evidence = package
        .goals
        .first()
        .map_or(EvidenceLevel::E1, |goal| goal.requirements.evidence);
    let admit_level = EvidenceLevel::E1;
    let verify_level = if verification_ran {
        EvidenceLevel::E3
    } else {
        EvidenceLevel::E0
    };
    let evidence_level = if verification_ran {
        EvidenceLevel::E3
    } else {
        EvidenceLevel::E1
    };
    if required_evidence > evidence_level {
        return Err(BuildError::Backend(format!(
            "E-EVID-103: goal requires {} but native build delivers only {}{}",
            required_evidence.as_str(),
            evidence_level.as_str(),
            if verification_ran {
                ""
            } else {
                " (enable verify_generated_crate for E3)"
            },
        )));
    }
    let public_exports: Vec<String> = declaration.exports.iter().map(|e| e.name.clone()).collect();

    // Live build source map: every entry carries the source FileId, the
    // semantic node anchor and the plan node it came from, so the map
    // round-trips through the checker with FileId and generated ranges.
    let plan_node: Option<String> = plans.first().map(|plan| plan.root.0.to_string());
    let source_map = SourceMap {
        schema: SchemaId(SOURCE_MAP_SCHEMA.to_string()),
        source_package: meta.package_id.clone(),
        entries: output
            .anchors
            .iter()
            .map(|anchor| SourceMapEntry {
                file: declaration.source.file.0,
                source_file: meta.source_name.to_string(),
                source_start: u64::from(declaration.source.start),
                source_end: u64::from(declaration.source.end),
                semantic_node: anchor.label.clone(),
                plan_node: plan_node.clone(),
                generated_file: anchor.file.clone(),
                generated_start: u64::from(anchor.start),
                generated_end: u64::from(anchor.end),
                generated_symbol: anchor_label_symbol(&anchor.label),
            })
            .collect(),
    };
    let plans_recorded: Vec<PlanRecord> = plans.iter().map(plan_to_record).collect();

    // Claims: honest about what ran. Unverified steps are `not-run` at E0.
    let mut claims = vec![EvidenceClaim {
        id: format!("{}.admitted", meta.package_id.0),
        statement: format!("`{0}` was admitted without errors", meta.crate_name),
        class: "static-semantics".to_string(),
        scope: meta.crate_name.to_string(),
        assumptions: vec!["strict-f64".to_string()],
        producer: COMPILER_DESCRIPTOR.to_string(),
        checker: Some("emath-sema/admit".to_string()),
        verdict: if diagnostics.has_errors() {
            ClaimVerdict::Fail
        } else {
            ClaimVerdict::Pass
        },
        level: admit_level,
        falsifiers: vec![],
        artifacts: vec!["emath/resolution-plan.json".to_string()],
        fresh_until: None,
    }];
    let verification_claim = EvidenceClaim {
        id: format!("{}.generated-crate", meta.package_id.0),
        statement: format!(
            "the generated `{0}` crate verifies: deterministic content ids, forbidden unsafe",
            meta.crate_name,
        ),
        class: "codegen".to_string(),
        scope: meta.crate_name.to_string(),
        assumptions: output.assumptions.clone(),
        producer: COMPILER_DESCRIPTOR.to_string(),
        checker: Some(
            if verification_ran {
                "cargo-test"
            } else {
                "emath-artifact/verify"
            }
            .to_string(),
        ),
        verdict: if verification_ran {
            ClaimVerdict::Pass
        } else {
            ClaimVerdict::NotRun
        },
        level: verify_level,
        falsifiers: vec![],
        artifacts: vec![
            "emath/artifact-manifest.json".to_string(),
            "emath/source-map.json".to_string(),
        ],
        fresh_until: None,
    };
    claims.push(verification_claim);

    Ok(ComposedArtifact {
        manifest: ArtifactManifest {
            schema: SchemaId(ARTIFACT_MANIFEST_SCHEMA.to_string()),
            artifact_id: emath_core::ContentId(String::new()), // filled by caller
            class: ArtifactClass::Native,
            source_package: meta.package_id.clone(),
            compiler: content_id_of_str(COMPILER_DESCRIPTOR),
            target,
            numeric_profile: "strict-f64".to_string(),
            providers: Vec::new(), // provider-free
            evidence_level,
            public_exports,
            assumptions: output.assumptions.clone(),
            files: BTreeMap::new(), // filled by stage_files
            source_map: emath_core::ContentId(String::new()),
            resolution_plan: emath_core::ContentId(String::new()),
            evidence_bundle: emath_core::ContentId(String::new()),
        },
        source_map,
        plans: plans_recorded,
        evidence: EvidenceBundleRecord {
            schema: SchemaId(EVIDENCE_BUNDLE_SCHEMA.to_string()),
            bundle_id: emath_core::ContentId(String::new()),
            source_package: meta.package_id.clone(),
            resolution_plan: emath_core::ContentId(String::new()),
            claims,
            artifact_paths: required_artifact_paths()
                .iter()
                .map(|p| (*p).to_string())
                .collect(),
            reproduction: vec![
                "emath build <spec> --out <dir>".to_string(),
                COMPILER_DESCRIPTOR.to_string(),
            ],
        },
    })
}

pub(super) fn anchor_label_symbol(label: &str) -> Option<String> {
    let parts: Vec<&str> = label.split(' ').collect();
    if parts.len() == 2 && matches!(parts[0], "fn" | "test" | "struct" | "impl") {
        Some(parts[1].to_string())
    } else {
        None
    }
}

pub(super) fn stage_files(
    output: &BackendOutput,
    artifact: &ComposedArtifact,
    manifest: &mut ArtifactManifest,
) -> Vec<StagedFile> {
    let mut files: Vec<StagedFile> = output
        .files
        .iter()
        .map(|(path, text)| StagedFile {
            relative_path: path.clone(),
            bytes: text.as_bytes().to_vec(),
        })
        .collect();

    let source_map_text = write_source_map(&artifact.source_map);
    files.push(StagedFile {
        relative_path: "emath/source-map.json".to_string(),
        bytes: source_map_text.as_bytes().to_vec(),
    });
    let plan_text = serialize_plans(&artifact.plans);
    files.push(StagedFile {
        relative_path: "emath/resolution-plan.json".to_string(),
        bytes: plan_text.as_bytes().to_vec(),
    });

    // ids for the documents the manifest references
    let source_map_id = content_id_of_str(&source_map_text);
    let plan_id = content_id_of_str(&plan_text);

    // The evidence bundle's own ids are content-derived at stage time:
    // `resolution_plan` points at the plan document, `bundle_id` is a hash
    // of the bundle body (with the id itself unfilled), so the durable
    // document is self-describing and never carries an empty id field
    // (the checker reads every document back).
    let mut evidence = artifact.evidence.clone();
    if evidence.resolution_plan.0.is_empty() {
        evidence.resolution_plan = plan_id.clone();
    }
    if evidence.bundle_id.0.is_empty() {
        evidence.bundle_id = emath_core::ContentId(String::new());
        let body = write_evidence_bundle(&evidence);
        evidence.bundle_id = content_id_of_str(&body);
    }
    let evidence_text = write_evidence_bundle(&evidence);
    files.push(StagedFile {
        relative_path: "emath/evidence-bundle.json".to_string(),
        bytes: evidence_text.as_bytes().to_vec(),
    });
    let evidence_id = content_id_of_str(&evidence_text);

    // The manifest references every other file by fingerprint. The manifest
    // itself is excluded from its own `files` map (self-referential ids are
    // unstable), so this resolves in one pass.
    manifest.files = output
        .files
        .iter()
        .map(|(path, text)| (path.clone(), content_id_of_str(text)))
        .chain(std::iter::once((
            "emath/source-map.json".to_string(),
            source_map_id.clone(),
        )))
        .chain(std::iter::once((
            "emath/resolution-plan.json".to_string(),
            plan_id.clone(),
        )))
        .chain(std::iter::once((
            "emath/evidence-bundle.json".to_string(),
            evidence_id.clone(),
        )))
        .collect();
    manifest.source_map = source_map_id;
    manifest.resolution_plan = plan_id;
    manifest.evidence_bundle = evidence_id;
    // Freeze the one artifact identity over this resolved body: it
    // excludes `artifact_id` itself and the manifest's own entry, so the
    // parsed-back document recomputes to the identical value.
    manifest.artifact_id = manifest_identity(manifest);
    let final_manifest_text = write_artifact_manifest(manifest);
    files.push(StagedFile {
        relative_path: "emath/artifact-manifest.json".to_string(),
        bytes: final_manifest_text.as_bytes().to_vec(),
    });
    files
}

pub(super) fn serialize_plans(plans: &[PlanRecord]) -> String {
    let mut out = String::new();
    for (index, plan) in plans.iter().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        out.push_str(&write_resolution_plan(plan));
    }
    out
}

/// Run a Cargo command under a wall-clock budget, including stdio drain.
/// Inherited output handles cannot extend the budget after Cargo exits.
#[cfg(unix)]
pub fn run_cargo_timed(
    mut command: std::process::Command,
    timeout: std::time::Duration,
) -> Result<std::process::Output, String> {
    use std::io::{ErrorKind, Read};
    use std::os::fd::OwnedFd;
    use std::os::unix::net::UnixStream;
    use std::process::Stdio;
    use std::time::{Duration, Instant};

    let started = Instant::now();
    // Only the host endpoints are nonblocking. Socket pairs provide safe,
    // cancellable byte streams without unsafe fcntl or reader threads that
    // can outlive the run when descendants retain inherited descriptors.
    let output_pair = || -> std::io::Result<(UnixStream, Stdio)> {
        let (host, child) = UnixStream::pair()?;
        host.set_nonblocking(true)?;
        Ok((host, Stdio::from(OwnedFd::from(child))))
    };
    let (mut stdout_pipe, stdout_stdio) =
        output_pair().map_err(|error| format!("cannot create stdout: {error}"))?;
    let (mut stderr_pipe, stderr_stdio) =
        output_pair().map_err(|error| format!("cannot create stderr: {error}"))?;
    // Generated crates can live outside the workspace's Cargo configuration.
    // Bound their default fan-out without overriding an explicit caller choice.
    for key in ["CARGO_BUILD_JOBS", "RUST_TEST_THREADS"] {
        if !command.get_envs().any(|(name, _)| name == key) {
            command.env(key, std::env::var_os(key).unwrap_or_else(|| "1".into()));
        }
    }
    // Stay in the terminal's foreground group so Ctrl-C / SIGTERM reach
    // cargo and rustc. On timeout, kill cargo's children first (rustc),
    // then cargo: Child::kill alone leaves the compiler holding
    // CARGO_TARGET_DIR.
    let mut child = command
        .stdout(stdout_stdio)
        .stderr(stderr_stdio)
        .stdin(Stdio::null())
        .spawn()
        .map_err(|error| format!("cannot spawn cargo: {error}"))?;
    // Command retains caller-supplied descriptors for reuse. Closing those
    // copies here is necessary for EOF when the child closes its own copies.
    drop(command);
    let mut status = None;
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let (mut stdout_done, mut stderr_done) = (false, false);
    let mut chunk = [0u8; 8192];
    loop {
        if status.is_none() {
            match child.try_wait() {
                Ok(exit) => status = exit,
                Err(error) => {
                    kill_timed_child(&mut child);
                    let _ = child.wait();
                    return Err(format!("cannot wait on cargo: {error}"));
                }
            }
        }
        let mut progress = false;
        for (pipe, buffer, done) in [
            (&mut stdout_pipe, &mut stdout, &mut stdout_done),
            (&mut stderr_pipe, &mut stderr, &mut stderr_done),
        ] {
            if *done {
                continue;
            }
            match pipe.read(&mut chunk) {
                Ok(0) => *done = true,
                Ok(n) => {
                    buffer.extend_from_slice(&chunk[..n]);
                    progress = true;
                }
                Err(error)
                    if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::Interrupted) => {}
                Err(_) => *done = true,
            }
        }
        if let Some(status) = status.filter(|_| stdout_done && stderr_done) {
            return Ok(std::process::Output {
                status,
                stdout,
                stderr,
            });
        }
        if started.elapsed() >= timeout {
            // Never sweep a reaped pid: it may already belong to an unrelated
            // process. Open descendants' descriptors are simply closed here.
            if status.is_none() {
                kill_timed_child(&mut child);
                let _ = child.wait();
            }
            return Err(format!(
                "E-RES-120: cargo exceeded the {timeout:?} wall-clock budget"
            ));
        }
        if !progress {
            std::thread::sleep(
                (timeout - started.elapsed().min(timeout)).min(Duration::from_millis(5)),
            );
        }
    }
}

/// Refuse rather than spawn when safe interruptible stdio is unavailable.
#[cfg(not(unix))]
pub fn run_cargo_timed(
    _command: std::process::Command,
    _timeout: std::time::Duration,
) -> Result<std::process::Output, String> {
    Err("bounded process stdio requires Unix socket pairs on this host".into())
}

/// Join `relative` under `root`, rejecting absolute paths and `..` / prefix
/// / root components (same policy as artifact staging path checks).
pub(super) fn safe_generated_join(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let rel = Path::new(relative);
    if rel.is_absolute() {
        return Err(format!(
            "refusing absolute generated path `{relative}` outside verify root"
        ));
    }
    for component in rel.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            Component::Prefix(_) | Component::RootDir | Component::ParentDir => {
                return Err(format!(
                    "refusing unsafe generated path `{relative}` (traversal or absolute component)"
                ));
            }
        }
    }
    Ok(root.join(rel))
}

pub(super) fn kill_timed_child(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        // try_wait already proved this pid is still our child. Kill rustc
        // (and other direct children) first; then SIGKILL cargo. A new
        // process group would orphan the same tree on Ctrl-C.
        let pid = child.id();
        let _ = std::process::Command::new("pkill")
            .args(["-9", "-P", &pid.to_string()])
            .status();
        let _ = child.kill();
    }
    #[cfg(not(unix))]
    {
        let _ = child.kill();
    }
}

pub(super) fn verify_crate(dir: &Path, output: &BackendOutput) -> Result<(), String> {
    // Mock-free honesty (E-TLT-012): refuse to report "tests passed" for a
    // crate with no `#[test]` functions. A spec without a `tests:` section
    // must drop `--verify` instead of trusting a vacuous `cargo test`.
    let test_file_count = output
        .files
        .values()
        .filter(|text| text.contains("#[test]"))
        .count();
    if test_file_count == 0 {
        return Err(
            "E-TLT-012: generated crate has no `#[test]` tests; --verify refuses an empty \
test surface (add a `tests:` section to the spec, or drop --verify)"
                .to_string(),
        );
    }
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    for (path, text) in &output.files {
        // Refuse absolute / `..` segments so a malicious or buggy backend map
        // cannot write outside the verify staging directory.
        let target = safe_generated_join(dir, path)?;
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        std::fs::write(target, text).map_err(|error| error.to_string())?;
    }
    // E-TLT-012: `--verify` promises a real test surface. A generated crate
    // with no `#[test]` functions would make `cargo test` vacuous; refuse it
    // instead of pretending coverage.
    let mut test_count = 0_usize;
    for (path, text) in &output.files {
        if std::path::Path::new(path)
            .extension()
            .is_some_and(|ext| ext == "rs")
        {
            test_count += text.matches("#[test]").count();
        }
    }
    if test_count == 0 {
        return Err("E-TLT-012: generated crate has no `#[test]` tests; --verify refuses an empty test surface (add a `tests:` section to the spec, or drop --verify)".to_string());
    }
    let key = output
        .files
        .get("src/lib.rs")
        .map_or_else(|| "verify".to_string(), |text| content_id_of_str(text).0);
    let mut command = std::process::Command::new("cargo");
    command
        .args(["test", "--quiet", "--lib", "--bins", "--tests"])
        .env("CARGO_TARGET_DIR", generated_crate_target_dir(&key))
        .current_dir(dir);
    let result = run_cargo_timed(command, std::time::Duration::from_secs(600))
        .map_err(|error| format!("cannot spawn cargo: {error}"))?;
    if !result.status.success() {
        let stdout = String::from_utf8_lossy(&result.stdout);
        let stderr = String::from_utf8_lossy(&result.stderr);
        return Err(format!(
            "cargo test exited {:?}\n{stdout}\n{stderr}",
            result.status.code()
        ));
    }
    Ok(())
}
