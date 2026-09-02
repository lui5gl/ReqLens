# ReqLens — Manual de Operaciones y Producción (SRE)

> Guía integral para operadores de sistemas y SRE: despliegue en Linux, administración de SQLite en modo WAL, rutinas de mantenimiento, observabilidad y resolución de incidencias.
> Para la visión general del producto → [README.md](../README.md). Para el modelo de datos → [docs/DATA.md](DATA.md).

| Propiedad | Especificación |
| :--- | :--- |
| **Sistema Operativo Objetivo** | Linux (Kernel $\ge$ 5.10 / systemd) |
| **Permisos de Ejecución** | `root` o `CAP_NET_RAW` para abrir AF_PACKET; SQLite debe permanecer restringido |
| **Modo de Base de Datos** | SQLite 3 (WAL mode) en `/var/lib/reqlens` (permisos `0700`) |
| **Audiencia** | Ingenieros de Sistemas, DevOps y SRE |

---

## 1. Despliegue e Instalación

### Opciones de Binarios y Arquitecturas (CI/CD Releases)

El pipeline de GitHub Actions genera automáticamente binarios precompilados en cada release:

| Target de Compilación | Tipo de Enlace | Compatibilidad de Sistema |
| :--- | :--- | :--- |
| **`x86_64-unknown-linux-musl`** | **100% Estático (Zero-Deps)** | **Universal / Máxima Compatibilidad:** Funciona en cualquier distribución Linux (antiguas como CentOS 6/7, Debian 8/9, RHEL o modernas como Alpine, Ubuntu, Fedora) sin importar la versión de `glibc`. |
| **`x86_64-unknown-linux-gnu`** | Dinámico (`glibc`) | Distribuciones Linux modernas estándar de 64 bits. |
| **`aarch64-unknown-linux-musl`** | **100% Estático (Zero-Deps)** | Servidores ARM64 (AWS Graviton, Raspberry Pi 4/5, servidores cloud ARM). |

### Instalación Rápida desde Release Precompilado
```bash
# Descargar el binario estático universal (musl)
TAG="v0.1.7"
curl -sSL "https://github.com/lui5gl/ReqLens/releases/download/${TAG}/reqlens-${TAG}-x86_64-unknown-linux-musl.tar.gz" | sudo tar -xz -C /usr/local/bin --strip-components=1 reqlens-${TAG}-x86_64-unknown-linux-musl/reqlens
sudo chmod +x /usr/local/bin/reqlens
```







### Compilación Local desde Código Fuente
```bash
# Compilación optimizada para producción
cargo install --path . --locked --root /usr/local
```


### Matriz de Parámetros de Configuración

| Parámetro CLI | Variable de Entorno | Valor por Defecto | Descripción Operativa |
| :--- | :--- | :--- | :--- |
| `sniff --interface` | `REQLENS_INTERFACE` | `any` | Interfaz Linux observada sin modificar tráfico |
| `sniff --server-ip` | `REQLENS_SERVER_IP` | cualquier IP | IP local de Apache para evitar tráfico ajeno |
| `sniff --port` | `REQLENS_PORT` | `80` | Puerto HTTP plaintext observado |
| `--listen` | `REQLENS_LISTEN` | `0.0.0.0:8080` | Listener usado solamente por el modo proxy histórico |
| `--upstream` | `REQLENS_UPSTREAM` | `http://127.0.0.1:80` | Apache destino usado solamente por el modo proxy |
| `--db-path` | `REQLENS_DB_PATH` | `./data/reqlens.db` | Ruta absoluta o relativa al archivo SQLite |
| `--max-body` | `REQLENS_MAX_BODY` | `65536` (64 KB) | Límite máximo en bytes de captura por payload |
| `--no-redact` | `REQLENS_NO_REDACT` | `false` | Desactiva redacción automática (**no recomendado**) |
| `web --listen` | `REQLENS_WEB_LISTEN` | `127.0.0.1:8420` | Dirección loopback del dashboard web local |

> 💡 **Principio Fail-Fast:** Precedencia: `CLI flags > Variables de Entorno > Defaults`. Cualquier error de parseo o puerto ocupado aborta inmediatamente el proceso con código de salida $\ne 0$ y traza en `stderr`.

### Modos de Ejecución

> [!IMPORTANT]
> En modo `sniff`, Apache continúa siendo el único listener de `:80`; ReqLens
> abre un socket `AF_PACKET` y recibe copias. No agregues NAT ni muevas Apache.
> En el modo `proxy` histórico sí deben usarse puertos diferentes; la combinación
> `--listen 0.0.0.0:80 --upstream http://127.0.0.1:80` se rechaza como bucle.

