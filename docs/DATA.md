# ReqLens — Modelo de datos

> Cómo se almacena y se consulta el tráfico capturado. Para el diseño del sistema → [ARCHITECTURE.md](../ARCHITECTURE.md).

|                          |            |
| ------------------------ | ---------- |
| **Versión**              | 0.1.0      |
| **Última actualización** | 2026-08-14 |

---

## 1. Decisiones de diseño del schema

- **Tabla única `requests`, desnormalizada.** Los headers viven como JSON; nunca se consulta por un header individual como clave → normalizar en tablas hijas multiplicaría joins sin beneficio.
- **`timestamp` como TEXT ISO-8601 UTC** con milisegundos: lexicográficamente ordenable e indexable, sin conversión de zona horaria en la app.
- **`query` crudo, sin decodificar**: `+` y `%XX` deben preservarse para fidelidad; la decodificación es responsabilidad del consumidor.
- **`method` como TEXT validado contra constante** (no enum en SQL): evita migraciones si aparece un método nuevo.

## 2. Schema físico

| Columna        | Tipo                     | Rationale                                                                                             |
| -------------- | ------------------------ | ----------------------------------------------------------------------------------------------------- |
| `id`           | INTEGER PK AUTOINCREMENT | Clave de correlación; AUTOINCREMENT evita reuso de ids tras delete                                    |
| `timestamp`    | TEXT                     | Ordenable, indexable, sin ambigüedad TZ                                                               |
| `duration_ms`  | INTEGER                  | Métrica de rendimiento; entero, sin float                                                             |
| `client_ip`    | TEXT                     | Último hop de `X-Forwarded-For`; socket como fallback (spoofeable — ver [SECURITY.md](./SECURITY.md)) |
| `client_ua`    | TEXT                     | Diagnóstico                                                                                           |
| `method`       | TEXT                     | Validado contra constante                                                                             |
| `path`         | TEXT                     | Crudo; el parsing de segmentos es del consumidor                                                      |
| `query`        | TEXT                     | Crudo, sin decodificar                                                                                |
| `req_headers`  | TEXT (JSON)              | Allowlist (ver [SECURITY.md](./SECURITY.md))                                                          |
| `req_body`     | TEXT                     | Truncado + `[TRUNCATED]` según `--max-body`                                                           |
| `resp_status`  | INTEGER                  | Permite agregados por rango (`>= 500`)                                                                |
| `resp_headers` | TEXT (JSON)              | Allowlist (ver [SECURITY.md](./SECURITY.md))                                                          |
| `resp_body`    | TEXT                     | Ídem req_body                                                                                         |

## 3. Índices y costo de escritura

- `(timestamp)` — consultas por ventana temporal (caso dominante).
- `(method, path)` — top endpoints.
- `(resp_status)` — agregados por status.
- `(client_ip)` — forense por cliente.

Cada índice incremental amplifica el INSERT (~4× con estos cuatro). Aceptable por debajo de ~5 k req/s (ver [ARCHITECTURE.md §9](../ARCHITECTURE.md#9-rendimiento-y-capacidad)); **no se añade ningún índice más sin una consulta real que lo justifique** (YAGNI).

## 4. DDL idempotente

`CREATE TABLE IF NOT EXISTS` + `CREATE INDEX IF NOT EXISTS` en cada arranque. Migraciones futuras: tabla nueva + backfill, nunca `ALTER` destructivo.

## 5. Consultas de ejemplo

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

## 6. Crecimiento del archivo

≈ 500 B + bodies por fila. A 1 k req/s con bodies de 1 KB promedio → **~1.5 GB/día**. Este número decide cuándo activar particionado/retención (roadmap del proyecto). Ver [OPERATIONS.md §4](./OPERATIONS.md#4-despliegue) para la ubicación del archivo.
