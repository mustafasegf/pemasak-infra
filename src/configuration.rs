use std::{
    io,
    net::{SocketAddr, ToSocketAddrs},
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
    pub builder: BuilderSettings,
}

#[derive(Deserialize, Debug, Clone)]
pub struct BuilderSettings {
    pub max_concurrent_builds: usize
}

#[derive(Deserialize, Debug, Clone)]
pub struct ApplicationSettings {
    pub port: u16,
    pub host: String,
    pub body_limit: String,
    pub secret: String,
    pub ipv6: bool,
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
}

#[derive(Deserialize, Debug, Clone)]
pub struct AuthSettings {
    pub git: bool,
    /// in hours
    pub lifespan: i64,
    pub cookie_name: String,
    /// in days
    pub cookie_max_age: i64,
    pub cookie_http_only: bool,
    pub cookie_secure: bool,
    /// in days
    pub max_lifespan: i64,
}

pub fn get_configuration() -> Result<Settings, ConfigError> {
    Config::builder()
        .set_default("application.host", "localhost")?
        .set_default("application.port", 8080)?
        .set_default("application.body_limit", "25mib")?
        .set_default("application.ipv6", false)?
        .set_default("database.user", "postgres")?
        .set_default("database.password", "postgres")?
        .set_default("database.host", "localhost")?
        .set_default("database.port", 5432)?
        .set_default("database.name", "postgres")?
        .set_default("database.timeout", 20)?
        .set_default("git.base", "./src/git-repo")?
        .set_default("application.git_auth", true)?
        .set_default("auth.git", true)?
        .set_default("auth.lifespan", 24 * 7)?
        .set_default("auth.cookie_name", "session")?
        .set_default("auth.cookie_max_age", 365)?
        .set_default("auth.cookie_http_only", true)?
        .set_default("auth.cookie_secure", false)?
        .set_default("auth.max_lifespan", 365)?
        .set_default("builder.max_concurrent_builds", 1)?
        .add_source(config::Environment::default().separator("_"))
        .add_source(config::File::with_name("configuration"))
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
        match self.application.port {
            80 | 443 => self.application.host.clone(),
            _ => format!("{}:{}", self.application.host, self.application.port),
        }
    }

    pub fn body_limit(&self) -> usize {
        Byte::from_str(&self.application.body_limit)
            .unwrap_or(Byte::from_bytes(25 * 1024 * 1024))
            .get_bytes() as usize
    }

    pub fn session_config(&self) -> SessionConfig {
        SessionConfig::default()
            .with_lifetime(Duration::hours(self.auth.lifespan))
            .with_cookie_name(self.auth.cookie_name.clone())
            .with_max_age(Some(Duration::days(self.auth.cookie_max_age)))
            .with_http_only(self.auth.cookie_http_only)
            .with_secure(self.auth.cookie_secure)
            .with_max_lifetime(Duration::days(self.auth.max_lifespan))
    }
}
