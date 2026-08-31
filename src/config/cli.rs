use clap::{Parser, Subcommand, ValueEnum};
use std::net::SocketAddr;
use std::path::PathBuf;

pub const DEFAULT_LISTEN: &str = "0.0.0.0:8080";
pub const DEFAULT_UPSTREAM: &str = "http://127.0.0.1:80";
pub const DEFAULT_DB_PATH: &str = "./data/reqlens.db";
pub const DEFAULT_MAX_BODY: usize = 65536;

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum CaptureMode {
    /// Captura pasiva; Apache conserva su puerto y ReqLens solo observa
    Sniff,
    /// Proxy reverso explícito; ReqLens queda en el camino del tráfico
    Proxy,
}

#[derive(Parser, Debug, Clone)]
#[command(
    name = "reqlens",
    version,
    about = "Passive HTTP observability and optional reverse proxy for Apache"
)]
pub struct CliArgs {
    #[command(subcommand)]
    pub command: Option<Commands>,

    #[arg(long, env = "REQLENS_LISTEN", default_value = DEFAULT_LISTEN)]
    pub listen: String,

    #[arg(long, env = "REQLENS_UPSTREAM", default_value = DEFAULT_UPSTREAM)]
    pub upstream: String,

    #[arg(long, env = "REQLENS_DB_PATH", default_value = DEFAULT_DB_PATH)]
    pub db_path: PathBuf,

    #[arg(long, env = "REQLENS_MAX_BODY", default_value_t = DEFAULT_MAX_BODY)]
    pub max_body: usize,

    #[arg(long, env = "REQLENS_NO_REDACT", default_value_t = false)]
    pub no_redact: bool,

    #[arg(long, env = "REQLENS_TUI", default_value_t = false)]
    pub tui: bool,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    /// Observa pasivamente HTTP sin recibir ni modificar el tráfico de Apache
    Sniff {
        /// Interfaz Linux; "any" observa todas las interfaces
        #[arg(long, env = "REQLENS_INTERFACE", default_value = "any")]
        interface: String,

        /// IP local de Apache; si se omite se acepta cualquier IP local
        #[arg(long, env = "REQLENS_SERVER_IP")]
        server_ip: Option<std::net::Ipv4Addr>,

        /// Puerto HTTP plaintext que observa ReqLens
        #[arg(long, env = "REQLENS_PORT", default_value_t = 80)]
        port: u16,

        #[arg(long, env = "REQLENS_DB_PATH", default_value = DEFAULT_DB_PATH)]
        db_path: PathBuf,

        #[arg(long, env = "REQLENS_MAX_BODY", default_value_t = DEFAULT_MAX_BODY)]
        max_body: usize,

        #[arg(long, env = "REQLENS_NO_REDACT", default_value_t = false)]
        no_redact: bool,

        /// Abre la TUI consumiendo la misma base SQLite del sniffer
        #[arg(long, env = "REQLENS_TUI", default_value_t = false)]
        tui: bool,
    },

    /// Inicia el proxy reverso de observabilidad
    #[command(alias = "proxy")]
    Start {
        #[arg(long, env = "REQLENS_LISTEN", default_value = DEFAULT_LISTEN)]
        listen: String,

        #[arg(long, env = "REQLENS_UPSTREAM", default_value = DEFAULT_UPSTREAM)]
        upstream: String,

        #[arg(long, env = "REQLENS_DB_PATH", default_value = DEFAULT_DB_PATH)]
        db_path: PathBuf,

        #[arg(long, env = "REQLENS_MAX_BODY", default_value_t = DEFAULT_MAX_BODY)]
        max_body: usize,

        #[arg(long, env = "REQLENS_NO_REDACT", default_value_t = false)]
        no_redact: bool,

        #[arg(long, env = "REQLENS_TUI", default_value_t = false)]
        tui: bool,
    },
    /// Abre el dashboard interactivo TUI para explorar peticiones y errores
    Tui {
        #[arg(long, env = "REQLENS_DB_PATH", default_value = DEFAULT_DB_PATH)]
        db_path: PathBuf,

        #[arg(long, env = "REQLENS_LISTEN", default_value = DEFAULT_LISTEN)]
        listen: String,

        #[arg(long, env = "REQLENS_UPSTREAM", default_value = DEFAULT_UPSTREAM)]
        upstream: String,
    },
    /// Consulta el estado del servicio y métricas de la base de datos
    Status {
        #[arg(long, env = "REQLENS_DB_PATH", default_value = DEFAULT_DB_PATH)]
        db_path: PathBuf,
    },
    /// Reinicia el servicio de ReqLens
    Restart,
    /// Detiene y deshabilita el servicio de ReqLens
    Disable,
    /// Instala el binario en /usr/local/bin y opcionalmente configura el servicio systemd
    Install {
        #[arg(long, value_enum, default_value_t = CaptureMode::Sniff)]
        mode: CaptureMode,

        #[arg(long, env = "REQLENS_INTERFACE", default_value = "any")]
        interface: String,

        #[arg(long, env = "REQLENS_SERVER_IP")]
        server_ip: Option<std::net::Ipv4Addr>,

        #[arg(long, env = "REQLENS_PORT", default_value_t = 80)]
        port: u16,

        #[arg(long, env = "REQLENS_LISTEN", default_value = DEFAULT_LISTEN)]
        listen: String,

        #[arg(long, env = "REQLENS_UPSTREAM", default_value = DEFAULT_UPSTREAM)]
        upstream: String,

        #[arg(
            long,
            env = "REQLENS_DB_PATH",
            default_value = "/var/lib/reqlens/reqlens.db"
        )]
        db_path: PathBuf,

        #[arg(long, env = "REQLENS_MAX_BODY", default_value_t = DEFAULT_MAX_BODY)]
        max_body: usize,

        #[arg(long, env = "REQLENS_NO_REDACT", default_value_t = false)]
        no_redact: bool,
    },
    /// Desinstala el servicio y binario de ReqLens del sistema
    Uninstall {
        /// Elimina también la base de datos histórica y el usuario del sistema
        #[arg(long, default_value_t = false)]
        purge: bool,
    },
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub listen_addr: SocketAddr,
    pub upstream_addr: String,
    pub upstream_host: String,
    pub db_path: PathBuf,
    pub max_body: usize,
    pub redact_enabled: bool,
    pub tui_enabled: bool,
}
