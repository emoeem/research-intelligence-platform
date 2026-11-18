//! WSW Body Artifact bridge.
//!
//! This module connects the WSW CAS body store to the existing database CAS row
//! and the platform's existing Artifact/Evidence registrar. It does not create a
//! second Artifact, Evidence, or ToolRun store.

use std::error::Error;
use std::fmt;
use std::path::PathBuf;

use database::{DbError, Pool};
use domain::{ArtifactId, TaskId, ToolRunId, TraceId};

use crate::cas::{sha256_hex, store_body, BodyRef, CasError};
use crate::ids::ExchangeId;
use crate::store::{register_body_blob, BodyBlobRow};

/// Request or response body direction within an HTTP exchange.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyDirection {
    /// Captured request body.
    Request,
    /// Captured response body.
    Response,
}

impl BodyDirection {
    /// Stable lowercase direction token for idempotency keys and metadata.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Request => "request",
            Self::Response => "response",
        }
    }
}

/// Registration request passed to the existing platform Artifact/Evidence owner.
#[derive(Debug, Clone)]
pub struct BodyArtifactRegistration {
    /// Existing ToolRun identifier; no new run record is created here.
    pub tool_run_id: ToolRunId,
    /// Existing Exchange that first observed this body, if available.
    pub exchange_id: Option<ExchangeId>,
    /// Optional existing task correlation.
    pub task_id: Option<TaskId>,
    /// Optional existing trace correlation.
    pub trace_id: Option<TraceId>,
    /// Request or response body.
    pub direction: BodyDirection,
    /// CAS pointer to the full original body bytes.
    pub body_ref: BodyRef,
    /// Media type recorded on the existing Artifact row.
    pub mime_type: String,
    /// Stable idempotency key for the tuple (tool_run_id, direction, body_sha256).
    ///
    /// Implementations must return the same Artifact for retries with this key.
    pub idempotency_key: String,
}

/// Result returned by the existing Artifact registrar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisteredArtifact {
    /// Existing platform Artifact ID.
    pub artifact_id: ArtifactId,
    /// Existing platform Artifact URI/reference.
    pub uri: String,
}

/// Final link joining a CAS body to the platform's existing ToolRun and Artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BodyArtifactLink {
    /// Existing ToolRun identifier.
    pub tool_run_id: ToolRunId,
    /// Existing Artifact identifier.
    pub artifact_id: ArtifactId,
    /// Existing Artifact URI/reference.
    pub artifact_uri: String,
    /// CAS reference to the full original body.
    pub body_ref: BodyRef,
}

/// Bridge implemented by the owner of the platform's existing Artifact/Evidence path.
///
/// Implementations should delegate to the current Artifact/Evidence registrar (for
/// example the existing EvidenceRegistrar adapter), not write a parallel table.
/// Registration must be idempotent by idempotency_key, attach the Artifact to the
/// supplied ToolRun, and preserve the same Artifact URI/hash on retry.
pub trait BodyArtifactRegistrar {
    /// Error returned by the platform Artifact/Evidence registrar.
    type Error: Error + Send + Sync + 'static;

    /// Register or resolve the existing Artifact for this CAS body.
    fn register_body_artifact(
        &self,
        registration: &BodyArtifactRegistration,
    ) -> Result<RegisteredArtifact, Self::Error>;
}

/// CAS + database + Artifact bridge errors.
#[derive(Debug)]
pub enum BodyArtifactAdapterError<E> {
    /// CAS write, path, or integrity error.
    Cas(CasError),
    /// CAS metadata row registration failed; the BodyRef is retained for retry.
    Database {
        /// Durable CAS reference retained for retry.
        body_ref: Box<BodyRef>,
        /// Existing database error.
        source: DbError,
    },
    /// Body size cannot be represented by the existing SQLite INTEGER column.
    BodySizeOverflow {
        /// Durable CAS reference retained for inspection/retry.
        body_ref: Box<BodyRef>,
    },
    /// Artifact registration failed after CAS and body-row persistence; the BodyRef
    /// is retained so the caller can retry without recapturing or rewriting bytes.
    Registrar {
        /// Durable CAS reference retained for retry.
        body_ref: Box<BodyRef>,
        /// Original registrar error.
        source: E,
    },
    /// MIME type was empty, malformed, or contained control bytes.
    InvalidMimeType(String),
    /// Registrar returned an empty or control-character-containing URI.
    InvalidArtifactUri {
        /// Durable CAS reference retained for retry/inspection.
        body_ref: Box<BodyRef>,
        /// Artifact ID returned by the registrar.
        artifact_id: ArtifactId,
        /// Invalid URI returned by the registrar.
        uri: String,
    },
}

