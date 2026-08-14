# ReqLens

> Observabilidad de tráfico HTTP hacia Apache: **quién** envía, **a qué endpoint**, **qué payload**, y **qué respondió Apache** (status + body). Persistido en SQLite, consultable con SQL.

**Estado:** v0.1.0 (MVP). La CLI y el esquema de la DB pueden cambiar antes de 1.0.

## Tabla de contenidos

- [Qué es y por qué existe](#qué-es-y-por-qué-existe)
- [Requisitos](#requisitos)
- [Quick start](#quick-start)
- [Configuración](#configuración)
- [Modelo de datos](#modelo-de-datos)
- [Garantías y límites](#garantías-y-límites-léelo-antes-de-operarlo)
- [Seguridad](#seguridad)
- [Operación](#operación)
- [Desarrollo](#desarrollo)
- [Roadmap](#roadmap)
- [Licencia](#licencia)
- [Documentación técnica](#documentación-técnica)

## Qué es y por qué existe

El `access.log` de Apache solo registra método, ruta, status y tamaño. No responde las preguntas que importan: ¿qué envió el cliente en el body?, ¿qué devolvió el backend?, ¿quién lo hizo?.

ReqLens es un **proxy reverso** que se interpone entre los clientes y Apache para capturar la información que el log nativo no tiene — el cuerpo de request y de response.

```
Cliente ──▶ ReqLens (:8080) ──▶ Apache (:80) ──▶ App Backend
                │
                └──▶ SQLite (data/reqlens.db, WAL)
```

- Apache **no se modifica**: ni config, ni módulos, ni reinicios.
- ReqLens es transparente: reenvía request y response íntegros.
- Si ReqLens falla, **el tráfico sigue** (fail-open) — la observabilidad nunca derriba el servicio.

## Requisitos

- Rust toolchain ≥ 1.85 (edition 2024). Verificado con Cargo 1.97.
- `sqlite3` CLI para consultar la base (opcional pero recomendado).
- SO: Linux (objetivo de producción); macOS funciona; Windows sin soporte formal.
- Un Apache accesible como upstream HTTP/1.1.

## Quick start

```bash
cargo build --release
cargo run --release -- --listen 0.0.0.0:8080 --upstream http://127.0.0.1:80
```

Smoke test de punta a punta (en otra terminal):

```bash
curl -s -X POST http://127.0.0.1:8080/api/login \
  -H "Content-Type: application/json" \
  -d '{"username":"jdoe","password":"secreto"}'
```

Verifica la captura — la contraseña debe aparecer como `[REDACTED]`:

```bash
sqlite3 data/reqlens.db "SELECT method, path, resp_status, req_body FROM requests ORDER BY timestamp DESC LIMIT 1;"
```

## Configuración

| Opción        | Variable de entorno | Default               | Descripción                                 |
| ------------- | ------------------- | --------------------- | ------------------------------------------- |
| `--listen`    | `REQLENS_LISTEN`    | `0.0.0.0:8080`        | Dirección del listener                      |
| `--upstream`  | `REQLENS_UPSTREAM`  | `http://127.0.0.1:80` | Apache al que reenviar                      |
| `--db-path`   | `REQLENS_DB_PATH`   | `./data/reqlens.db`   | Ruta del archivo SQLite                     |
| `--max-body`  | `REQLENS_MAX_BODY`  | `65536` (64 KB)       | Límite de captura por body                  |
| `--no-redact` | `REQLENS_NO_REDACT` | `false`               | Desactiva la redacción (**no recomendado**) |

**Precedencia:** argumento CLI > variable de entorno > default. No hay archivo de configuración (YAGNI).

**Validación fail-fast:** config inválida, puerto ocupado o directorio sin permisos → el proceso no arranca y el error se imprime con contexto. Un sistema a medias no sirve.

**Regla operativa:** si desactivas la redacción, el `warn!` de startup es la prueba de que asumiste el riesgo conscientemente.

## Modelo de datos

Tabla única `requests` (desnormalizada a propósito; ver [ARCHITECTURE.md § Base de datos](./ARCHITECTURE.md#base-de-datos)):

| Columna        | Tipo        | Contenido                                                  |
| -------------- | ----------- | ---------------------------------------------------------- |
| `id`           | INTEGER PK  | Autoincremental                                            |
| `timestamp`    | TEXT        | UTC ISO-8601 con milisegundos                              |
| `duration_ms`  | INTEGER     | Latencia total del ciclo proxy                             |
| `client_ip`    | TEXT        | IP del cliente (último hop de `X-Forwarded-For`, o socket) |
| `client_ua`    | TEXT        | User-Agent                                                 |
| `method`       | TEXT        | Método HTTP                                                |
| `path`         | TEXT        | Endpoint (crudo)                                           |
| `query`        | TEXT        | Query string (crudo, sin decodificar)                      |
| `req_headers`  | TEXT (JSON) | Headers permitidos                                         |
| `req_body`     | TEXT        | Body del request                                           |
| `resp_status`  | INTEGER     | Status devuelto por Apache                                 |
| `resp_headers` | TEXT (JSON) | Headers permitidos                                         |
| `resp_body`    | TEXT        | Body de la respuesta                                       |

### Consultas de ejemplo

```sql
-- Últimos 50 requests
SELECT timestamp, method, path, resp_status, duration_ms
FROM requests ORDER BY timestamp DESC LIMIT 50;

-- Requests a /api/login en las últimas 24h
SELECT * FROM requests
WHERE path = '/api/login' AND timestamp >= datetime('now', '-1 day');

-- Top endpoints con status 5xx
SELECT path, COUNT(*) AS errores FROM requests
WHERE resp_status >= 500 GROUP BY path ORDER BY errores DESC;

-- Actividad por cliente
SELECT client_ip, COUNT(*) AS requests FROM requests
GROUP BY client_ip ORDER BY requests DESC;
```

### Reglas de contenido

- Solo headers de una allowlist; `authorization`, `cookie`, `set-cookie` **nunca** se capturan.
- Bodies binarios → `[BINARY]`; comprimidos (`gzip`/`br`/`deflate`) → `[COMPRESSED]` (no se descomprimen).
- Claves sensibles en bodies JSON → `[REDACTED]`.
- Bodies que exceden `--max-body` → prefijo + `[TRUNCATED]`.

## Garantías y límites (léelo antes de operarlo)

ReqLens es **at-most-once**: en condiciones normales persiste todo, pero hay ventanas de pérdida acotadas y explícitas.

| Situación                                             | Qué pasa                                                                                |
| ----------------------------------------------------- | --------------------------------------------------------------------------------------- |
| Crash del proceso                                     | Se pierde ≤ 100 eventos o 250 ms del último batch no commiteado                         |
| Pico de tráfico sostenido (> ~1 k req/s con DB lenta) | Se descartan eventos (cola llena); se registran y cuentan — el tráfico nunca se bloquea |
| Disco lleno                                           | El batch se revierte completo y se registra el error; ReqLens sigue sirviendo           |
| Shutdown limpio (SIGTERM/SIGINT)                      | Drain completo: no se pierde nada                                                       |

Si tu caso de uso exige pérdida cero, esto no es la herramienta — está diseñado para telemetría, no para auditoría legal.

## Seguridad

- Redacción **default-on** (fail-safe): la desactivación requiere flag explícito.
- Allowlist de headers; credenciales y cookies excluidas siempre.
- Archivo `.db` con permisos `0600`.
- Captura con límite de tamaño: memoria acotada incluso con bodies gigantes.
- **Límite conocido:** la redacción cubre una lista configurable de claves + regex de respaldo. Si un campo sensible no está en la lista, se capturará — ajusta la configuración antes de exponer endpoints con datos críticos.

Modelo de amenazas completo → [ARCHITECTURE.md § Seguridad](./ARCHITECTURE.md#seguridad).

## Operación

```bash
# Backup en caliente (seguro con WAL — no copies el .db a pelo)
sqlite3 data/reqlens.db ".backup 'reqlens.backup.db'"

# Verificación de integridad
sqlite3 data/reqlens.db "PRAGMA integrity_check;"
```

**Crecimiento estimado:** ≈ 500 B + bodies por fila. A 1 k req/s con bodies de 1 KB promedio → ~1.5 GB/día (ver [ARCHITECTURE.md § Base de datos](./ARCHITECTURE.md#base-de-datos)).

Runbook completo, troubleshooting, despliegue y rendimiento → [ARCHITECTURE.md § Operación](./ARCHITECTURE.md#operación).

## Desarrollo

```bash
cargo test                 # unit + integración + E2E con upstream mock
cargo clippy -- -D warnings
cargo fmt --check
```

El E2E usa un upstream mock en memoria, nunca Apache real. Estrategia completa en [ARCHITECTURE.md § Testing](./ARCHITECTURE.md#testing).

## Roadmap

- Particionado por fecha + retención automática (control de crecimiento).
- Endpoint `/metrics` y `/healthz` para el propio ReqLens.
- Exportación a NDJSON/CSV para pipelines externos.
- Filtros de captura por path/método (ignorar healthchecks).
- Modo offline: analizador de `access.log` enriquecido.

## Licencia

[MIT](./LICENSE) © 2026.

## Documentación técnica

- [ARCHITECTURE.md](./ARCHITECTURE.md) — cómo está construido y por qué: decisiones de diseño, flujo, base de datos, seguridad y operación.
