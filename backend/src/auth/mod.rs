use anyhow::Result;
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use rand::Rng;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::path::Path;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct User {
    pub id: String,
    pub username: String,
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub role: String,
    pub force_password_change: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub user_id: String,
    pub expires_at: i64,
    pub created_at: i64,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicUser {
    pub id: String,
    pub username: String,
    pub role: String,
    pub force_password_change: bool,
}

impl From<User> for PublicUser {
    fn from(u: User) -> Self {
        Self {
            id: u.id,
            username: u.username,
            role: u.role,
            force_password_change: u.force_password_change,
        }
    }
}

pub fn hash_password(password: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("Password hashing failed: {e}"))?
        .to_string();
    Ok(hash)
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    let parsed = match PasswordHash::new(hash) {
        Ok(h) => h,
        Err(_) => return false,
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

pub fn generate_session_token() -> String {
    let bytes: [u8; 32] = rand::thread_rng().gen();
    hex::encode(bytes)
}

pub fn generate_bootstrap_token() -> String {
    let bytes: [u8; 20] = rand::thread_rng().gen();
    // Format as groups for readability: vtk-XXXX-XXXX-XXXX-XXXX
    let hex = hex::encode(bytes);
    format!(
        "vtk-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20]
    )
}

pub async fn ensure_bootstrap_token(token_path: &Path) -> Result<Option<String>> {
    if token_path.exists() {
        return Ok(None);
    }

    let token = generate_bootstrap_token();
    if let Some(parent) = token_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(token_path, &token)?;

    // Set restrictive permissions on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(token_path, std::fs::Permissions::from_mode(0o600))?;
    }

    Ok(Some(token))
}

pub async fn read_bootstrap_token(token_path: &Path) -> Result<Option<String>> {
    if !token_path.exists() {
        return Ok(None);
    }
    let token = std::fs::read_to_string(token_path)?.trim().to_string();
    Ok(if token.is_empty() { None } else { Some(token) })
}

pub async fn create_user(
    pool: &SqlitePool,
    username: &str,
    password: &str,
    role: &str,
    force_change: bool,
) -> Result<User> {
    let id = Uuid::new_v4().to_string();
    let hash = hash_password(password)?;
    let now = unix_now();

    sqlx::query(
        "INSERT INTO users (id, username, password_hash, role, force_password_change, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(username)
    .bind(&hash)
    .bind(role)
    .bind(force_change)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await?;

    Ok(User {
        id,
        username: username.to_string(),
        password_hash: hash,
        role: role.to_string(),
        force_password_change: force_change,
        created_at: now,
        updated_at: now,
    })
}

pub async fn change_password(
    pool: &SqlitePool,
    user_id: &str,
    new_password: &str,
    new_username: Option<&str>,
) -> Result<()> {
    let hash = hash_password(new_password)?;
    let now = unix_now();
    if let Some(name) = new_username {
        sqlx::query(
            "UPDATE users SET password_hash = ?, username = ?, force_password_change = 0, updated_at = ? WHERE id = ?"
        )
        .bind(&hash).bind(name).bind(now).bind(user_id)
        .execute(pool).await?;
    } else {
        sqlx::query(
            "UPDATE users SET password_hash = ?, force_password_change = 0, updated_at = ? WHERE id = ?"
        )
        .bind(&hash).bind(now).bind(user_id)
        .execute(pool).await?;
    }
    Ok(())
}

pub async fn find_user_by_username(pool: &SqlitePool, username: &str) -> Result<Option<User>> {
    let user = sqlx::query_as::<_, User>(
        "SELECT id, username, password_hash, role, force_password_change, created_at, updated_at
         FROM users WHERE username = ?",
    )
    .bind(username)
    .fetch_optional(pool)
    .await?;
    Ok(user)
}

pub async fn create_session(
    pool: &SqlitePool,
    user_id: &str,
    ip: Option<&str>,
    ua: Option<&str>,
) -> Result<Session> {
    let id = generate_session_token();
    let now = unix_now();
    let expires_at = now + 7 * 24 * 3600; // 7 days

    sqlx::query(
        "INSERT INTO sessions (id, user_id, expires_at, created_at, ip_address, user_agent)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(user_id)
    .bind(expires_at)
    .bind(now)
    .bind(ip)
    .bind(ua)
    .execute(pool)
    .await?;

    Ok(Session {
        id,
        user_id: user_id.to_string(),
        expires_at,
        created_at: now,
        ip_address: ip.map(str::to_string),
        user_agent: ua.map(str::to_string),
    })
}

pub async fn validate_session(pool: &SqlitePool, session_id: &str) -> Result<Option<User>> {
    let now = unix_now();
    let row = sqlx::query_as::<_, User>(
        "SELECT u.id, u.username, u.password_hash, u.role, u.force_password_change, u.created_at, u.updated_at
         FROM sessions s
         JOIN users u ON u.id = s.user_id
         WHERE s.id = ? AND s.expires_at > ?",
    )
    .bind(session_id)
    .bind(now)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn delete_session(pool: &SqlitePool, session_id: &str) -> Result<()> {
    sqlx::query("DELETE FROM sessions WHERE id = ?")
        .bind(session_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete_expired_sessions(pool: &SqlitePool) -> Result<u64> {
    let now = unix_now();
    let result = sqlx::query("DELETE FROM sessions WHERE expires_at < ?")
        .bind(now)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

pub async fn validate_api_token(
    pool: &SqlitePool,
    raw_token: &str,
    required_scope: &str,
) -> Result<String> {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(raw_token.as_bytes());
    let token_hash = hex::encode(h.finalize());

    #[derive(sqlx::FromRow)]
    struct TokenRow {
        id: String,
        user_id: String,
        scopes: String,
        expires_at: Option<i64>,
    }

    let row = sqlx::query_as::<_, TokenRow>(
        "SELECT id, user_id, scopes, expires_at FROM api_tokens WHERE token_hash = ?",
    )
    .bind(&token_hash)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| anyhow::anyhow!("Invalid token"))?;

    let now = unix_now();
    if let Some(exp) = row.expires_at {
        if exp < now {
            return Err(anyhow::anyhow!("Token expired"));
        }
    }

    let scopes: Vec<String> = serde_json::from_str(&row.scopes).unwrap_or_default();
    if !scopes.iter().any(|s| s == required_scope) {
        return Err(anyhow::anyhow!("Token missing required scope: {required_scope}"));
    }

    let _ = sqlx::query("UPDATE api_tokens SET last_used_at = ? WHERE id = ?")
        .bind(now)
        .bind(&row.id)
        .execute(pool)
        .await;

    Ok(row.user_id)
}

pub async fn has_any_user(pool: &SqlitePool) -> Result<bool> {
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await?;
    Ok(count.0 > 0)
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