impl<E: fmt::Display> fmt::Display for BodyArtifactAdapterError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cas(error) => write!(f, "store WSW body in CAS: {error}"),
            Self::Database { source, .. } => {
                write!(f, "register WSW body CAS row: {source}")
            }
            Self::BodySizeOverflow { .. } => {
                write!(f, "WSW body size exceeds SQLite INTEGER range")
            }
            Self::Registrar { source, .. } => {
                write!(f, "register WSW body Artifact: {source}")
            }
            Self::InvalidMimeType(value) => write!(f, "invalid WSW body MIME type: {value:?}"),
            Self::InvalidArtifactUri { uri, .. } => {
                write!(f, "Artifact registrar returned an invalid URI: {uri:?}")
            }
        }
    }
}

impl<E: Error + 'static> Error for BodyArtifactAdapterError<E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Cas(error) => Some(error),
            Self::Database { source, .. } => Some(source),
            Self::BodySizeOverflow { .. } => None,
            Self::Registrar { source, .. } => Some(source),
            Self::InvalidMimeType(_) | Self::InvalidArtifactUri { .. } => None,
        }
    }
}

/// Adapter that stores original body bytes in CAS, registers the wsw_body_blob row,
/// then delegates Artifact registration to the platform's existing owner.
#[derive(Clone)]
pub struct BodyArtifactAdapter {
    pool: Pool,
    artifacts_root: PathBuf,
    cap: u64,
}

impl BodyArtifactAdapter {
    /// Construct an adapter with the existing database pool, Artifact root, and
    /// fidelity cap. The pool is the shared platform database, not a new database.
    pub fn new(pool: Pool, artifacts_root: impl Into<PathBuf>, cap: u64) -> Self {
        Self {
            pool,
            artifacts_root: artifacts_root.into(),
            cap,
        }
    }

    /// Persist a body in CAS, register its CAS row, and link an Artifact to ToolRun.
    ///
    /// If database or Artifact registration fails, the returned error includes the
    /// durable BodyRef. A retry with the same ToolRun, direction, and bytes produces
    /// the same idempotency key and reuses the same CAS object.
    #[allow(clippy::too_many_arguments)]
    pub fn capture_and_register<R: BodyArtifactRegistrar>(
        &self,
        bytes: &[u8],
        tool_run_id: ToolRunId,
        exchange_id: Option<ExchangeId>,
        task_id: Option<TaskId>,
        trace_id: Option<TraceId>,
        direction: BodyDirection,
        mime_type: impl Into<String>,
        registrar: &R,
    ) -> Result<BodyArtifactLink, BodyArtifactAdapterError<R::Error>> {
        let mime_type = mime_type.into();
        if !valid_mime_type(&mime_type) {
            return Err(BodyArtifactAdapterError::InvalidMimeType(mime_type));
        }

        let body_ref = store_body(&self.artifacts_root, bytes, self.cap)
            .map_err(BodyArtifactAdapterError::Cas)?;
        let size_bytes = i64::try_from(body_ref.size).map_err(|_| {
            BodyArtifactAdapterError::BodySizeOverflow {
                body_ref: Box::new(body_ref.clone()),
            }
        })?;
        let row = BodyBlobRow {
            sha256: body_ref.sha256.clone(),
            uri: body_ref.uri.clone(),
            size_bytes,
            mime_type: mime_type.clone(),
            first_seen_exchange_id: exchange_id.as_ref().map(ToString::to_string),
            created_at: domain::time::now_iso(),
        };
        self.pool
            .with_conn(|conn| register_body_blob(conn, &row))
            .map_err(|source| BodyArtifactAdapterError::Database {
                body_ref: Box::new(body_ref.clone()),
                source,
            })?;

        let key_material = format!(
            "wsw-body-artifact-v1\0{}\0{}\0{}",
            tool_run_id.as_str(),
            direction.as_str(),
            body_ref.sha256
        );
        let registration = BodyArtifactRegistration {
            tool_run_id: tool_run_id.clone(),
            exchange_id,
            task_id,
            trace_id,
            direction,
            body_ref: body_ref.clone(),
            mime_type,
            idempotency_key: format!("wsw-body-v1:{}", sha256_hex(key_material.as_bytes())),
        };
        let artifact = registrar
            .register_body_artifact(&registration)
            .map_err(|source| BodyArtifactAdapterError::Registrar {
                body_ref: Box::new(body_ref.clone()),
                source,
            })?;
        if !valid_artifact_uri(&artifact.uri) {
            return Err(BodyArtifactAdapterError::InvalidArtifactUri {
                body_ref: Box::new(body_ref),
                artifact_id: artifact.artifact_id,
                uri: artifact.uri,
            });
        }

        Ok(BodyArtifactLink {
            tool_run_id,
            artifact_id: artifact.artifact_id,
            artifact_uri: artifact.uri,
            body_ref,
        })
    }
}

