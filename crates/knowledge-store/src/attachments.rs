//! Reusable bounded object storage, independent of HTTP/page content.
use bytes::{Bytes, BytesMut};
use futures_util::StreamExt;
use object_store::{
    GetOptions, GetRange, MultipartUpload, ObjectStore as Backend, ObjectStoreExt, PutPayload,
    PutResult, UploadPart, WriteMultipart,
    multipart::{MultipartStore, PartId},
    path::Path,
};
use sqlx::{PgPool, Postgres, pool::PoolConnection};
use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
};
use uuid::Uuid;

pub type StorageFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, StorageError>> + Send + 'a>>;
#[derive(Debug, thiserror::Error)]
#[error("object storage operation failed")]
pub struct StorageError;
pub trait ObjectUpload: Send {
    fn upload_id(&self) -> &str;
    fn write(&mut self, bytes: Bytes) -> StorageFuture<'_, ()>;
    fn finish(self: Box<Self>) -> StorageFuture<'static, ()>;
    fn abort(self: Box<Self>) -> StorageFuture<'static, ()>;
}
pub trait ObjectStore: Send + Sync {
    fn start(&self, key: Uuid) -> StorageFuture<'_, Box<dyn ObjectUpload>>;
    fn read_range(&self, key: Uuid, range: std::ops::Range<u64>) -> StorageFuture<'_, Bytes>;
    fn size(&self, key: Uuid) -> StorageFuture<'_, u64>;
    fn delete(&self, key: Uuid) -> StorageFuture<'_, ()>;
    fn abort_upload<'a>(&'a self, key: Uuid, id: &'a str) -> StorageFuture<'a, ()>;
}

