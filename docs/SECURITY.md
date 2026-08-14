# ReqLens — Seguridad

> Modelo de amenazas y mitigaciones del sistema. Para el diseño → [ARCHITECTURE.md](../ARCHITECTURE.md).

|                          |                                      |
| ------------------------ | ------------------------------------ |
| **Versión**              | 0.1.0                                |
| **Última actualización** | 2026-08-14                           |
| **Audiencia**            | Auditores de seguridad, mantenedores |

---

## 1. Modelo de amenazas

| Amenaza                                         | Exposición          | Mitigación                                                                                                                                                                                |
| ----------------------------------------------- | ------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Filtración de secretos (passwords, tokens, PII) | Confidencialidad    | Redacción default-on + allowlist de headers + `0600` + sin descompresión. **Bypass conocido:** claves sensibles fuera de la lista → la lista es configurable y hay regex de respaldo      |
| Spoofing de `X-Forwarded-For`                   | Integridad del dato | El cliente puede forjar XFF; se hace **append** del IP real de socket, nunca replace — Apache decide                                                                                      |
| DoS por body gigante                            | Disponibilidad      | Captura acotada por `--max-body`; el reenvío es streaming (memoria no afectada)                                                                                                           |
| Request smuggling (CL/TE)                       | Integridad          | hyper normaliza; el body capturado es el lógico. Riesgo residual si Apache interpreta distinto → testing adversarial ([ARCHITECTURE.md §11](../ARCHITECTURE.md#11-estrategia-de-testing)) |
| Acceso al `.db`                                 | Confidencialidad    | Permisos `0600`; SQLite no cifra → SQLCipher si el threat model lo exige (diferido)                                                                                                       |
| Inyección SQL                                   | Integridad          | No aplica: ningún input del usuario llega a SQL; todos los valores van como parámetros vinculados                                                                                         |
| SSRF                                            | —                   | ReqLens **es** un proxy; el upstream es configuración, no input del cliente. Riesgo bajo por diseño                                                                                       |
| Logs propios (tracing)                          | Confidencialidad    | Nunca incluyen bodies; solo metadata                                                                                                                                                      |

## 2. Redacción de secretos (fail-safe)

- **Default-on**: la redacción está activada por defecto (fail-safe). La desactivación (`--no-redact`) requiere flag explícito y emite `warn!` en startup — la prueba de que el riesgo fue asumido conscientemente.
- **Mecanismo:** si el body es JSON, se parsea y se reemplazan valores de claves en la lista de sensibles (`password`, `token`, `secret`, `api_key`, `authorization`, ...) por `[REDACTED]`. Si el parseo falla, se aplica redacción por regex sobre pares `clave=valor` y `"clave":"valor"`.
- **Límite conocido:** la lista de claves es configurable. Un campo sensible fuera de la lista se capturará — ajusta la configuración antes de exponer endpoints con datos críticos.

## 3. Filtro de headers (allowlist)

- Se capturan solo headers de una **allowlist** configurable (`content-type`, `content-length`, `accept`, `user-agent`, `referer`, `x-request-id`, ...).
- `authorization`, `cookie`, `proxy-authorization`, `set-cookie` se excluyen **siempre**, sin excepción configurable.

## 4. Datos no textuales

- Cuerpos que no decodifiquen como UTF-8 → marcador `[BINARY]` (nunca base64 de datos arbitrarios).
- `content-encoding: gzip/br/deflate` → se registra el header, el body se marca `[COMPRESSED]` (no se descomprime por defecto).

## 5. Hardening del despliegue

- Archivo `.db` creado con permisos `0600` (POSIX) para no exponer payloads en disco.
- Servicio systemd sin root, con `NoNewPrivileges` y `ProtectSystem=strict` (ver [OPERATIONS.md §4](../docs/OPERATIONS.md#4-despliegue)).
- Backup en caliente con `.backup` (WAL) — nunca copiar el `.db` a pelo sin incluir `-wal`/`-shm` (ver [OPERATIONS.md §1](../docs/OPERATIONS.md#1-runbook-operativo)).

## 6. Decisiones diferidas

| Decisión                      | Motivo                                                           |
| ----------------------------- | ---------------------------------------------------------------- |
| SQLCipher (cifrado del `.db`) | Solo si el threat model lo exige                                 |
| Descompresión de bodies       | Costo CPU; default-off hasta que una consulta real lo justifique |