1. **Modo pasivo recomendado:**
   ```bash
   sudo reqlens sniff --interface any --server-ip 172.23.25.36 --port 80
   ```
   Apache conserva `:80`; ReqLens requiere root o `CAP_NET_RAW`, pero no recibe,
   redirige ni reenvía conexiones. Solo IPv4 HTTP/1.x plaintext es inspeccionable.

2. **Modo proxy histórico:**
   ```bash
   reqlens --listen 0.0.0.0:8080 --upstream http://127.0.0.1:80
   ```
   Ideal para entornos desatendidos, servicios systemd o contenedores. Las trazas de observabilidad se emiten en formato estructurado `tracing`.

3. **Modo Dashboard Web:**
   ```bash
   reqlens web --db-path /var/lib/reqlens/reqlens.db
   ```
   Abre un dashboard local de solo lectura en `http://127.0.0.1:8420`, con actualización automática, filtros, búsqueda, ordenamiento e inspección detallada de cabeceras y payloads. Para usar otro puerto local, añade `--listen 127.0.0.1:PUERTO`.

4. **Captura pasiva y arranque automático:**
   ```bash
   # Apache permanece sin cambios en :80.
   sudo reqlens install \
     --mode sniff \
     --interface any \
     --server-ip 172.23.25.36 \
     --port 80 \
     --db-path /var/lib/reqlens/reqlens.db
   ```
   `reqlens install` registra el servicio en systemd o SysV, lo inicia en ese
   momento y lo habilita para los siguientes arranques. No es necesario usar
   `nohup`. El dashboard se abre después con `reqlens web --db-path
   /var/lib/reqlens/reqlens.db`; ese subcomando consulta el servicio existente
   y no ocupa el puerto HTTP observado.


---

## 2. Configuración del Servicio Systemd (Hardened)

Crea el archivo de servicio en `/etc/systemd/system/reqlens.service`:

```ini
[Unit]
Description=ReqLens — Observabilidad HTTP Pasiva
Documentation=https://github.com/tu-org/reqlens
After=network.target network-online.target
Wants=network-online.target

[Service]
Type=simple
User=root
ExecStart=/usr/local/bin/reqlens sniff \
    --interface any \
    --server-ip 172.23.25.36 \
    --port 80 \
    --db-path /var/lib/reqlens/reqlens.db \
    --max-body 65536
Restart=on-failure
RestartSec=5s
LimitNOFILE=65535

# Sandbox y Aislamiento de Seguridad (Hardening)
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
PrivateTmp=true
PrivateDevices=true
ProtectKernelTunables=true
ProtectControlGroups=true
ReadWritePaths=/var/lib/reqlens

[Install]
WantedBy=multi-user.target
```

### Aprovisionamiento del Entorno y Arranque
```bash
# Crear usuario de sistema sin login
sudo useradd -r -s /usr/sbin/nologin reqlens

# Crear directorio de datos con permisos estrictos
sudo mkdir -p /var/lib/reqlens
sudo chown -R reqlens:reqlens /var/lib/reqlens
sudo chmod 0700 /var/lib/reqlens

# Recargar systemd y arrancar el servicio
sudo systemctl daemon-reload
sudo systemctl enable --now reqlens
```

---

## 3. Comandos CLI de Ciclo de Vida (Gestión Nativa)

ReqLens incluye subcomandos nativos en la CLI para consultar estado, reiniciar, deshabilitar y desinstalar sin memorizar comandos complejos de bajo nivel:

### A. Consultar la Versión
```bash
reqlens --version
# o de forma abreviada:
reqlens -V
```

### B. Consultar el Estado del Sistema y Base de Datos
```bash
reqlens status
# con ruta personalizada de DB:
reqlens status --db-path /var/lib/reqlens/reqlens.db
```
Muestra el estado del servicio systemd (Activo, Inactivo, Fallido), ubicación física del archivo SQLite, tamaño en disco, volumen de peticiones capturadas, conteo de errores y latencia media.

### C. Reiniciar el Servicio
```bash
sudo reqlens restart
```
Ejecuta el reinicio seguro asegurando que el proceso anterior realice el drenado (*graceful drain*) de la cola MPSC a SQLite antes de levantarse nuevamente.

### D. Deshabilitar y Detener el Servicio (Bypass)
```bash
sudo reqlens disable
```
Detiene el servicio en ejecución y lo deshabilita para que no inicie automáticamente con el sistema. Si se reconfigura el puerto, Apache atenderá el tráfico directo sin intervención de ReqLens.

### E. Desinstalación Completa del Sistema
```bash
# Desinstalación estándar (conservando base de datos histórica):
sudo reqlens uninstall

# Desinstalación y purga completa (elimina unidad systemd, usuario y base de datos):
sudo reqlens uninstall --purge
```

---

## 4. Administración y Mantenimiento de SQLite (WAL)

ReqLens opera SQLite en modo **Write-Ahead Logging (WAL)**. En producción, el directorio de datos contendrá tres archivos: `reqlens.db`, `reqlens.db-wal` y `reqlens.db-shm`.