trait StorageBackend: Backend + MultipartStore {}
impl<T: Backend + MultipartStore> StorageBackend for T {}
pub struct S3ObjectStore {
    backend: Arc<dyn StorageBackend>,
}
impl S3ObjectStore {
    /// S3-compatible, path-style addressing. No provider is selected here.
    pub fn new(
        endpoint: &str,
        bucket: &str,
        region: &str,
        access_key: &str,
        secret_key: &str,
        allow_http: bool,
    ) -> Result<Self, StorageError> {
        let backend = object_store::aws::AmazonS3Builder::new()
            .with_endpoint(endpoint)
            .with_bucket_name(bucket)
            .with_region(region)
            .with_access_key_id(access_key)
            .with_secret_access_key(secret_key)
            .with_allow_http(allow_http)
            .with_virtual_hosted_style_request(false)
            .build()
            .map_err(|_| StorageError)?;
        Ok(Self {
            backend: Arc::new(backend),
        })
    }
}
/// Explicitly injected test implementation; never a runtime fallback.
pub struct MemoryObjectStore {
    backend: Arc<dyn StorageBackend>,
}
impl Default for MemoryObjectStore {
    fn default() -> Self {
        Self {
            backend: Arc::new(object_store::memory::InMemory::new()),
        }
    }
}
struct MultipartWriter {
    writer: WriteMultipart,
    id: String,
}
struct DurableMultipart {
    backend: Arc<dyn StorageBackend>,
    path: Path,
    id: String,
    parts: Arc<Mutex<Vec<Option<PartId>>>>,
}
impl std::fmt::Debug for DurableMultipart {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DurableMultipart([redacted])")
    }
}
#[async_trait::async_trait]
impl MultipartUpload for DurableMultipart {
    fn put_part(&mut self, data: PutPayload) -> UploadPart {
        let index = {
            let mut parts = self.parts.lock().expect("part lock");
            let index = parts.len();
            parts.push(None);
            index
        };
        let backend = self.backend.clone();
        let path = self.path.clone();
        let id = self.id.clone();
        let parts = self.parts.clone();
        Box::pin(async move {
            let part = backend.put_part(&path, &id, index, data).await?;
            parts.lock().expect("part lock")[index] = Some(part);
            Ok(())
        })
    }
    async fn complete(&mut self) -> object_store::Result<PutResult> {
        let parts = self
            .parts
            .lock()
            .expect("part lock")
            .iter()
            .cloned()
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| object_store::Error::Generic {
                store: "encrypted object adapter",
                source: "multipart incomplete".into(),
            })?;
        self.backend
            .complete_multipart(&self.path, &self.id, parts)
            .await
    }
    async fn abort(&mut self) -> object_store::Result<()> {
        self.backend.abort_multipart(&self.path, &self.id).await
    }
}
impl ObjectUpload for MultipartWriter {
    fn upload_id(&self) -> &str {
        &self.id
    }
    fn write(&mut self, bytes: Bytes) -> StorageFuture<'_, ()> {
        Box::pin(async move {
            // WriteMultipart's default parts are 5 MiB. Its write does NOT provide
            // backpressure: wait before each bounded (<=64 KiB+28) crypto frame.
            if bytes.len() > 128 * 1024 {
                return Err(StorageError);
            }
            self.writer
                .wait_for_capacity(2)
                .await
                .map_err(|_| StorageError)?;
            self.writer.put(bytes);
            Ok(())
        })
    }
    fn finish(self: Box<Self>) -> StorageFuture<'static, ()> {
        Box::pin(async move {
            self.writer
                .finish()
                .await
                .map(|_| ())
                .map_err(|_| StorageError)
        })
    }
    fn abort(self: Box<Self>) -> StorageFuture<'static, ()> {
        Box::pin(async move { self.writer.abort().await.map_err(|_| StorageError) })
    }
}
macro_rules! adapter {
    ($ty:ty,$memory:expr) => {
        impl ObjectStore for $ty {
            fn start(&self, key: Uuid) -> StorageFuture<'_, Box<dyn ObjectUpload>> {
                Box::pin(async move {
                    let path = Path::from(key.to_string());
                    let id = self
                        .backend
                        .create_multipart(&path)
                        .await
                        .map_err(|_| StorageError)?;
                    let upload = DurableMultipart {
                        backend: self.backend.clone(),
                        path,
                        id: id.clone(),
                        parts: Arc::new(Mutex::new(Vec::new())),
                    };
                    Ok(Box::new(MultipartWriter {
                        writer: WriteMultipart::new(Box::new(upload)),
                        id,
                    }) as Box<dyn ObjectUpload>)
                })
            }
            fn read_range(
                &self,
                key: Uuid,
                range: std::ops::Range<u64>,
            ) -> StorageFuture<'_, Bytes> {
                Box::pin(async move {
                    if range.end < range.start || range.end - range.start > 128 * 1024 {
                        return Err(StorageError);
                    }
                    let result = self
                        .backend
                        .get_opts(
                            &Path::from(key.to_string()),
                            GetOptions {
                                range: Some(GetRange::Bounded(range.clone())),
                                ..Default::default()
                            },
                        )
                        .await
                        .map_err(|_| StorageError)?;
                    if result.range != range {
                        return Err(StorageError);
                    }
                    let expected = (range.end - range.start) as usize;
                    let mut output = BytesMut::with_capacity(expected);
                    let mut stream = result.into_stream();
                    while let Some(part) = stream.next().await {
                        let part = part.map_err(|_| StorageError)?;
                        if part.len() > expected - output.len() {
                            return Err(StorageError);
                        }
                        output.extend_from_slice(&part);
                    }
                    if output.len() != expected {
                        return Err(StorageError);
                    }
                    Ok(output.freeze())
                })
            }
            fn size(&self, key: Uuid) -> StorageFuture<'_, u64> {
                Box::pin(async move {
                    self.backend
                        .head(&Path::from(key.to_string()))
                        .await
                        .map(|meta| meta.size)
                        .map_err(|_| StorageError)
                })
            }
            fn delete(&self, key: Uuid) -> StorageFuture<'_, ()> {
                Box::pin(async move {
                    match self.backend.delete(&Path::from(key.to_string())).await {
                        Ok(()) | Err(object_store::Error::NotFound { .. }) => Ok(()),
                        Err(_) => Err(StorageError),
                    }
                })
            }
            fn abort_upload<'a>(&'a self, key: Uuid, id: &'a str) -> StorageFuture<'a, ()> {
                Box::pin(async move {
                    match self
                        .backend
                        .abort_multipart(&Path::from(key.to_string()), &id.to_owned())
                        .await
                    {
                        Ok(()) | Err(object_store::Error::NotFound { .. }) => Ok(()),
                        Err(_) if $memory => Ok(()),
                        Err(_) => Err(StorageError),
                    }
                })
            }
        }
    };
}
adapter!(S3ObjectStore, false);
// InMemory abort's only failure is a missing multipart ID, wrapped as Generic.
adapter!(MemoryObjectStore, true);

