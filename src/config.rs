//! Typed application configuration.
//!
//! The environment is first deserialized (via serde/envy) into [`RawConfig`],
//! where every field is an `Option<String>`. A second pass parses and
//! validates each value while *collecting* problems, so a misconfigured
//! deployment fails fast with the complete list of what is missing or invalid
//! instead of dying one variable at a time.

use std::fmt;
use std::net::IpAddr;
use std::path::PathBuf;
use std::str::FromStr;

use axum::http::HeaderValue;
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppEnv {
    Development,
    Production,
}

impl FromStr for AppEnv {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "development" | "dev" => Ok(Self::Development),
            "production" | "prod" => Ok(Self::Production),
            other => Err(format!("expected development|production, got {other:?}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    Json,
    Pretty,
}

impl FromStr for LogFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "json" => Ok(Self::Json),
            "pretty" => Ok(Self::Pretty),
            other => Err(format!("expected json|pretty, got {other:?}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StorageDriver {
    Local,
    S3,
}

impl FromStr for StorageDriver {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "local" => Ok(Self::Local),
            "s3" => Ok(Self::S3),
            other => Err(format!("expected local|s3, got {other:?}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NotifierDriver {
    Log,
    Fcm,
}

impl FromStr for NotifierDriver {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "log" => Ok(Self::Log),
            "fcm" => Ok(Self::Fcm),
            other => Err(format!("expected log|fcm, got {other:?}")),
        }
    }
}

/// Which file-storage backend to construct at boot.
#[derive(Debug, Clone)]
pub enum StorageConfig {
    /// Development driver: files under a local directory, "presigned" URLs are
    /// HMAC-signed links back into this API.
    Local { root: PathBuf },
    /// OCI Object Storage via its S3 Compatibility API (or any S3-compatible
    /// store). Credentials are OCI Customer Secret Keys.
    S3 {
        endpoint: String,
        region: String,
        bucket: String,
        access_key_id: String,
        secret_access_key: String,
    },
}

/// Which push-notification backend to construct at boot.
#[derive(Debug, Clone)]
pub enum NotifierConfig {
    /// Development driver: notifications are written to the log.
    Log,
    /// Firebase Cloud Messaging HTTP v1 API, sending to a topic both clinic
    /// devices subscribe to.
    Fcm {
        service_account_path: PathBuf,
        topic: String,
    },
}

#[derive(Debug, Clone)]
pub struct Config {
    pub app_env: AppEnv,
    pub http_host: IpAddr,
    pub http_port: u16,
    pub log_format: LogFormat,
    /// Absolute origin of this API as reachable by clients; used by the local
    /// storage driver to mint absolute "presigned" URLs.
    pub public_base_url: String,
    pub database_url: String,
    pub database_max_connections: u32,
    pub auto_migrate: bool,
    pub jwt_secret: String,
    pub access_token_ttl_secs: i64,
    pub refresh_token_ttl_days: i64,
    pub cors_allowed_origins: Vec<String>,
    pub rate_limit_enabled: bool,
    pub rate_limit_per_second: u64,
    pub rate_limit_burst: u32,
    pub request_timeout_secs: u64,
    pub body_limit_bytes: usize,
    pub presign_ttl_secs: u64,
    pub storage: StorageConfig,
    pub notifier: NotifierConfig,
    pub scheduler_enabled: bool,
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        let raw: RawConfig = envy::from_env().map_err(|e| ConfigError {
            problems: vec![format!("failed to read environment: {e}")],
        })?;
        Self::from_raw(raw)
    }

    fn from_raw(raw: RawConfig) -> Result<Self, ConfigError> {
        let mut p = Problems::default();

        let app_env = p.or_default("APP_ENV", &raw.app_env, AppEnv::Development);
        let http_host: IpAddr =
            p.or_default("HTTP_HOST", &raw.http_host, IpAddr::from([0, 0, 0, 0]));
        let http_port: u16 = p.or_default("HTTP_PORT", &raw.http_port, 8080);
        let log_format = p.or_default("LOG_FORMAT", &raw.log_format, LogFormat::Json);

        let database_url: Option<String> = p.required("DATABASE_URL", &raw.database_url);
        let database_max_connections: u32 = p.or_default(
            "DATABASE_MAX_CONNECTIONS",
            &raw.database_max_connections,
            10,
        );
        if database_max_connections == 0 {
            p.push("DATABASE_MAX_CONNECTIONS must be at least 1");
        }
        let auto_migrate = p.or_default("AUTO_MIGRATE", &raw.auto_migrate, true);

        let jwt_secret: Option<String> = p.required("JWT_SECRET", &raw.jwt_secret);
        if let Some(secret) = &jwt_secret
            && secret.len() < 32
        {
            p.push("JWT_SECRET must be at least 32 characters (openssl rand -hex 32)");
        }
        let access_token_ttl_secs: i64 =
            p.or_default("ACCESS_TOKEN_TTL_SECS", &raw.access_token_ttl_secs, 900);
        if access_token_ttl_secs <= 0 {
            p.push("ACCESS_TOKEN_TTL_SECS must be positive");
        }
        let refresh_token_ttl_days: i64 =
            p.or_default("REFRESH_TOKEN_TTL_DAYS", &raw.refresh_token_ttl_days, 30);
        if refresh_token_ttl_days <= 0 {
            p.push("REFRESH_TOKEN_TTL_DAYS must be positive");
        }

        let cors_allowed_origins: Vec<String> = raw
            .cors_allowed_origins
            .as_deref()
            .unwrap_or("")
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect();
        for origin in &cors_allowed_origins {
            let scheme_ok = origin.starts_with("http://") || origin.starts_with("https://");
            if !scheme_ok || HeaderValue::from_str(origin).is_err() {
                p.push(format!(
                    "CORS_ALLOWED_ORIGINS entry {origin:?} is not a valid origin"
                ));
            }
        }

        let rate_limit_enabled = p.or_default("RATE_LIMIT_ENABLED", &raw.rate_limit_enabled, true);
        // sized for image-heavy screens: opening a page of pet photos fires a
        // burst of presign requests on top of the JSON calls
        let rate_limit_per_second: u64 =
            p.or_default("RATE_LIMIT_PER_SECOND", &raw.rate_limit_per_second, 25);
        if rate_limit_per_second == 0 {
            p.push("RATE_LIMIT_PER_SECOND must be at least 1");
        }
        let rate_limit_burst: u32 = p.or_default("RATE_LIMIT_BURST", &raw.rate_limit_burst, 100);
        if rate_limit_burst == 0 {
            p.push("RATE_LIMIT_BURST must be at least 1");
        }
        let request_timeout_secs: u64 =
            p.or_default("REQUEST_TIMEOUT_SECS", &raw.request_timeout_secs, 30);
        if request_timeout_secs == 0 {
            p.push("REQUEST_TIMEOUT_SECS must be at least 1");
        }
        let body_limit_bytes: usize =
            p.or_default("BODY_LIMIT_BYTES", &raw.body_limit_bytes, 2 * 1024 * 1024);
        if body_limit_bytes < 1024 {
            p.push("BODY_LIMIT_BYTES must be at least 1024");
        }
        let presign_ttl_secs: u64 = p.or_default("PRESIGN_TTL_SECS", &raw.presign_ttl_secs, 900);
        if presign_ttl_secs < 60 {
            p.push("PRESIGN_TTL_SECS must be at least 60");
        }

        let public_base_url = raw
            .public_base_url
            .clone()
            .unwrap_or_else(|| format!("http://localhost:{http_port}"))
            .trim_end_matches('/')
            .to_string();

        let storage =
            match p.or_default("STORAGE_DRIVER", &raw.storage_driver, StorageDriver::Local) {
                StorageDriver::Local => StorageConfig::Local {
                    root: PathBuf::from(
                        raw.storage_local_root
                            .clone()
                            .unwrap_or_else(|| "./storage-data".to_string()),
                    ),
                },
                StorageDriver::S3 => {
                    let endpoint: Option<String> = p.required("S3_ENDPOINT", &raw.s3_endpoint);
                    let region: Option<String> = p.required("S3_REGION", &raw.s3_region);
                    let bucket: Option<String> = p.required("S3_BUCKET", &raw.s3_bucket);
                    let access_key_id: Option<String> =
                        p.required("S3_ACCESS_KEY_ID", &raw.s3_access_key_id);
                    let secret_access_key: Option<String> =
                        p.required("S3_SECRET_ACCESS_KEY", &raw.s3_secret_access_key);
                    match (endpoint, region, bucket, access_key_id, secret_access_key) {
                        (
                            Some(endpoint),
                            Some(region),
                            Some(bucket),
                            Some(access_key_id),
                            Some(secret_access_key),
                        ) => StorageConfig::S3 {
                            endpoint,
                            region,
                            bucket,
                            access_key_id,
                            secret_access_key,
                        },
                        // Placeholder — problems were recorded above, so boot fails
                        // before this value is ever used.
                        _ => StorageConfig::Local {
                            root: PathBuf::from("./storage-data"),
                        },
                    }
                }
            };

        let notifier =
            match p.or_default("NOTIFIER_DRIVER", &raw.notifier_driver, NotifierDriver::Log) {
                NotifierDriver::Log => NotifierConfig::Log,
                NotifierDriver::Fcm => {
                    let path: Option<String> =
                        p.required("FCM_SERVICE_ACCOUNT_PATH", &raw.fcm_service_account_path);
                    let topic: Option<String> = p.required("FCM_TOPIC", &raw.fcm_topic);
                    match (path, topic) {
                        (Some(path), Some(topic)) => NotifierConfig::Fcm {
                            service_account_path: PathBuf::from(path),
                            topic,
                        },
                        _ => NotifierConfig::Log, // placeholder, see above
                    }
                }
            };

        let scheduler_enabled = p.or_default("SCHEDULER_ENABLED", &raw.scheduler_enabled, true);

        if !p.problems.is_empty() {
            return Err(ConfigError {
                problems: p.problems,
            });
        }

        Ok(Config {
            app_env,
            http_host,
            http_port,
            log_format,
            public_base_url,
            database_url: database_url.expect("checked above"),
            database_max_connections,
            auto_migrate,
            jwt_secret: jwt_secret.expect("checked above"),
            access_token_ttl_secs,
            refresh_token_ttl_days,
            cors_allowed_origins,
            rate_limit_enabled,
            rate_limit_per_second,
            rate_limit_burst,
            request_timeout_secs,
            body_limit_bytes,
            presign_ttl_secs,
            storage,
            notifier,
            scheduler_enabled,
        })
    }
}

/// All configuration problems found in one validation pass.
#[derive(Debug)]
pub struct ConfigError {
    pub problems: Vec<String>,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "invalid configuration ({} problem(s)):",
            self.problems.len()
        )?;
        for problem in &self.problems {
            writeln!(f, "  - {problem}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ConfigError {}

/// Untyped mirror of the environment. Field names map 1:1 to UPPER_SNAKE_CASE
/// environment variables (envy uppercases them when looking up).
#[derive(Debug, Default, Deserialize)]
struct RawConfig {
    app_env: Option<String>,
    http_host: Option<String>,
    http_port: Option<String>,
    log_format: Option<String>,
    public_base_url: Option<String>,
    database_url: Option<String>,
    database_max_connections: Option<String>,
    auto_migrate: Option<String>,
    jwt_secret: Option<String>,
    access_token_ttl_secs: Option<String>,
    refresh_token_ttl_days: Option<String>,
    cors_allowed_origins: Option<String>,
    rate_limit_enabled: Option<String>,
    rate_limit_per_second: Option<String>,
    rate_limit_burst: Option<String>,
    request_timeout_secs: Option<String>,
    body_limit_bytes: Option<String>,
    presign_ttl_secs: Option<String>,
    storage_driver: Option<String>,
    storage_local_root: Option<String>,
    s3_endpoint: Option<String>,
    s3_region: Option<String>,
    s3_bucket: Option<String>,
    s3_access_key_id: Option<String>,
    s3_secret_access_key: Option<String>,
    notifier_driver: Option<String>,
    fcm_service_account_path: Option<String>,
    fcm_topic: Option<String>,
    scheduler_enabled: Option<String>,
}

/// Accumulates every parse/validation problem instead of failing on the first.
#[derive(Default)]
struct Problems {
    problems: Vec<String>,
}

impl Problems {
    fn push(&mut self, problem: impl Into<String>) {
        self.problems.push(problem.into());
    }

    /// Parse a required variable; records a problem and yields `None` when
    /// missing or unparseable.
    fn required<T: FromStr>(&mut self, name: &str, raw: &Option<String>) -> Option<T>
    where
        T::Err: fmt::Display,
    {
        match raw {
            None => {
                self.push(format!("{name} is required but not set"));
                None
            }
            Some(value) => match value.parse() {
                Ok(parsed) => Some(parsed),
                Err(e) => {
                    self.push(format!("{name}={value:?} is invalid: {e}"));
                    None
                }
            },
        }
    }

    /// Parse an optional variable, falling back to `default` when unset.
    /// A present-but-unparseable value is still recorded as a problem.
    fn or_default<T: FromStr>(&mut self, name: &str, raw: &Option<String>, default: T) -> T
    where
        T::Err: fmt::Display,
    {
        match raw {
            None => default,
            Some(value) => match value.parse() {
                Ok(parsed) => parsed,
                Err(e) => {
                    self.push(format!("{name}={value:?} is invalid: {e}"));
                    default
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_raw() -> RawConfig {
        RawConfig {
            database_url: Some("postgres://u:p@localhost/db".to_string()),
            jwt_secret: Some("0123456789abcdef0123456789abcdef".to_string()),
            ..RawConfig::default()
        }
    }

    #[test]
    fn minimal_config_gets_defaults() {
        let config = Config::from_raw(minimal_raw()).expect("minimal config should be valid");
        assert_eq!(config.http_port, 8080);
        assert_eq!(config.access_token_ttl_secs, 900);
        assert_eq!(config.refresh_token_ttl_days, 30);
        assert!(config.rate_limit_enabled);
        assert!(matches!(config.storage, StorageConfig::Local { .. }));
        assert!(matches!(config.notifier, NotifierConfig::Log));
    }

    #[test]
    fn missing_and_invalid_vars_are_all_reported_at_once() {
        let raw = RawConfig {
            http_port: Some("not-a-port".to_string()),
            jwt_secret: Some("short".to_string()),
            storage_driver: Some("s3".to_string()), // S3 vars missing → 5 more problems
            ..RawConfig::default()
        };
        let err = Config::from_raw(raw).expect_err("config must be rejected");
        let text = err.to_string();
        assert!(text.contains("DATABASE_URL is required"), "{text}");
        assert!(text.contains("HTTP_PORT"), "{text}");
        assert!(text.contains("JWT_SECRET must be at least 32"), "{text}");
        assert!(text.contains("S3_ENDPOINT is required"), "{text}");
        assert!(text.contains("S3_SECRET_ACCESS_KEY is required"), "{text}");
        assert!(
            err.problems.len() >= 7,
            "expected all problems collected, got: {text}"
        );
    }

    #[test]
    fn cors_origins_are_split_and_validated() {
        let mut raw = minimal_raw();
        raw.cors_allowed_origins =
            Some("https://app.example.com, http://localhost:3000".to_string());
        let config = Config::from_raw(raw).unwrap();
        assert_eq!(
            config.cors_allowed_origins,
            vec![
                "https://app.example.com".to_string(),
                "http://localhost:3000".to_string()
            ]
        );

        let mut bad = minimal_raw();
        bad.cors_allowed_origins = Some("ftp://nope".to_string());
        assert!(Config::from_raw(bad).is_err());
    }
}
