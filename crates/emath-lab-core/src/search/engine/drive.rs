//! Index persistence: build/open/search drives, atomic swaps.

use super::*;

/// Marker written after a successful install; unmarked non-empty directories
/// are never wiped.
pub(super) const INDEX_MARKER: &str = "emath-search.index";

/// Build the new tree in a sibling directory; the live index and searcher
/// stay put until the staging tree is marked and swapped in.
pub(super) fn build_drive(worker: &mut Worker, docs: &[ArtifactDoc]) -> Result<IndexStats, SearchError> {
    if docs.is_empty() {
        return Err(SearchError::InvalidArgument {
            field: "docs",
            reason: "corpus must be non-empty (engine refuses zero-document builds)".into(),
        });
    }
    recover_leftovers(&worker.path)?;
    ensure_index_dir_usable(&worker.path)?;
    let staging = staging_dir(&worker.path);
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging).map_err(|error| SearchError::Query {
        reason: format!("create staging {}: {error}", staging.display()),
    })?;

    let mut documents = Vec::with_capacity(docs.len());
    for doc in docs {
        let id = match doc.fs_doc_id() {
            Ok(id) => id,
            Err(error) => {
                // Staging was created above; `?` must not leave it behind for
                // recover_leftovers on a later build.
                let _ = std::fs::remove_dir_all(&staging);
                return Err(error);
            }
        };
        documents.push(IndexableDocument {
            id,
            content: doc.text.clone(),
            title: None,
            metadata: HashMap::default(),
        });
    }

    let builder = IndexBuilder::new(&staging)
        .with_embedder_stack(control_stack())
        .with_config(index_config())
        .add_documents(documents);
    let stats = match worker.runtime.block_on(builder.build(&worker.cx)) {
        Ok(stats) => stats,
        Err(error) => {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(map_fs_error(error));
        }
    };

    if stats.error_count > 0 {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(SearchError::Build {
            report: format!(
                "{} of {} documents failed to embed: {}",
                stats.error_count,
                stats.source_count,
                stats
                    .errors
                    .iter()
                    .map(|(id, err)| format!("{id}: {err}"))
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
        });
    }

    if let Err(error) = write_marker(&staging) {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(error);
    }
    if let Err(error) = install_ready_index(&worker.path, &staging) {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(error);
    }
    if let Err(error) = open_drive(worker) {
        worker.searcher = None;
        return Err(error);
    }

    let lexical = stats.lexical.as_ref().map(|receipt| LexicalArmStats {
        backend: receipt.backend,
        path: receipt.path.display().to_string(),
        attempted: receipt.attempted,
        indexed: receipt.indexed,
        errors: receipt.errors.clone(),
    });
    Ok(IndexStats {
        source_count: stats.source_count,
        doc_count: stats.doc_count,
        error_count: stats.error_count,
        quality_indexed: stats.quality_indexed,
        lexical,
    })
}

/// Open the index at the worker's directory into fresh searcher state.
pub(super) fn open_drive(worker: &mut Worker) -> Result<(), SearchError> {
    let vectors = Arc::new(
        TwoTierIndex::open(&worker.path, index_config())
            .map_err(|error| map_open(worker, &error))?,
    );
    let fast = Arc::new(HashEmbedder::default_256());
    let mut searcher = TwoTierSearcher::new(vectors.clone(), fast.clone(), index_config())
        .with_nqc_dense_downweight_disabled();

    // Lexical arm: attach the Quill BM25 reader when the build wrote one.
    // (open_hybrid cannot be used here: it refuses hash-identity generations
    // by design; we mirror its documented manual shape — TwoTierSearcher::new
    // + with_lexical — instead.)
    let lexical_dir = worker.path.join("lexical");
    if lexical_dir.is_dir() {
        let index = worker
            .runtime
            .block_on(RootBoundQuillSearchIndex::open(
                &worker.cx,
                &lexical_dir,
                QuillConfig::default(),
            ))
            .map_err(|error| SearchError::Open {
                path: lexical_dir.display().to_string(),
                reason: format!("quill reader: {error}"),
            })?;
        let read: Arc<dyn LexicalRead> = Arc::new(index);
        searcher = searcher.with_lexical(read);
    }

    worker.searcher = Some(searcher);
    Ok(())
}

