use crate::error::Result;
use crate::tui::repo::{fetch_stats, open_readonly_conn};
use std::fs;
use std::path::Path;
use std::process::Command;

pub fn print_status(db_path: &Path) -> Result<()> {
    println!("\n🔍 ReqLens — Estado del Sistema");
    println!("──────────────────────────────────────────");

    check_systemd_status();
    inspect_database(db_path)?;

    println!("──────────────────────────────────────────\n");
    Ok(())
}

fn check_systemd_status() {
    if let Ok(out) = Command::new("systemctl")
        .args(["is-active", "reqlens"])
        .output()
    {
        let status = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let color_status = match status.as_str() {
            "active" => "🟢 Activo (Running - systemd)",
            "inactive" => "⚪ Inactivo (Stopped)",
            "failed" => "🔴 Fallido (Failed)",
            _ => "⚪ Desconocido / No instalado",
        };
        println!("• Servicio:         {}", color_status);
        return;
    }

    if Path::new("/etc/init.d/reqlens").exists() {
        let is_running = Command::new("service")
            .args(["reqlens", "status"])
            .output()
            .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains("ejecución"));
        if is_running {
            println!("• Servicio:         🟢 Activo (Running - SysV init /etc/init.d/reqlens)");
        } else {
            println!("• Servicio:         ⚪ Inactivo (SysV init /etc/init.d/reqlens)");
        }
        return;
    }

    println!("• Servicio:         ⚪ No instalado como servicio");
}

fn inspect_database(db_path: &Path) -> Result<()> {
    println!("• Base de Datos:    {:?}", db_path);

    if !db_path.exists() {
        println!("  Estado:           No inicializada (se creará al recibir tráfico)");
        return Ok(());
    }

    if let Ok(metadata) = fs::metadata(db_path) {
        let size_kb = metadata.len() as f64 / 1024.0;
        println!("  Tamaño en disco:  {:.2} KB", size_kb);
    }

    if let Some(conn) = open_readonly_conn(db_path)?
        && let Ok(stats) = fetch_stats(&conn)
    {
        println!("  Total Peticiones: {}", stats.total_requests);
        println!("  Total Errores:    {}", stats.error_count);
        println!("  Latencia Media:   {:.2} ms", stats.avg_latency_ms);
    }

    Ok(())
}
