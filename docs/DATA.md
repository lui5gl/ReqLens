# ReqLens — Modelo de Datos y Especificación de Almacenamiento

> Especificación del esquema físico SQLite, reglas de serialización, formato de payloads y catálogo de consultas analíticas.
> Para la visión arquitectónica global → [ARCHITECTURE.md](../ARCHITECTURE.md). Para manual de operaciones y backups → [docs/OPERATIONS.md](OPERATIONS.md).

| Propiedad | Especificación |
| :--- | :--- |
| **Versión de Modelo** | 0.1.5 |
| **Motor de Base de Datos** | SQLite 3 embebido |





| **Modo de Transacción / Diario** | WAL (*Write-Ahead Logging*) |
| **Estructura de Tabla** | Tabla única desnormalizada (`requests`) |
| **Audiencia** | Desarrolladores, Analistas de Datos y Operadores SRE |

---

## 1. Filosofía del Modelo de Datos

ReqLens utiliza un diseño **desnormalizado de tabla única** optimizado para máxima velocidad de ingestión y simplicidad en consultas forenses:

1. **Cero Joins en Ingestión:** Almacenar los encabezados HTTP como documentos JSON dentro de la misma fila permite persistir cada evento HTTP mediante un único `INSERT` atómico, eliminando transacciones complejas o bloqueos en tablas hijas.
2. **Timestamps ISO-8601 UTC:** Fechas almacenadas en formato textual `YYYY-MM-DDTHH:MM:SS.sssZ` (ej. `2026-08-29T14:30:00.123Z`). Garantizan ordenación lexicográfica natural (`ORDER BY timestamp DESC`) y compatibilidad directa con las funciones de fecha nativas de SQLite (`datetime()`, `strftime()`).
3. **Fidelidad Forense en URIs:** `path` y `query` se preservan en crudo, sin decodificar entidades URL (`%20`, `+`, `%2F`), permitiendo auditar exactamente qué bytes fueron transmitidos por el cliente.
4. **Modo WAL Permanente:** Configurado con `PRAGMA journal_mode=WAL`, habilitando lecturas concurrentes sin retener bloqueos sobre el hilo escritor.

---

## 2. Definición DDL y Schema Físico

El esquema se autoinicializa en el arranque mediante sentencias DDL idempotentes:

```sql
-- Tabla principal de eventos HTTP
CREATE TABLE IF NOT EXISTS requests (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp    TEXT    NOT NULL,
    duration_ms  INTEGER NOT NULL,
    client_ip    TEXT    NOT NULL,
    client_ua    TEXT,
    method       TEXT    NOT NULL,
    path         TEXT    NOT NULL,
    query        TEXT,
    req_headers  TEXT    NOT NULL,
    req_body     TEXT,
    resp_status  INTEGER NOT NULL,
    resp_headers TEXT    NOT NULL,
    resp_body    TEXT
);

-- Índices optimizados para consultas de observabilidad
CREATE INDEX IF NOT EXISTS idx_requests_timestamp   ON requests (timestamp);
CREATE INDEX IF NOT EXISTS idx_requests_method_path ON requests (method, path);
CREATE INDEX IF NOT EXISTS idx_requests_resp_status ON requests (resp_status);
CREATE INDEX IF NOT EXISTS idx_requests_client_ip   ON requests (client_ip);
```

### Diccionario de Columnas

| Columna | Tipo SQLite | Constraints | Índice | Descripción |
| :--- | :--- | :--- | :---: | :--- |
| `id` | `INTEGER` | `PRIMARY KEY AUTOINCREMENT` | Sí | Secuencia única de correlación de eventos. |
| `timestamp` | `TEXT` | `NOT NULL` | Sí | Marca temporal UTC en formato ISO-8601 con milisegundos. |
| `duration_ms` | `INTEGER` | `NOT NULL` | No | Latencia total de la petición (desde recepción hasta respuesta de Apache) en ms. |
| `client_ip` | `TEXT` | `NOT NULL` | Sí | IP del socket cliente (o último salto de `X-Forwarded-For`). |
| `client_ua` | `TEXT` | `NULL` | No | Contenido del encabezado `User-Agent`. |
| `method` | `TEXT` | `NOT NULL` | Sí | Verbo HTTP normalizado en mayúsculas (`GET`, `POST`, `PUT`, etc.). |
| `path` | `TEXT` | `NOT NULL` | Sí | Ruta del endpoint sin query string (ej. `/api/v1/auth/login`). |
| `query` | `TEXT` | `NULL` | No | Parámetros de consulta crudos (ej. `token=xyz&debug=1`). |
| `req_headers` | `TEXT (JSON)` | `NOT NULL` | No | Objeto JSON plano con los headers permitidos por la allowlist. |
| `req_body` | `TEXT` | `NULL` | No | Cuerpo del request (sujeto a redacción de secretos y truncado). |
| `resp_status` | `INTEGER` | `NOT NULL` | Sí | Código de estado HTTP retornado por Apache (ej. `200`, `404`, `500`). |
| `resp_headers`| `TEXT (JSON)` | `NOT NULL` | No | Objeto JSON plano con los headers de respuesta permitidos. |
| `resp_body` | `TEXT` | `NULL` | No | Cuerpo de la respuesta devuelta por Apache (sujeto a truncado). |

