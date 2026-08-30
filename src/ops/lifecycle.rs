use crate::error::Result;
use std::fs;
use std::path::Path;
use std::process::Command;

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
            println!(
                "✅ Servicio reqlens deshabilitado. Apache recibirá tráfico directo si se reconfiguró el puerto."
            );
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

    println!("✅ Unidad systemd removida.");

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

    println!(
        "\n✨ Desinstalación concluida. Para remover el binario: sudo rm -f /usr/local/bin/reqlens\n"
    );
    Ok(())
}
