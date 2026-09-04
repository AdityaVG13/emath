//! Lock persistence, verification, and portfolio capping.

use super::*;

impl MeaningLock {
    /// Empty lock with the default cap.
    #[must_use]
    pub fn empty() -> Self {
        Self::with_cap(DEFAULT_PORTFOLIO_CAP)
    }

    /// Empty lock with an explicit cap (receipted configuration).
    #[must_use]
    pub fn with_cap(portfolio_cap: u32) -> Self {
        let mut lock = Self {
            portfolio_cap,
            entries: BTreeMap::new(),
            lock_id: 0,
        };
        lock.lock_id = lock.compute_lock_id();
        lock
    }

    /// Canonical lock path `<root>/.emath/meaning.lock`.
    #[must_use]
    pub fn path(project_root: &Path) -> PathBuf {
        project_root.join(LOCK_DIR).join(LOCK_FILE_NAME)
    }

    /// Walks from `start` to a project root: a lock file or
    /// `emath-package.toml` wins, else the source-file parent (so a lock
    /// can live next to a lone genesis file). An empty `.emath/` directory
    /// is not a root — that would let a nested decoy shadow a parent lock.
    #[must_use]
    pub fn discover_project_root(start: &Path) -> PathBuf {
        let mut current = if start.is_file() {
            start
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| start.to_path_buf())
        } else if start.as_os_str().is_empty() {
            PathBuf::from(".")
        } else {
            start.to_path_buf()
        };
        let fallback = current.clone();
        loop {
            if Self::path(&current).is_file() || current.join("emath-package.toml").is_file() {
                return current;
            }
            match current.parent() {
                Some(parent) => current = parent.to_path_buf(),
                None => return fallback,
            }
        }
    }

    /// Loads a lock if the file exists. Missing is `Ok(None)`.
    /// Malformed / unknown version / tamper refuse.
    pub fn load(project_root: &Path) -> Result<Option<Self>, LockError> {
        let path = Self::path(project_root);
        match fs::read_to_string(&path) {
            Ok(body) => Ok(Some(Self::parse(&body)?)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(LockError::Malformed {
                detail: format!("cannot read {}: {error}", path.display()),
            }),
        }
    }

    /// Writes the canonical document. Creates `.emath/` as needed.
    pub fn save(&self, project_root: &Path) -> Result<(), LockError> {
        let dir = project_root.join(LOCK_DIR);
        fs::create_dir_all(&dir).map_err(|error| LockError::Malformed {
            detail: format!("cannot create {}: {error}", dir.display()),
        })?;
        let path = Self::path(project_root);
        fs::write(&path, self.encode()).map_err(|error| LockError::Malformed {
            detail: format!("cannot write {}: {error}", path.display()),
        })
    }

    /// Removes the lock file. Missing is success.
    pub fn remove_file(project_root: &Path) -> Result<(), LockError> {
        let path = Self::path(project_root);
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(LockError::Malformed {
                detail: format!("cannot remove {}: {error}", path.display()),
            }),
        }
    }

    /// Strict parse. Unknown fields, truncated JSON, and extra data refuse.
    pub fn parse(text: &str) -> Result<Self, LockError> {
        let value = parse_json(text).map_err(|detail| LockError::Malformed { detail })?;
        let object = value.as_object().ok_or_else(|| LockError::Malformed {
            detail: "root must be an object".to_string(),
        })?;
        refuse_unknown_keys(
            object,
            &[
                "schema",
                "schema_version",
                "portfolio_cap",
                "lock_id",
                "entries",
            ],
        )?;
        let schema = required_str(object, "schema")?;
        if schema != LOCK_SCHEMA {
            return Err(LockError::Malformed {
                detail: format!("schema must be {LOCK_SCHEMA}"),
            });
        }
        let version = required_u32(object, "schema_version")?;
        if version != LOCK_SCHEMA_VERSION {
            return Err(LockError::UnknownVersion { version });
        }
        let portfolio_cap = required_u32(object, "portfolio_cap")?;
        if portfolio_cap == 0 {
            return Err(LockError::Malformed {
                detail: "portfolio_cap must be >= 1".to_string(),
            });
        }
        let stored_id = parse_hex(required_str(object, "lock_id")?)?;
        let entries_value = object.get("entries").ok_or_else(|| LockError::Malformed {
            detail: "missing field entries".to_string(),
        })?;
        let raw_entries = entries_value
            .as_array()
            .ok_or_else(|| LockError::Malformed {
                detail: "entries must be an array".to_string(),
            })?;
        let mut entries = BTreeMap::new();
        for item in raw_entries {
            let row = item.as_object().ok_or_else(|| LockError::Malformed {
                detail: "lock entry must be an object".to_string(),
            })?;
            refuse_unknown_keys(
                row,
                &[
                    "declaration_id",
                    "hole_id",
                    "source",
                    "source_hash",
                    "world_fingerprint",
                    "portfolio_receipt_id",
                    "selection_method",
                    "selected_at",
                ],
            )?;
            let declaration_id = parse_hex(required_str(row, "declaration_id")?)?;
            let hole_id = required_str(row, "hole_id")?.to_string();
            if hole_id.is_empty() {
                return Err(LockError::Malformed {
                    detail: "hole_id must be non-empty".to_string(),
                });
            }
            let method = required_str(row, "selection_method")?;
            let selection_method =
                SelectionMethod::parse(method).ok_or_else(|| LockError::Malformed {
                    detail: format!("unknown selection_method `{method}`"),
                })?;
            let key = LockKey {
                declaration_id,
                hole_id,
            };
            let entry = LockEntry {
                source: required_str(row, "source")?.to_string(),
                source_hash: parse_hex(required_str(row, "source_hash")?)?,
                world_fingerprint: parse_hex(required_str(row, "world_fingerprint")?)?,
                portfolio_receipt_id: parse_hex(required_str(row, "portfolio_receipt_id")?)?,
                selection_method,
                selected_at: parse_decimal(required_str(row, "selected_at")?)?,
            };
            if entries.insert(key, entry).is_some() {
                return Err(LockError::Malformed {
                    detail: "duplicate lock key".to_string(),
                });
            }
        }
        let lock = Self {
            portfolio_cap,
            entries,
            lock_id: stored_id,
        };
        let computed = lock.compute_lock_id();
        if computed != stored_id {
            return Err(LockError::Tampered { fingerprint: None });
        }
        Ok(lock)
    }

    /// Deterministic JSON (BTreeMap order, two-space indent, trailing newline).
    #[must_use]
    pub fn encode(&self) -> String {
        let mut entries = String::new();
        for (index, (key, entry)) in self.entries.iter().enumerate() {
            if index > 0 {
                entries.push_str(",\n");
            }
            entries.push_str("    {\n");
            entries.push_str(&format!(
                "      {}: {},\n",
                quote("declaration_id"),
                quote(&hex(key.declaration_id))
            ));
            entries.push_str(&format!(
                "      {}: {},\n",
                quote("hole_id"),
                quote(&key.hole_id)
            ));
            entries.push_str(&format!(
                "      {}: {},\n",
                quote("source"),
                quote(&entry.source)
            ));
            entries.push_str(&format!(
                "      {}: {},\n",
                quote("source_hash"),
                quote(&hex(entry.source_hash))
            ));
            entries.push_str(&format!(
                "      {}: {},\n",
                quote("world_fingerprint"),
                quote(&hex(entry.world_fingerprint))
            ));
            entries.push_str(&format!(
                "      {}: {},\n",
                quote("portfolio_receipt_id"),
                quote(&hex(entry.portfolio_receipt_id))
            ));
            entries.push_str(&format!(
                "      {}: {},\n",
                quote("selection_method"),
                quote(entry.selection_method.as_str())
            ));
            entries.push_str(&format!(
                "      {}: {}\n",
                quote("selected_at"),
                quote(&entry.selected_at.to_string())
            ));
            entries.push_str("    }");
        }
        let entries_body = if self.entries.is_empty() {
            "[]".to_string()
        } else {
            format!("[\n{entries}\n  ]")
        };
        format!(
            "{{\n  {}: {},\n  {}: {},\n  {}: {},\n  {}: {},\n  {}: {}\n}}\n",
            quote("schema"),
            quote(LOCK_SCHEMA),
            quote("schema_version"),
            LOCK_SCHEMA_VERSION,
            quote("portfolio_cap"),
            self.portfolio_cap,
            quote("lock_id"),
            quote(&hex(self.lock_id)),
            quote("entries"),
            entries_body
        )
    }

    /// Identity over schema, version, cap, and entries excluding `selected_at`.
    #[must_use]
    pub fn compute_lock_id(&self) -> u64 {
        fnv1a64(self.identity_body().as_bytes())
    }

    fn identity_body(&self) -> String {
        let mut body = format!(
            "{LOCK_SCHEMA}:{LOCK_SCHEMA_VERSION}:cap={}:",
            self.portfolio_cap
        );
        for (key, entry) in &self.entries {
            body.push_str(&format!(
                "{}|{}|{}|{:016x}|{:016x}|{:016x}|{};",
                hex(key.declaration_id),
                key.hole_id,
                entry.source,
                entry.source_hash,
                entry.world_fingerprint,
                entry.portfolio_receipt_id,
                entry.selection_method.as_str()
            ));
        }
        body
    }

    /// Inserts or replaces an entry and refreshes `lock_id`.
    pub fn upsert(&mut self, key: LockKey, entry: LockEntry) {
        self.entries.insert(key, entry);
        self.lock_id = self.compute_lock_id();
    }

    /// Removes one key. Returns whether it was present.
    pub fn unset(&mut self, key: &LockKey) -> bool {
        let removed = self.entries.remove(key).is_some();
        self.lock_id = self.compute_lock_id();
        removed
    }

    /// Resolves a lock for this run. A miss with a same-source witness is drift.
    pub fn resolve(
        &self,
        declaration_id: u64,
        hole_id: &str,
        source: &str,
    ) -> Result<Option<&LockEntry>, LockError> {
        let key = LockKey {
            declaration_id,
            hole_id: hole_id.to_string(),
        };
        if let Some(entry) = self.entries.get(&key) {
            return Ok(Some(entry));
        }
        for (existing, entry) in &self.entries {
            if existing.hole_id == hole_id && entry.source == source {
                return Err(LockError::Drifted {
                    fingerprint: entry.world_fingerprint,
                    detail: format!(
                        "source `{source}` drifted: locked declaration {} current {}",
                        hex(existing.declaration_id),
                        hex(declaration_id)
                    ),
                });
            }
        }
        Ok(None)
    }
}