pub fn object_lock_key(key: Uuid) -> i64 {
    i64::from_be_bytes(key.as_bytes()[8..].try_into().expect("UUID has 16 bytes"))
}
pub async fn lock_object(
    pool: &PgPool,
    key: Uuid,
) -> Result<PoolConnection<Postgres>, sqlx::Error> {
    let mut connection = pool.acquire().await?;
    // Set before the first lock await. Cancellation must CLOSE, not return a
    // session holding pg_advisory_lock to the pool.
    connection.close_on_drop();
    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(object_lock_key(key))
        .execute(&mut *connection)
        .await?;
    Ok(connection)
}
/// Idempotent orphan cleanup. Does not purge soft-deleted live attachments.
pub async fn cleanup_orphans(
    pool: &PgPool,
    objects: &dyn ObjectStore,
) -> Result<u64, StorageError> {
    let keys:Vec<Uuid>=sqlx::query_scalar("SELECT object_key FROM attachment_staging WHERE expires_at < now() ORDER BY expires_at LIMIT 100").fetch_all(pool).await.map_err(|_|StorageError)?;
    let mut removed = 0;
    for key in keys {
        let mut connection = pool.acquire().await.map_err(|_| StorageError)?;
        connection.close_on_drop();
        let locked: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
            .bind(object_lock_key(key))
            .fetch_one(&mut *connection)
            .await
            .map_err(|_| StorageError)?;
        if !locked {
            continue;
        }
        let eligible:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM attachment_staging s WHERE object_key=$1 AND expires_at < now() AND NOT EXISTS(SELECT 1 FROM attachments a WHERE a.object_key=s.object_key))").bind(key).fetch_one(&mut *connection).await.map_err(|_|StorageError)?;
        if !eligible {
            continue;
        }
        let remote: Option<String> = sqlx::query_scalar(
            "SELECT remote_upload_id FROM attachment_staging WHERE object_key=$1",
        )
        .bind(key)
        .fetch_one(&mut *connection)
        .await
        .map_err(|_| StorageError)?;
        if let Some(id) = remote {
            tokio::time::timeout(
                std::time::Duration::from_secs(30),
                objects.abort_upload(key, &id),
            )
            .await
            .map_err(|_| StorageError)??;
        }
        tokio::time::timeout(std::time::Duration::from_secs(30), objects.delete(key))
            .await
            .map_err(|_| StorageError)??;
        sqlx::query(
            "DELETE FROM attachment_upload_requests WHERE object_key=$1 AND attachment_id IS NULL",
        )
        .bind(key)
        .execute(&mut *connection)
        .await
        .map_err(|_| StorageError)?;
        sqlx::query("DELETE FROM attachment_staging WHERE object_key=$1")
            .bind(key)
            .execute(&mut *connection)
            .await
            .map_err(|_| StorageError)?;
        removed += 1;
    }
    Ok(removed)
}
