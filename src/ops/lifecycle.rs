use crate::error::Result;
use std::fs;
use std::path::Path;
use std::process::Command;

pub fn install_service(
    listen: &str,
    upstream: &str,
    db_path: &Path,
    max_body: usize,
    no_redact: bool,
) -> Result<()> {
    println!("📦 Instalando ReqLens en el sistema...");

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

    if let Some(parent) = db_path.parent()
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

    let redact_flag = if no_redact { " --no-redact" } else { "" };
    let service_content = format!(
        r#"[Unit]
Description=ReqLens — HTTP Observability Reverse Proxy
After=network.target network-online.target
Wants=network-online.target

[Service]
Type=simple
User=root
ExecStart=/usr/local/bin/reqlens --listen {} --upstream {} --db-path {}{} --max-body {}
Restart=on-failure
RestartSec=5s
LimitNOFILE=65535

[Install]
WantedBy=multi-user.target
"#,
        listen,
        upstream,
        db_path.display(),
        redact_flag,
        max_body
    );

    let service_path = Path::new("/etc/systemd/system/reqlens.service");
    let has_systemd = Path::new("/run/systemd/system").exists()
        || Command::new("systemctl").arg("--version").output().is_ok();

    if has_systemd && fs::write(service_path, service_content).is_ok() {
        let _ = Command::new("systemctl").arg("daemon-reload").status();
        let _ = Command::new("systemctl")
            .args(["enable", "--now", "reqlens"])
            .status();
        println!("✅ Servicio systemd registrado e iniciado automáticamente.");
    } else {
        println!("ℹ️  Binario instalado globalmente en /usr/local/bin/reqlens.");
    }

    println!(
        "\n🎉 Instalación completada. Ahora puedes ejecutar 'reqlens' o 'reqlens tui' desde cualquier directorio.\n"
    );
    Ok(())
}

pub fn restart_service() -> Result<()> {
    println!("🔄 Reiniciando servicio ReqLens...");
    let status = Command::new("systemctl")
        .args(["restart", "reqlens"])
        .status();

    match status {
        Ok(s) if s.success() => {
            println!("✅ Servicio reqlens reiniciado correctamente.");
        }
        Ok(_) | Err(_) => {
            println!("⚠️ No se pudo ejecutar 'systemctl restart reqlens' directamente.");
            println!("   Asegúrate de ejecutar con privilegios: sudo systemctl restart reqlens");
        }
    }
    Ok(())
}

pub fn disable_service() -> Result<()> {
    println!("🛑 Deteniendo y deshabilitando servicio ReqLens...");
    let _ = Command::new("systemctl").args(["stop", "reqlens"]).status();
    let status = Command::new("systemctl")
        .args(["disable", "reqlens"])
        .status();

    match status {
        Ok(s) if s.success() => {
            println!("✅ Servicio reqlens deshabilitado.");
        }
        _ => {
            println!(
                "⚠️ Ejecuta con privilegios administrativos: sudo systemctl disable --now reqlens"
            );
        }
    }
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

    let _ = fs::remove_file("/usr/local/bin/reqlens");
    let _ = fs::remove_file("/usr/bin/reqlens");

    println!("✅ Unidad systemd y binario removidos de /usr/local/bin/reqlens.");

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
