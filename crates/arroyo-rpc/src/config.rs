use figment::providers::{Env, Format, Json, Toml, Yaml};
use figment::Figment;
use k8s_openapi::api::core::v1::{EnvVar, ResourceRequirements, Volume, VolumeMount};
use regex::Regex;
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fmt::{Debug, Formatter};
use std::net::IpAddr;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::process::exit;
use std::str::FromStr;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use url::Url;

const DEFAULT_CONFIG: &str = include_str!("../default.toml");

static CONFIG: OnceLock<Arc<Config>> = OnceLock::new();

pub fn initialize_config(path: Option<&Path>) {
    if let Some(path) = path {
        if !path.exists() {
            eprintln!(
                "Cannot load configuration from {}; file does not exist",
                path.to_string_lossy()
            );
            exit(1);
        }
    }

    CONFIG
        .set(match load_config(path).extract() {
            Ok(config) => Arc::new(config),
            Err(errors) => {
                eprintln!("Configuration is invalid!");
                for err in errors {
                    eprintln!("  • {err}");
                }

                exit(1);
            }
        })
        .expect("Unable to initialize global config!");
}

pub fn config() -> &'static Arc<Config> {
    CONFIG
        .get()
        .expect("Configuration was accessed before initialization!")
}

fn load_config(path: Option<&Path>) -> Figment {
    // Priority (from highest to lowest) is:
    //   1. ARROYO_* environment variables
    //   2. The config file specified in <path>
    //   3. arroyo.toml in the current directory
    //   4. $(confdir)/arroyo/config.toml
    //   5. ../default.toml
    let mut figment = Figment::from(Toml::string(DEFAULT_CONFIG));

    if let Some(config_dir) = dirs::config_dir() {
        figment = figment.merge(Toml::file(config_dir.join("arroyo/config.toml")));
    }

    figment = figment.merge(Toml::file("arroyo.toml"));

    if let Some(path) = path {
        match path.extension().and_then(OsStr::to_str) {
            Some("yaml") => {
                figment = figment.merge(Yaml::file(path));
            }
            Some("json") => {
                figment = figment.merge(Json::file(path));
            }
            _ => {
                figment = figment.merge(Toml::file(path));
            }
        }
    };

    figment.merge(Env::prefixed("ARROYO_").split("_"))
}

/// Arroyo configuration
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Config {
    /// API service configuration
    pub api: ApiConfig,

    /// Controller service configuration
    pub controller: ControllerConfig,

    /// Compiler service configuration
    pub compiler: CompilerConfig,

    /// Node service configuration
    pub node: NodeConfig,

    /// Worker configuration
    pub worker: WorkerConfig,

    /// Admin service configuration
    pub admin: AdminConfig,

    /// Default pipeline configuration
    pub pipeline: PipelineConfig,

    /// Database configuration
    pub database: DatabaseConfig,

    /// Process scheduler configuration
    pub process_scheduler: ProcessSchedulerConfig,

    // Kubernetes scheduler configuration
    pub kubernetes_scheduler: KubernetesSchedulerConfig,

    /// URL of an object store or filesystem for storing checkpoints
    pub checkpoint_url: String,

    /// The endpoint of the controller, used by other services to connect to it. This must be set
    /// if running the controller on a separate machine from the other services or on a separate
    /// process with a non-standard port.
    controller_endpoint: Option<Url>,

    // The endpoint of the API service, used by the Web UI
    pub api_endpoint: Option<Url>,

    /// The endpoint of the compiler, used by the API server to connect to it. This must be set
    /// if running the compiler on a separate machine from the other services or on a separate
    /// process with a non-standard port.
    compiler_endpoint: Option<Url>,

    /// Telemetry config
    #[serde(default)]
    pub disable_telemetry: bool,
}

impl Config {
    pub fn controller_endpoint(&self) -> String {
        self.controller_endpoint
            .as_ref()
            .map(|t| t.to_string())
            .unwrap_or_else(|| format!("http://localhost:{}", self.controller.rpc_port))
    }