---

## 3. Especificación de Marcadores Especiales en Payloads

Para evitar el almacenamiento de binarios corruptos o el colapso de almacenamiento por payloads desmedidos, los campos `req_body` y `resp_body` aplican las siguientes reglas deterministas:

| Marcador | Condición de Activación | Ejemplo / Resultado |
| :--- | :--- | :--- |
| `[REDACTED]` | Valores de campos sensibles identificados en JSON o texto plano. | `{"user":"admin","password":"[REDACTED]"}` |
| `[TRUNCATED]` | Cuerpos que superan el tamaño configurado en `--max-body` (default 64 KB). | `{"data":[1,2,3...]} [TRUNCATED]` |
| `[BINARY]` | Cargas con `Content-Type` no textual o bytes no decodificables en UTF-8. | *(El contenido binario se omite para proteger la base)* |
| `[COMPRESSED]`| Respuestas con `Content-Encoding: gzip / br / deflate`. | *(Se omite la descompresión para evitar zip bombs y CPU waste)* |

> 🔒 **Exclusión Absoluta de Cabeceras:** `authorization`, `cookie`, `set-cookie` y `proxy-authorization` son descartadas antes de serializar `req_headers` y `resp_headers`. Ver [docs/SECURITY.md](SECURITY.md).

---

## 4. Recetario de Consultas SQL (Playbook Forense)

### A. Diagnóstico de Errores e Incidentes 5xx
```sql
-- Top 10 endpoints con mayor tasa de error 5xx en las últimas 24 horas
SELECT method, path, resp_status, COUNT(*) AS total_fallos
FROM requests
WHERE resp_status >= 500 
  AND timestamp >= datetime('now', '-1 day')
GROUP BY method, path, resp_status
ORDER BY total_fallos DESC
LIMIT 10;

-- Inspección detallada del último error 500 (con payloads de request y response)
SELECT timestamp, client_ip, method, path, req_body, resp_body
FROM requests
WHERE resp_status = 500
ORDER BY id DESC
LIMIT 1;
```

### B. Análisis de Rendimiento y Detección de Cuellos de Botella
```sql
-- Distribución de latencia promedio y máxima por endpoint (mínimo 10 muestras)
SELECT method, path,
       COUNT(*) AS total_peticiones,
       ROUND(AVG(duration_ms), 2) AS latencia_media_ms,
       MAX(duration_ms) AS latencia_maxima_ms
FROM requests
GROUP BY method, path
HAVING COUNT(*) >= 10
ORDER BY latencia_media_ms DESC
LIMIT 15;
```

### C. Auditoría Forense y Seguridad por IP
```sql
-- Actividad sospechosa: IPs con mayor volumen de peticiones erróneas (4xx / 5xx)
SELECT client_ip, 
       COUNT(*) AS total_errores,
       MIN(timestamp) AS primer_evento,
       MAX(timestamp) AS ultimo_evento
FROM requests
WHERE resp_status >= 400
GROUP BY client_ip
ORDER BY total_errores DESC
LIMIT 10;

-- Extracción de encabezados específicos usando funciones JSON nativas
SELECT timestamp, client_ip,
       json_extract(req_headers, '$.user-agent') AS user_agent,
       json_extract(req_headers, '$.x-request-id') AS request_id
FROM requests
WHERE path = '/api/v1/checkout'
ORDER BY id DESC
LIMIT 20;
```

---

## 5. Proyecciones de Crecimiento y Almacenamiento

* **Cálculo de huella por fila:** $\approx 500\text{ bytes (metadatos + índices)} + \text{longitud real de bodies capturados}$.
* **Escenario Típico (1,000 req/s con bodies de 1 KB):** $\approx 1.5\text{ GB / día}$.
* **Políticas de Mantenimiento:** Para la ejecución de respaldos en caliente y compactación del archivo de base de datos, consulte [docs/OPERATIONS.md](OPERATIONS.md).


