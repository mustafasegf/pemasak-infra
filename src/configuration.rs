use std::{
    io,
    net::{SocketAddr, ToSocketAddrs},
    num::NonZeroUsize,
    thread::available_parallelism,
};

use axum_session::SessionConfig;
use byte_unit::Byte;
use chrono::Duration;
use config::{Config, ConfigError};
use serde::Deserialize;
use sqlx::postgres::PgConnectOptions;

#[derive(Deserialize, Debug, Clone)]
pub struct Settings {
    pub database: DatabaseSettings,
    pub application: ApplicationSettings,
    pub git: GitSettings,
    pub auth: AuthSettings,
    pub build: BuilderSettings,
}

#[derive(Deserialize, Debug, Clone)]
pub struct BuilderSettings {
    pub max: usize,
    pub timeout: usize,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ApplicationSettings {
    pub port: u16,
    pub host: String,
    pub domain: String,
    pub hostip: String,
    pub bodylimit: String,
    pub ipv6: bool,
    pub secure: bool,
    pub idle: u64,
}

#[derive(Deserialize, Debug, Clone)]
pub struct DatabaseSettings {
    pub user: String,
    pub password: String,
    pub host: String,
    pub port: u16,
    pub name: String,
    pub timeout: u64,
}

#[derive(Deserialize, Debug, Clone)]
pub struct GitSettings {
    pub base: String,
    pub auth: bool,
}

// TODO: _ doesn't work for env vars
#[derive(Deserialize, Debug, Clone)]
pub struct AuthSettings {
    pub sso: bool,
    pub register: bool,
    /// in hours
    pub lifespan: i64,
    pub cookiename: String,
    /// in days
    pub maxage: i64,
    pub httponly: bool,
    pub secure: bool,
    /// in days
    pub maxlifespan: i64,
}

pub fn get_configuration() -> Result<Settings, ConfigError> {
    Config::builder()
        .set_default("application.port", 8080)?
        .set_default("application.host", "0.0.0.0")?
        .set_default("application.domain", "localhost:8080")?
        .set_default("application.hostip", "127.0.0.1")?
        .set_default("application.bodylimit", "25mib")?
        .set_default("application.ipv6", false)?
        .set_default("application.secure", false)?
        .set_default("application.idle", 30)?
        .set_default("database.user", "postgres")?
        .set_default("database.password", "postgres")?
        .set_default("database.host", "localhost")?
        .set_default("database.port", 5432)?
        .set_default("database.name", "postgres")?
        .set_default("database.timeout", 20)?
        .set_default("git.base", "./git-repo")?
        .set_default("git.auth", true)?
        .set_default("auth.sso", true)?
        .set_default("auth.register", true)?
        .set_default("auth.lifespan", 24 * 7)?
        .set_default("auth.cookiename", "session")?
        .set_default("auth.maxage", 365)?
        .set_default("auth.httponly", true)?
        .set_default("auth.secure", false)?
        .set_default("auth.maxlifespan", 365)?
        .set_default("build.timeout", 120000)?
        .set_default(
            "builder.max",
            available_parallelism()
                .unwrap_or(NonZeroUsize::new(3).unwrap())
                .get() as i32
                - 1,
        )?
        .set_default("builder.cpums", 100000)?
        .add_source(config::File::with_name("configuration"))
        .add_source(config::Environment::default().separator("_"))
        .build()?
        .try_deserialize::<Settings>()
}

impl Settings {
    pub fn connection_options(&self) -> PgConnectOptions {
        PgConnectOptions::new()
            .host(&self.database.host)
            .port(self.database.port)
            .username(&self.database.user)
            .password(&self.database.password)
            .database(&self.database.name)
    }

    pub fn address_string(&self) -> String {
        format!("{}:{}", self.application.host, self.application.port)
    }

    pub fn address(&self) -> io::Result<SocketAddr> {
        self.address_string()
            .to_socket_addrs()?
            .min_by_key(|addr| match addr {
                SocketAddr::V4(_) => self.application.ipv6 as usize,
                SocketAddr::V6(_) => self.application.ipv6 as usize ^ 1,
            })
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "invalid address"))
    }

    pub fn domain(&self) -> String {
        self.application.domain.clone()

        // match self.application.port {
        //     80 | 443 => self.application.domain.clone(),
        //     _ => format!("{}:{}", self.application.domain, self.application.port),
        // }
    }

    pub fn body_limit(&self) -> usize {
        Byte::from_str(&self.application.bodylimit)
            .unwrap_or(Byte::from_bytes(25 * 1024 * 1024))
            .get_bytes() as usize
    }

    pub fn session_config(&self) -> SessionConfig {
        SessionConfig::default()
            .with_lifetime(Duration::hours(self.auth.lifespan))
            .with_cookie_name(self.auth.cookiename.clone())
            .with_max_age(Some(Duration::days(self.auth.maxage)))
            .with_http_only(self.auth.httponly)
            .with_secure(self.auth.secure)
            .with_max_lifetime(Duration::days(self.auth.maxlifespan))
    }
}