### Respaldo en Caliente (Online Hot-Backup)
> ⚠️ **NUNCA** utilices comandos como `cp` o `rsync` directamente sobre `reqlens.db` mientras el proxy esté en ejecución, ya que generará copias corruptas si hay escrituras activas en el archivo WAL.

```bash
# Generar respaldo consistente en caliente sin detener el tráfico
sqlite3 /var/lib/reqlens/reqlens.db ".backup '/var/backups/reqlens_$(date +%Y%m%d_%H%M%S).db'"
```

### Comprobación de Integridad Periódica
```bash
# Verificación rápida (óptima para healthchecks y cronjobs frecuentes)
sqlite3 /var/lib/reqlens/reqlens.db "PRAGMA quick_check;"

# Verificación estructural exhaustiva
sqlite3 /var/lib/reqlens/reqlens.db "PRAGMA integrity_check;"
```

### Control y Truncado del Archivo WAL
Por defecto, SQLite ejecuta checkpoints automáticos cada 1,000 páginas. Si el archivo `-wal` crece continuamente de forma anómala (habitualmente por consultas analíticas externas reteniendo transacciones):

```bash
# Forzar checkpoint y reducir el archivo WAL a cero
sqlite3 /var/lib/reqlens/reqlens.db "PRAGMA wal_checkpoint(TRUNCATE);"
```

### Procedimiento de Recuperación de Desastres
Si una desconexión abrupta del host corrompe el archivo principal:
```bash
# Extraer filas recuperables a una base de datos nueva
sqlite3 /var/lib/reqlens/reqlens.db ".recover" | sqlite3 /var/lib/reqlens/reqlens_recovered.db
```

---

## 5. Matriz de Resolución de Problemas (Troubleshooting)

| Síntoma Observado | Causa Raíz Probable | Solución Operativa |
| :--- | :--- | :--- |
| `Address already in use` al arrancar | El puerto (`--listen`) está ocupado por otro proceso o instancia previa. | Identificar el proceso en conflicto con `lsof -i :8080` y liberar el puerto o modificar el flag `--listen`. |
| `proxy loop detected` o CPU elevada con listener `:80` | Se inició el modo proxy antiguo en vez del modo pasivo. | Detenerlo y usar `reqlens sniff --interface any --server-ip IP_DEL_SERVIDOR --port 80`. Apache permanece en `:80`. |
| `passive capture needs root or CAP_NET_RAW` | El kernel rechazó la apertura de AF_PACKET. | Ejecutar como root o aplicar `setcap cap_net_raw=eip /usr/local/bin/reqlens`. |
| No se capturan peticiones HTTPS | TLS cifra el protocolo HTTP antes de que AF_PACKET entregue la copia. | El modo pasivo inspecciona solamente HTTP plaintext; use instrumentación tras la terminación TLS si necesita bodies HTTPS. |
| El dashboard no abre | Ya hay otra instancia usando `127.0.0.1:8420` o no hay navegador gráfico disponible. | Consulta la URL mostrada por `reqlens web`, libera el puerto o elige otro con `--listen 127.0.0.1:8421`. |
| `database is locked` al ejecutar SQL | Una sesión externa mantiene una transacción `BEGIN EXCLUSIVE` sin cerrar. | Identificar y terminar la sesión analítica interactiva colgada. |
| El archivo `-wal` no disminuye de tamaño | Checkpoints bloqueados por lectores concurrentes de larga duración. | Ejecutar `PRAGMA wal_checkpoint(TRUNCATE);` una vez concluidas las consultas pesadas. |
| No aparecen peticiones recientes | Persistencia asíncrona por lotes (espera hasta 250 ms) o cola MPSC saturada. | Esperar 250 ms o inspeccionar trazas de `tracing` para descartar eventos descartados por saturación. |
| Bodies aparecen con `[BINARY]` | El encabezado `Content-Type` no es textual o los bytes no son UTF-8 válidos. | Comportamiento normal por diseño para salvaguardar la integridad de la base. |
| Peticiones devuelven HTTP 502 Bad Gateway | Apache está apagado o no responde en la URL `--upstream`. | Verificar el estado de Apache con `systemctl status apache2` o `curl -I http://127.0.0.1:80`. |

---

## 6. Observabilidad del Propio Proceso

- **Trazas Estructuradas (`tracing`):** Cada conexión entrante genera un `request_id` único correlacionado. Los errores de serialización o conexión al upstream se registran con contexto sin exponer datos sensibles.
- **Endpoints de Diagnóstico (Roadmap):**
  - `/healthz`: Estado operativo del pipeline (salud del worker de SQLite y estado de la cola).
  - `/metrics`: Métricas de peticiones totales, latencia p50/p95, eventos descartados y errores de disco en formato compatible con Prometheus.
