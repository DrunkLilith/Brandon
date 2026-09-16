//! At-least-once leasing. The monotonically increasing attempt is the fencing
//! token; UUID owner alone is insufficient when a runner reuses its identity.
use crate::StoreError;
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(FromRow)]
pub struct JobLease {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub job_type: String,
    pub payload_ciphertext: Vec<u8>,
    pub payload_nonce: Vec<u8>,
    pub payload_key_version: i32,
    pub attempts: i32,
    pub lease_owner: Uuid,
}
impl JobLease {
    #[must_use]
    pub const fn should_execute(&self) -> bool {
        self.attempts <= 5
    }
}
pub struct JobRepository;
impl JobRepository {
    pub async fn claim(
        pool: &PgPool,
        owner: Uuid,
        seconds: i32,
    ) -> Result<Option<JobLease>, StoreError> {
        Ok(sqlx::query_as("WITH candidate AS (SELECT id FROM jobs WHERE run_after<=clock_timestamp() AND (status='queued' OR (status='running' AND lease_expires_at<=clock_timestamp())) ORDER BY run_after,created_at,id FOR UPDATE SKIP LOCKED LIMIT 1) UPDATE jobs j SET status='running',attempts=attempts+1,lease_owner=$1,lease_expires_at=clock_timestamp()+make_interval(secs=>$2),updated_at=clock_timestamp() FROM candidate c WHERE j.id=c.id RETURNING j.id,j.workspace_id,j.job_type,j.payload_ciphertext,j.payload_nonce,j.payload_key_version,j.attempts,j.lease_owner")
            .bind(owner).bind(f64::from(seconds.clamp(10,600))).fetch_optional(pool).await?)
    }
    pub async fn renew(pool: &PgPool, lease: &JobLease, seconds: i32) -> Result<bool, StoreError> {
        Ok(sqlx::query("UPDATE jobs SET lease_expires_at=clock_timestamp()+make_interval(secs=>$4),updated_at=clock_timestamp() WHERE id=$1 AND lease_owner=$2 AND attempts=$3 AND status='running' AND lease_expires_at>clock_timestamp()")
            .bind(lease.id).bind(lease.lease_owner).bind(lease.attempts).bind(f64::from(seconds.clamp(10,600))).execute(pool).await?.rows_affected()==1)
    }
    pub async fn finish(
        tx: &mut Transaction<'_, Postgres>,
        lease: &JobLease,
    ) -> Result<bool, StoreError> {
        Ok(sqlx::query("UPDATE jobs SET status='succeeded',lease_owner=NULL,lease_expires_at=NULL,error_summary=NULL,updated_at=clock_timestamp() WHERE id=$1 AND lease_owner=$2 AND attempts=$3 AND status='running' AND lease_expires_at>clock_timestamp()")
            .bind(lease.id).bind(lease.lease_owner).bind(lease.attempts).execute(&mut **tx).await?.rows_affected()==1)
    }
    pub async fn retry(
        tx: &mut Transaction<'_, Postgres>,
        lease: &JobLease,
        delay: i32,
    ) -> Result<bool, StoreError> {
        Ok(sqlx::query("UPDATE jobs SET status=CASE WHEN attempts>=5 THEN 'failed' ELSE 'queued' END,run_after=clock_timestamp()+make_interval(secs=>$4),lease_owner=NULL,lease_expires_at=NULL,error_summary='operation_failed',updated_at=clock_timestamp() WHERE id=$1 AND lease_owner=$2 AND attempts=$3 AND status='running' AND lease_expires_at>clock_timestamp()")
            .bind(lease.id).bind(lease.lease_owner).bind(lease.attempts).bind(f64::from(delay.clamp(0,3600))).execute(&mut **tx).await?.rows_affected()==1)
    }
}
