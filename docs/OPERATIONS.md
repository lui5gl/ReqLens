# ReqLens — Operación

> Guía operativa: runbook, despliegue, troubleshooting y observabilidad del propio ReqLens.
> Para el uso básico → [README.md](../README.md). Para el diseño → [ARCHITECTURE.md](../ARCHITECTURE.md).

|                          |                  |
| ------------------------ | ---------------- |
| **Versión**              | 0.1.0            |
| **Última actualización** | 2026-08-14       |
| **Audiencia**            | Operadores / SRE |

---

## 1. Runbook operativo

```bash
# Backup en caliente (seguro con WAL — nunca copies el .db a pelo sin incluir -wal/-shm)
sqlite3 data/reqlens.db ".backup 'reqlens.backup.db'"

# Verificación de integridad
sqlite3 data/reqlens.db "PRAGMA integrity_check;"   # completo
sqlite3 data/reqlens.db "PRAGMA quick_check;"        # rápido
```

- **Recuperación de corrupción:** restaurar backup; si no hay, `sqlite3 data/reqlens.db ".recover"` (≥ 3.29) extrae filas legibles a un archivo nuevo.
- **WAL desmedido:** checkpoint automático por defecto (1000 páginas). Si `-wal` crece sin límite, sospechar una sesión `sqlite3` abierta sin commit. Forzar: `PRAGMA wal_checkpoint(TRUNCATE);`.
- **Diagnóstico rápido:** métricas de drops/errores de ingest (`tracing`) + `SELECT COUNT(*)` por ventana temporal.

## 2. Observabilidad del propio ReqLens

- `tracing`: request-id por conexión; errores con contexto, nunca silenciados.
- Métricas (roadmap cercano): requests totales, duración p50/p95, eventos persistidos, eventos descartados, errores de commit — expuestas en `/metrics` (red interna, sin auth).
- Health: `/healthz` refleja el estado del writer (cola, última persistencia exitosa).

## 3. Troubleshooting

| Síntoma                              | Causa probable                                     | Solución                                                      |
| ------------------------------------ | -------------------------------------------------- | ------------------------------------------------------------- |
| `Address already in use` al arrancar | Puerto del listener ocupado                        | Cambia `--listen` o libera el puerto                          |
| `database is locked` al consultar    | Sesión `sqlite3` con transacción abierta           | Cierra la transacción; el writer no bloquea lecturas (WAL)    |
| `-wal` crece sin límite              | Checkpoint no ejecutado                            | `PRAGMA wal_checkpoint(TRUNCATE);`                            |
| No aparecen eventos                  | La persistencia es asíncrona (≤ 250 ms de retardo) | Reintenta; revisa los logs de `tracing` por errores de ingest |
| Bodies como `[BINARY]`               | Content-Type no textual o encoding no soportado    | Esperado por diseño; no es un fallo                           |

## 4. Despliegue

```bash
cargo install --path . --locked
```

Servicio systemd de ejemplo (`/etc/systemd/system/reqlens.service`):

```ini
[Unit]
Description=ReqLens — observabilidad de tráfico HTTP
After=network.target

[Service]
User=reqlens
Group=reqlens
ExecStart=/usr/local/bin/reqlens --listen 0.0.0.0:8080 --upstream http://127.0.0.1:80 --db-path /var/lib/reqlens/reqlens.db
Restart=on-failure
RestartSec=5
NoNewPrivileges=true
ProtectSystem=strict
ReadWritePaths=/var/lib/reqlens
PrivateTmp=true

[Install]
WantedBy=multi-user.target
```

Notas:

- Usuario dedicado `reqlens`, sin root. Crea `/var/lib/reqlens` con permisos `0700` del usuario.
- Los clientes deben apuntar a ReqLens (DNS/LB/edge TLS); Apache permanece intacto en `:80`.
- El reemplazo del binario no pierde datos: la DB vive en el directorio persistente.

## 5. Rendimiento esperado

- Objetivo: ≥ 5 k req/s en hardware commodity (SSD/NVMe, WAL).
- Latencia añadida al tráfico: mínima — la persistencia es asíncrona y no bloquea la respuesta.
- Dimensionamiento y límites de backpressure: [ARCHITECTURE.md §9](../ARCHITECTURE.md#9-rendimiento-y-capacidad).