fn valid_mime_type(value: &str) -> bool {
    let Some((kind, subtype)) = value.trim().split_once('/') else {
        return false;
    };
    !kind.is_empty()
        && !subtype.is_empty()
        && !value.bytes().any(|byte| byte < 0x20 || byte == 0x7f)
}

fn valid_artifact_uri(value: &str) -> bool {
    !value.trim().is_empty() && !value.bytes().any(|byte| byte < 0x20 || byte == 0x7f)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeRegistrar {
        seen: Mutex<Vec<BodyArtifactRegistration>>,
        fail: bool,
        invalid_uri: bool,
    }

    impl BodyArtifactRegistrar for FakeRegistrar {
        type Error = std::io::Error;

        fn register_body_artifact(
            &self,
            registration: &BodyArtifactRegistration,
        ) -> Result<RegisteredArtifact, Self::Error> {
            self.seen.lock().unwrap().push(registration.clone());
            if self.fail {
                return Err(std::io::Error::other("artifact store unavailable"));
            }
            Ok(RegisteredArtifact {
                artifact_id: ArtifactId::try_new("art-wsw-fixture").unwrap(),
                uri: if self.invalid_uri {
                    "\n".into()
                } else {
                    format!(
                        "ripartifact://{}/{}",
                        registration.tool_run_id.as_str(),
                        registration.idempotency_key
                    )
                },
            })
        }
    }

    fn migrated_pool() -> Pool {
        use database::{migrate, DbPath, PoolConfig};
        use std::time::Duration;

        let pool = Pool::open(PoolConfig {
            path: DbPath::Memory,
            readonly: false,
            max_connections: 1,
            busy_timeout: Duration::from_secs(5),
        })
        .unwrap();
        let migrations = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
        migrate(&pool, &migrations).unwrap();
        pool
    }

    fn temp_root() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "wsw-artifact-adapter-{}-{}",
            std::process::id(),
            crate::ids::ExchangeId::generate()
        ));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn cas_body_is_linked_to_existing_tool_run_artifact_contract() {
        let root = temp_root();
        let pool = migrated_pool();
        let adapter = BodyArtifactAdapter::new(pool.clone(), &root, 4);
        let registrar = FakeRegistrar::default();
        let run = ToolRunId::try_new("rn-wsw-fixture").unwrap();
        let task = TaskId::try_new("task-wsw-fixture").unwrap();
        let trace = TraceId::try_new("trace-wsw-fixture").unwrap();
        let first_exchange = ExchangeId::generate();

        let first = adapter
            .capture_and_register(
                b"request body",
                run.clone(),
                Some(first_exchange.clone()),
                Some(task.clone()),
                Some(trace.clone()),
                BodyDirection::Request,
                "application/octet-stream",
                &registrar,
            )
            .unwrap();
        assert_eq!(first.tool_run_id, run);
        assert_eq!(first.artifact_id.as_str(), "art-wsw-fixture");
        assert!(first
            .artifact_uri
            .starts_with("ripartifact://rn-wsw-fixture/"));
        assert_eq!(
            first.body_ref.fidelity,
            crate::types::CaptureFidelity::Truncated
        );
        assert_eq!(
            crate::cas::read_body(&root, &first.body_ref.uri).unwrap(),
            b"request body"
        );

        let second = adapter
            .capture_and_register(
                b"request body",
                ToolRunId::try_new("rn-wsw-fixture").unwrap(),
                Some(ExchangeId::generate()),
                Some(task),
                Some(trace),
                BodyDirection::Request,
                "application/octet-stream",
                &registrar,
            )
            .unwrap();
        assert_eq!(first.body_ref, second.body_ref);
        let seen = registrar.seen.lock().unwrap();
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[0].idempotency_key, seen[1].idempotency_key);
        assert_eq!(seen[0].direction, BodyDirection::Request);
        assert!(seen[0].exchange_id.is_some());
        let blob_count = pool.with_conn(crate::store::blob_count).unwrap();
        assert_eq!(blob_count, 1, "CAS row is registered once for shared bytes");
        let stored_row = pool
            .with_conn(|conn| crate::store::get_body_blob(conn, &first.body_ref.sha256))
            .unwrap()
            .unwrap();
        let first_exchange_id = first_exchange.to_string();
        assert_eq!(
            stored_row.first_seen_exchange_id.as_deref(),
            Some(first_exchange_id.as_str()),
            "deduplicated registration preserves first-seen Exchange"
        );
        assert_eq!(
            seen[0].task_id.as_ref().unwrap().as_str(),
            "task-wsw-fixture"
        );
        assert_eq!(
            seen[0].trace_id.as_ref().unwrap().as_str(),
            "trace-wsw-fixture"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn registrar_failure_returns_durable_body_ref_for_retry() {
        let root = temp_root();
        let adapter = BodyArtifactAdapter::new(migrated_pool(), &root, 1024);
        let registrar = FakeRegistrar {
            fail: true,
            ..FakeRegistrar::default()
        };
        let result = adapter.capture_and_register(
            b"response",
            ToolRunId::try_new("rn-wsw-failure").unwrap(),
            Some(ExchangeId::generate()),
            None,
            None,
            BodyDirection::Response,
            "text/plain",
            &registrar,
        );
        match result {
            Err(BodyArtifactAdapterError::Registrar { body_ref, .. }) => {
                assert_eq!(
                    crate::cas::read_body(&root, &body_ref.uri).unwrap(),
                    b"response"
                );
            }
            other => panic!("expected registrar failure with CAS reference, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn invalid_mime_and_artifact_uri_fail_closed() {
        let root = temp_root();
        let pool = migrated_pool();
        let adapter = BodyArtifactAdapter::new(pool.clone(), &root, 1024);
        let registrar = FakeRegistrar::default();
        let invalid_mime = adapter.capture_and_register(
            b"body",
            ToolRunId::try_new("rn-wsw-invalid").unwrap(),
            Some(ExchangeId::generate()),
            None,
            None,
            BodyDirection::Request,
            "text/\nplain",
            &registrar,
        );
        assert!(matches!(
            invalid_mime,
            Err(BodyArtifactAdapterError::InvalidMimeType(_))
        ));
        assert!(
            !root.join("wsw").exists(),
            "invalid MIME must fail before CAS write"
        );

        let bad_uri_registrar = FakeRegistrar {
            invalid_uri: true,
            ..FakeRegistrar::default()
        };
        let invalid_uri = adapter.capture_and_register(
            b"body",
            ToolRunId::try_new("rn-wsw-invalid").unwrap(),
            Some(ExchangeId::generate()),
            None,
            None,
            BodyDirection::Request,
            "text/plain",
            &bad_uri_registrar,
        );
        assert!(matches!(
            invalid_uri,
            Err(BodyArtifactAdapterError::InvalidArtifactUri { .. })
        ));
        let _ = std::fs::remove_dir_all(root);
    }
}