/// Run a query against the open searcher. `NotReady` when no index is open.
pub(super) fn search_drive(worker: &Worker, query: &str, k: usize) -> Result<Vec<Hit>, SearchError> {
    let searcher = worker
        .searcher
        .as_ref()
        .ok_or_else(|| SearchError::NotReady {
            reason: "no index is built or open at this directory".into(),
        })?;
    let (results, _metrics) = worker
        .runtime
        .block_on(searcher.search_collect(&worker.cx, query, k))
        .map_err(map_fs_error)?;
    Ok(results.iter().map(map_hit).collect())
}

pub(super) fn sibling(path: &Path, suffix: &str) -> PathBuf {
    match path.file_name() {
        Some(name) => {
            let mut file = name.to_os_string();
            file.push(suffix);
            match path.parent() {
                Some(parent) if !parent.as_os_str().is_empty() => parent.join(file),
                _ => PathBuf::from(file),
            }
        }
        None => {
            let mut fallback = path.as_os_str().to_os_string();
            fallback.push(suffix);
            PathBuf::from(fallback)
        }
    }
}

pub(super) fn staging_dir(path: &Path) -> PathBuf {
    sibling(path, ".emath-search-staging")
}

pub(super) fn backup_dir(path: &Path) -> PathBuf {
    sibling(path, ".emath-search-backup")
}

pub(super) fn recover_leftovers(dest: &Path) -> Result<(), SearchError> {
    let _ = std::fs::remove_dir_all(staging_dir(dest));
    recover_stranded_backup(dest)
}

pub(super) fn recover_stranded_backup(dest: &Path) -> Result<(), SearchError> {
    let backup = backup_dir(dest);
    if !backup.exists() {
        return Ok(());
    }
    if dest.join(INDEX_MARKER).is_file() {
        let _ = std::fs::remove_dir_all(&backup);
        return Ok(());
    }
    if dest_missing_or_empty(dest)? {
        if dest.exists() {
            std::fs::remove_dir_all(dest).map_err(|error| SearchError::Query {
                reason: format!(
                    "clear empty dest {} before restoring backup: {error}",
                    dest.display()
                ),
            })?;
        }
        return std::fs::rename(&backup, dest).map_err(|error| SearchError::Query {
            reason: format!(
                "restore stranded index {} -> {}: {error}",
                backup.display(),
                dest.display()
            ),
        });
    }
    Ok(())
}

pub(super) fn dest_missing_or_empty(path: &Path) -> Result<bool, SearchError> {
    match std::fs::read_dir(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(error) => Err(SearchError::Query {
            reason: format!("read dest {}: {error}", path.display()),
        }),
        Ok(entries) => Ok(entries.count() == 0),
    }
}

pub(super) fn install_ready_index(dest: &Path, staging: &Path) -> Result<(), SearchError> {
    let backup = backup_dir(dest);
    if dest.join(INDEX_MARKER).is_file() {
        std::fs::rename(dest, &backup).map_err(|error| SearchError::Query {
            reason: format!(
                "park live index {} -> {}: {error}",
                dest.display(),
                backup.display()
            ),
        })?;
        if let Err(error) = std::fs::rename(staging, dest) {
            if backup.exists() {
                let _ = std::fs::remove_dir_all(dest);
                let _ = std::fs::rename(&backup, dest);
            }
            return Err(SearchError::Query {
                reason: format!(
                    "install staging {} -> {}: {error}",
                    staging.display(),
                    dest.display()
                ),
            });
        }
        let _ = std::fs::remove_dir_all(&backup);
        return Ok(());
    }
    if dest.exists() {
        std::fs::remove_dir_all(dest).map_err(|error| SearchError::Query {
            reason: format!("clear empty dest {}: {error}", dest.display()),
        })?;
    }
    std::fs::rename(staging, dest).map_err(|error| SearchError::Query {
        reason: format!(
            "install staging {} -> {}: {error}",
            staging.display(),
            dest.display()
        ),
    })
}