    pub fn compiler_endpoint(&self) -> String {
        self.compiler_endpoint
            .as_ref()
            .map(|t| t.to_string())
            .unwrap_or_else(|| format!("http://localhost:{}", self.compiler.rpc_port))
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct ApiConfig {
    /// The host the API service should bind to
    pub bind_address: IpAddr,

    /// The HTTP port for the API service
    pub http_port: u16,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct ControllerConfig {
    /// The host the controller should bind to
    pub bind_address: IpAddr,

    /// The RPC port for the controller
    pub rpc_port: u16,

    /// The scheduler to use
    pub scheduler: Scheduler,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct CompilerConfig {
    /// Bind address for the compiler
    pub bind_address: IpAddr,

    /// RPC port for the compiler
    pub rpc_port: u16,

    /// Whether the compiler should attempt to install clang if it's not already installed
    pub install_clang: bool,

    /// Whether the compiler should attempt to install rustc if it's not already installed
    pub install_rustc: bool,

    /// Where to store compilation artifacts
    pub artifact_url: String,

    /// Directory to build artifacts in
    pub build_dir: String,

    /// Whether to use a local version of the UDF library or the published crate (only
    /// enable in development environments)
    #[serde(default)]
    pub use_local_udf_crate: bool,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct WorkerConfig {
    /// Bind address for the worker RPC socket
    pub bind_address: IpAddr,

    /// RPC port for the worker to listen on; set to 0 to use a random available port
    pub rpc_port: u16,

    /// Data port for the worker to listen on; set to 0 to use a random available port
    pub data_port: u16,

    /// Number of task slots for this worker
    pub task_slots: u32,

    /// ID for this worker
    pub id: Option<u64>,

    /// Name to identify this worker (e.g., e.g., its hostname or a pod name)
    pub name: String,

    /// Size of the queues between nodes in the dataflow graph
    pub queue_size: u32,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct NodeConfig {
    /// Bind address for the node service
    pub bind_address: IpAddr,

    /// RPC port for the node service
    pub rpc_port: u16,

    /// Number of task slots for this node
    pub task_slots: u32,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct AdminConfig {
    /// Bind address for the admin service
    pub bind_address: IpAddr,

    /// HTTP port the admin service will listen on
    pub http_port: u16,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PipelineConfig {
    /// Batch size
    pub source_batch_size: usize,

    /// Batch linger time (how long to wait before flushing)
    pub source_batch_linger: HumanReadableDuration,

    // How often to flush aggregates
    pub update_aggregate_flush_interval: HumanReadableDuration,
}

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum DatabaseType {
    Postgres,
    Sqlite,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct DatabaseConfig {
    pub r#type: DatabaseType,
    pub postgres: PostgresConfig,
    #[serde(default)]
    pub sqlite: SqliteConfig,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PostgresConfig {
    pub database_name: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct SqliteConfig {
    pub path: PathBuf,
}

impl Default for SqliteConfig {
    fn default() -> Self {
        Self {
            path: dirs::config_dir()
                .map(|p| p.join("arroyo/config.sqlite"))
                .unwrap_or_else(|| PathBuf::from_str("config.sqlite").unwrap()),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Scheduler {
    Embedded,
    Process,
    Node,
    Kubernetes,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct ProcessSchedulerConfig {
    pub slots_per_process: u32,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct KubernetesSchedulerConfig {
    pub namespace: String,
    pub worker: KubernetesWorkerConfig,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct KubernetesWorkerConfig {
    name_prefix: String,

    name: Option<String>,

    pub image: String,

    pub image_pull_policy: String,

    pub service_account_name: String,

    #[serde(default)]
    pub labels: BTreeMap<String, String>,

    #[serde(default)]
    pub annotations: BTreeMap<String, String>,

    #[serde(default)]
    pub env: Vec<EnvVar>,

    pub resources: ResourceRequirements,

    pub task_slots: u32,

    #[serde(default)]
    pub volumes: Vec<Volume>,

    #[serde(default)]
    pub volume_mounts: Vec<VolumeMount>,

    pub config_map: Option<String>,
}

impl KubernetesWorkerConfig {
    pub fn name(&self) -> String {
        self.name
            .as_ref()
            .cloned()
            .unwrap_or_else(|| format!("{}-worker", self.name_prefix))
    }
}

pub struct HumanReadableDuration {
    duration: Duration,
    original: String,
}

impl Deref for HumanReadableDuration {
    type Target = Duration;

    fn deref(&self) -> &Self::Target {
        &self.duration
    }
}

impl Debug for HumanReadableDuration {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.original.fmt(f)
    }
}

impl Serialize for HumanReadableDuration {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.original)
    }
}

impl<'de> Deserialize<'de> for HumanReadableDuration {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let str = String::deserialize(deserializer)?;

        let r = Regex::new(r"^(\d+)\s*([a-zA-Zµ]+)$").unwrap();
        let captures = r.captures(&str).ok_or_else(|| {
            de::Error::custom(format!("invalid duration specification '{}'", str))
        })?;
        let mut capture = captures.iter();

        capture.next();

        let n: u64 = capture.next().unwrap().unwrap().as_str().parse().unwrap();
        let unit = capture.next().unwrap().unwrap().as_str();

        let duration = match unit {
            "ns" | "nanos" => Duration::from_nanos(n),
            "µs" | "micros" => Duration::from_micros(n),
            "ms" | "millis" => Duration::from_millis(n),
            "s" | "secs" | "seconds" => Duration::from_secs(n),
            "m" | "mins" | "minutes" => Duration::from_secs(n * 60),
            "h" | "hrs" | "hours" => Duration::from_secs(n * 60 * 60),
            x => return Err(de::Error::custom(format!("unknown time unit '{}'", x))),
        };

        Ok(HumanReadableDuration {
            duration,
            original: str,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::config::{load_config, Config, DatabaseType, SqliteConfig};

    #[test]
    fn test_config() {
        figment::Jail::expect_with(|jail| {
            // test default loading
            let _config: Config = load_config(None).extract().unwrap();

            // try overriding database by config file
            jail.create_file(
                "arroyo.toml",
                r#"
            [database]
            type = "sqlite"
            "#,
            )
            .unwrap();

            let config: Config = load_config(None).extract().unwrap();
            assert_eq!(config.database.sqlite.path, SqliteConfig::default().path);
            assert_eq!(config.database.r#type, DatabaseType::Sqlite);

            // try overriding with environment variables
            jail.set_env("ARROYO_ADMIN_HTTP-PORT", 9111);
            let config: Config = load_config(None).extract().unwrap();
            assert_eq!(config.admin.http_port, 9111);

            Ok(())
        });
    }
}