/// Refuses `set` when `fingerprint` is on the disqualification ledger or
/// absent from the receipt entirely.
pub fn refuse_disqualified(fingerprint: u64, receipt: &PortfolioReceipt) -> Result<(), LockError> {
    if let Some(entry) = receipt
        .ledger
        .iter()
        .find(|entry| entry.fingerprint == fingerprint)
    {
        match &entry.reason {
            crate::portfolio::DisqualificationReason::Dominated { .. } => {}
            crate::portfolio::DisqualificationReason::FailedGuard { .. }
            | crate::portfolio::DisqualificationReason::Refused { .. } => {
                return Err(LockError::Disqualified {
                    fingerprint,
                    ledger: entry.clone(),
                });
            }
        }
    }
    let known = receipt
        .input
        .candidates
        .iter()
        .any(|candidate| candidate.world_fingerprint == fingerprint);
    if !known {
        return Err(LockError::UnknownCandidate { fingerprint });
    }
    Ok(())
}

/// Commits to `locked` before ranking. The receipt policy is user-locked
/// and authority is copied from the candidate (never raised).
pub fn commit_locked_world(
    locked: WorldCandidate,
    axes: Vec<MetricAxis>,
    lock_id: u64,
    origin_receipt_id: u64,
    method: &SelectionMethod,
) -> Result<PortfolioReceipt, LockError> {
    if locked.labeled_authority > locked.evidence_authority {
        return Err(LockError::Drifted {
            fingerprint: locked.world_fingerprint,
            detail: "locked world would escalate authority".to_string(),
        });
    }
    if locked.guard_failure.is_some() {
        return Err(LockError::Drifted {
            fingerprint: locked.world_fingerprint,
            detail: "locked world is no longer admissible".to_string(),
        });
    }
    let policy = InterpretationPolicy::UserLocked {
        lock_id,
        origin_receipt_id,
        method: method.as_str().to_string(),
    };
    match evaluate(vec![locked.clone()], axes, policy) {
        Ok(receipt) => Ok(receipt),
        Err(PortfolioError::NoViableCandidate | PortfolioError::AmbiguousSingleBest { .. }) => {
            Err(LockError::Drifted {
                fingerprint: locked.world_fingerprint,
                detail: "locked world is no longer admissible".to_string(),
            })
        }
        Err(PortfolioError::AuthorityEscalation { .. }) => Err(LockError::Drifted {
            fingerprint: locked.world_fingerprint,
            detail: "locked world would escalate authority".to_string(),
        }),
        Err(PortfolioError::DuplicateFingerprint { fingerprint }) => Err(LockError::Malformed {
            detail: format!("duplicate fingerprint {fingerprint:016x}"),
        }),
    }
}

/// Caps a ranked candidate list. `cap == 0` is a caller error; genesis
/// maps it to `E-GEN-093`.
#[must_use]
pub fn apply_portfolio_cap<T: Clone>(candidates: &[T], cap: u32) -> Vec<T> {
    let limit = usize::try_from(cap).unwrap_or(usize::MAX);
    candidates.iter().take(limit).cloned().collect()
}
