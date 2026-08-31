use crate::config::cli::CaptureMode;
use crate::config::{parse_upstream, validate_proxy_endpoints};
use crate::error::Result;
use std::fs;
use std::net::Ipv4Addr;
use std::path::Path;
use std::process::Command;

pub struct InstallConfig<'a> {
    pub mode: CaptureMode,
    pub interface: &'a str,
    pub server_ip: Option<Ipv4Addr>,
    pub port: u16,
    pub listen: &'a str,
    pub upstream: &'a str,
    pub db_path: &'a Path,
    pub max_body: usize,
    pub no_redact: bool,
}

pub fn auto_deploy_to_bin() {
    let Ok(current_exe) = std::env::current_exe() else {
        return;
    };
    let target_bin = Path::new("/usr/local/bin/reqlens");

    if current_exe != target_bin && fs::copy(&current_exe, target_bin).is_ok() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(mut perms) = fs::metadata(target_bin).map(|m| m.permissions()) {
                perms.set_mode(0o755);
                let _ = fs::set_permissions(target_bin, perms);
            }
        }
        println!("💡 ReqLens se ha copiado automáticamente a /usr/local/bin/reqlens");
        println!("   (Ya disponible globalmente desde cualquier directorio como 'reqlens')\n");
    }
}

pub fn install_service(config: InstallConfig<'_>) -> Result<()> {
    println!("📦 Instalando ReqLens en el sistema...");
    let InstallConfig {
        mode,
        interface,
        server_ip,
        port,
        listen,
        upstream,
        db_path,
        max_body,
        no_redact,
    } = config;

    let listen_addr = listen.parse().map_err(|error| {
        crate::error::ReqLensError::Config(format!("Invalid listen address '{listen}': {error}"))
    })?;
    let (upstream_addr, _) = parse_upstream(upstream)?;
    validate_proxy_endpoints(listen_addr, &upstream_addr)?;

    let InstallConfig {
        mode,
        interface,
        server_ip,
        port,
        listen,
        upstream,
        db_path,
        max_body,
        no_redact,
    } = config;

    if mode == CaptureMode::Proxy {
        let listen_addr = listen.parse().map_err(|error| {
            crate::error::ReqLensError::Config(format!(
                "Invalid listen address '{listen}': {error}"
            ))
        })?;
        let (upstream_addr, _) = parse_upstream(upstream)?;
        validate_proxy_endpoints(listen_addr, &upstream_addr)?;
    }

    if config.mode == CaptureMode::Proxy {
        let listen_addr = config.listen.parse().map_err(|error| {
            crate::error::ReqLensError::Config(format!(
                "Invalid listen address '{}': {error}",
                config.listen
            ))
        })?;
        let (upstream_addr, _) = parse_upstream(config.upstream)?;
        validate_proxy_endpoints(listen_addr, &upstream_addr)?;
    }

    let current_exe = std::env::current_exe()?;
    let target_bin = Path::new("/usr/local/bin/reqlens");

    if current_exe != target_bin {
        fs::copy(&current_exe, target_bin)?;
        println!("✅ Binario copiado a /usr/local/bin/reqlens");
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(target_bin)?.permissions();
        perms.set_mode(0o755);
        let _ = fs::set_permissions(target_bin, perms);
    }

    if let Some(parent) = config.db_path.parent()
        && !parent.exists()
    {
        let _ = fs::create_dir_all(parent);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(parent)?.permissions();
            perms.set_mode(0o700);
            let _ = fs::set_permissions(parent, perms);
        }
    }

    let _ = Command::new("useradd")
        .args(["-r", "-s", "/usr/sbin/nologin", "reqlens"])
        .status();

    let redact_flag = if config.no_redact { " --no-redact" } else { "" };
    let server_ip_flag = config
        .server_ip
        .map(|ip| format!(" --server-ip {ip}"))
        .unwrap_or_default();
    let exec_args = match config.mode {
        CaptureMode::Sniff => format!(
            "sniff --interface {} --port {}{} --db-path {}{} --max-body {}",
            config.interface,
            config.port,
            server_ip_flag,
            config.db_path.display(),
            redact_flag,
            config.max_body
        ),
        CaptureMode::Proxy => format!(
            "--listen {} --upstream {} --db-path {}{} --max-body {}",
            config.listen,
            config.upstream,
            config.db_path.display(),
            redact_flag,
            config.max_body
        ),
    };
    let service_content = format!(
        r#"[Unit]
Description=ReqLens — HTTP Observability
After=network.target network-online.target
Wants=network-online.target

[Service]
Type=simple
User=root
ExecStart=/usr/local/bin/reqlens {}
Restart=on-failure
RestartSec=5s
LimitNOFILE=65535

[Install]
WantedBy=multi-user.target
"#,
        exec_args
    );

    let service_path = Path::new("/etc/systemd/system/reqlens.service");
    let has_systemd = Path::new("/run/systemd/system").exists()
        || Command::new("systemctl").arg("--version").output().is_ok();

    if has_systemd && fs::write(service_path, service_content).is_ok() {
        let _ = Command::new("systemctl").arg("daemon-reload").status();
        let _ = Command::new("systemctl")
            .args(["enable", "--now", "reqlens"])
            .status();
        println!(
            "✅ Servicio systemd registrado e iniciado automáticamente al arranque del sistema."
        );
    } else {
        // Soporte nativo para SysV Init (CentOS 5, CentOS 6, RedHat legacy)
        let init_script_content = format!(
            r#"#!/bin/bash
# chkconfig: 2345 90 10
# description: ReqLens HTTP Observability

PIDFILE=/var/run/reqlens.pid
BIN=/usr/local/bin/reqlens
ARGS="{}"

case "$1" in
    start)
        echo -n "Iniciando reqlens: "
        nohup $BIN $ARGS > /var/log/reqlens.log 2>&1 &
        echo $! > $PIDFILE
        echo "OK"
        ;;
    stop)
        echo -n "Deteniendo reqlens: "
        if [ -f $PIDFILE ]; then
            kill $(cat $PIDFILE) 2>/dev/null
            rm -f $PIDFILE
        else
            pkill -f "$BIN" 2>/dev/null
        fi
        echo "OK"
        ;;
    restart)
        $0 stop
        sleep 1
        $0 start
        ;;
    status)
        if [ -f $PIDFILE ] && kill -0 $(cat $PIDFILE) 2>/dev/null; then
            echo "reqlens está en ejecución (PID $(cat $PIDFILE))"
        else
            echo "reqlens está detenido"
        fi
        ;;
    *)
        echo "Uso: $0 {{start|stop|restart|status}}"
        exit 1
        ;;