pub(super) fn index_dir_is_usable(path: &Path) -> Result<bool, SearchError> {
    match std::fs::read_dir(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(error) => Err(SearchError::Query {
            reason: format!("read dir {}: {error}", path.display()),
        }),
        Ok(entries) => {
            let mut empty = true;
            let mut marked = false;
            for entry in entries {
                let entry = entry.map_err(|error| SearchError::Query {
                    reason: format!("read dir entry {}: {error}", path.display()),
                })?;
                empty = false;
                if entry.file_name() == INDEX_MARKER {
                    marked = true;
                }
            }
            Ok(empty || marked)
        }
    }
}

pub(super) fn ensure_index_dir_usable(path: &Path) -> Result<(), SearchError> {
    if index_dir_is_usable(path)? {
        Ok(())
    } else {
        Err(SearchError::InvalidArgument {
            field: "path",
            reason: format!(
                "{} is not an emath-search index (missing {INDEX_MARKER}); refusing to overwrite",
                path.display()
            ),
        })
    }
}

pub(super) fn write_marker(path: &Path) -> Result<(), SearchError> {
    std::fs::write(path.join(INDEX_MARKER), b"emath-search\n").map_err(|error| SearchError::Query {
        reason: format!("write marker {}: {error}", path.display()),
    })
}

pub(super) fn clear_dir(path: &Path) -> Result<(), SearchError> {
    match std::fs::read_dir(path) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry.map_err(|error| SearchError::Query {
                    reason: format!("read dir entry {}: {error}", path.display()),
                })?;
                let target = entry.path();
                if target.is_dir() {
                    std::fs::remove_dir_all(&target).map_err(|error| SearchError::Query {
                        reason: format!("remove dir {}: {error}", target.display()),
                    })?;
                } else {
                    std::fs::remove_file(&target).map_err(|error| SearchError::Query {
                        reason: format!("remove file {}: {error}", target.display()),
                    })?;
                }
            }
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(SearchError::Query {
            reason: format!("read dir {}: {error}", path.display()),
        }),
    }
}

pub(super) fn remove_owned_index(path: &Path) -> Result<(), SearchError> {
    ensure_index_dir_usable(path)?;
    clear_dir(path)
}

pub(super) fn map_hit(result: &ScoredResult) -> Hit {
    let (kind, id) = from_fs_doc_id(&result.doc_id)
        .unwrap_or_else(|| (String::new(), result.doc_id.to_string()));
    Hit {
        doc_id: result.doc_id.to_string(),
        kind,
        id,
        score: result.score,
        source: match result.source {
            ScoreSource::Lexical => HitSource::Lexical,
            ScoreSource::SemanticFast | ScoreSource::SemanticQuality => HitSource::Semantic,
            ScoreSource::Hybrid => HitSource::Hybrid,
            ScoreSource::Reranked => HitSource::Reranked,
            ScoreSource::HashControl => HitSource::HashControl,
        },
    }
}

pub(super) fn map_open(worker: &Worker, error: &FsError) -> SearchError {
    SearchError::Open {
        path: worker.path.display().to_string(),
        reason: format!("{error}"),
    }
}

/// Map engine errors onto the crate error model.
pub(super) fn map_fs_error(error: FsError) -> SearchError {
    match error {
        FsError::InvalidConfig {
            field,
            value,
            reason,
        } => SearchError::InvalidArgument {
            field: "index_config",
            reason: format!("{field} = {value}: {reason}"),
        },
        FsError::EmbedderUnavailable { model, reason } => SearchError::Query {
            reason: format!("embedder unavailable for {model}: {reason}"),
        },
        FsError::IndexCorrupted { path, detail } => SearchError::Open {
            path: path.display().to_string(),
            reason: format!("index corrupted: {detail}"),
        },
        other => SearchError::Query {
            reason: format!("{other}"),
        },
    }
}

#[cfg(all(test, feature = "search"))]