esac
exit 0
"#,
            exec_args
        );

        let init_path = Path::new("/etc/init.d/reqlens");
        if fs::write(init_path, init_script_content).is_ok() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(mut perms) = fs::metadata(init_path).map(|m| m.permissions()) {
                    perms.set_mode(0o755);
                    let _ = fs::set_permissions(init_path, perms);
                }
            }
            let _ = Command::new("chkconfig")
                .args(["--add", "reqlens"])
                .status();
            let _ = Command::new("chkconfig").args(["reqlens", "on"]).status();
            let _ = Command::new("service")
                .args(["reqlens", "restart"])
                .status();
            println!(
                "✅ Servicio SysV init (/etc/init.d/reqlens) registrado e iniciado automáticamente con chkconfig."
            );
        } else {
            println!("ℹ️  Binario instalado globalmente en /usr/local/bin/reqlens.");
        }
    }

    println!(
        "\n🎉 Instalación completada. Ahora puedes ejecutar 'reqlens' o 'reqlens tui' desde cualquier directorio.\n"
    );
    Ok(())
}

pub fn restart_service() -> Result<()> {
    println!("🔄 Reiniciando servicio ReqLens...");
    if Command::new("systemctl")
        .args(["restart", "reqlens"])
        .status()
        .is_ok_and(|s| s.success())
    {
        println!("✅ Servicio reqlens reiniciado correctamente (systemd).");
        return Ok(());
    }
    if Command::new("service")
        .args(["reqlens", "restart"])
        .status()
        .is_ok_and(|s| s.success())
    {
        println!("✅ Servicio reqlens reiniciado correctamente (SysV init).");
        return Ok(());
    }
    println!("⚠️ Asegúrate de ejecutar con privilegios administrativos (sudo/root).");
    Ok(())
}

pub fn disable_service() -> Result<()> {
    println!("🛑 Deteniendo y deshabilitando servicio ReqLens...");
    let _ = Command::new("systemctl").args(["stop", "reqlens"]).status();
    let _ = Command::new("systemctl")
        .args(["disable", "reqlens"])
        .status();
    let _ = Command::new("service").args(["reqlens", "stop"]).status();
    let _ = Command::new("chkconfig").args(["reqlens", "off"]).status();
    println!("✅ Servicio reqlens detenido y deshabilitado del inicio del sistema.");
    Ok(())
}

pub fn uninstall_service(purge: bool) -> Result<()> {
    println!("🗑️  Desinstalando ReqLens del sistema...");

    let _ = Command::new("systemctl").args(["stop", "reqlens"]).status();
    let _ = Command::new("systemctl")
        .args(["disable", "reqlens"])
        .status();
    let _ = fs::remove_file("/etc/systemd/system/reqlens.service");
    let _ = Command::new("systemctl").arg("daemon-reload").status();
    let _ = Command::new("systemctl").arg("reset-failed").status();

    let _ = Command::new("service").args(["reqlens", "stop"]).status();
    let _ = Command::new("chkconfig")
        .args(["--del", "reqlens"])
        .status();
    let _ = fs::remove_file("/etc/init.d/reqlens");

    let _ = fs::remove_file("/usr/local/bin/reqlens");
    let _ = fs::remove_file("/usr/bin/reqlens");

    println!("✅ Servicio, binarios y configuraciones removidas.");

    if purge {
        let db_dir = Path::new("/var/lib/reqlens");
        if db_dir.exists() {
            let _ = fs::remove_dir_all(db_dir);
            println!("🧹 Datos purgados en /var/lib/reqlens");
        }
        let _ = Command::new("userdel").arg("reqlens").status();
        println!("👤 Usuario de sistema 'reqlens' eliminado.");
    } else {
        println!("ℹ️  Base de datos conservada en /var/lib/reqlens (usa --purge para eliminarla).");
    }

    println!("\n✨ Desinstalación concluida limpiamente.\n");
    Ok(())
}
