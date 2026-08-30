# ReqLens — Modelo de Seguridad y Auditoría

> Especificación del modelo de amenazas, algoritmos de redacción de secretos, filtros de cabeceras y directivas de hardening para entornos de producción.
> Para la visión arquitectónica global → [ARCHITECTURE.md](../ARCHITECTURE.md). Para la guía de despliegue seguro → [docs/OPERATIONS.md](OPERATIONS.md).

| Propiedad | Especificación |
| :--- | :--- |
| **Postura de Seguridad** | *Fail-Safe* (Redacción y exclusión de secretos activa por defecto) |
| **Protección de Almacenamiento** | Permisos POSIX `0600` / Sandbox systemd sin privilegios |
| **Filtro de Encabezados** | Allowlist estricta + Blacklist inmutable de credenciales |
| **Audiencia** | Auditores de Seguridad, Ingenieros de SecOps y Mantenedores |

---

## 1. Matriz de Amenazas y Mitigaciones (STRIDE)

| Vector de Amenaza | Categoría | Exposición / Riesgo | Mitigación Implementada en ReqLens |
| :--- | :--- | :--- | :--- |
| **Filtración de Secretos y PII** | Confidencialidad | Presencia de contraseñas, tokens JWT o claves de API en los cuerpos HTTP capturados. | Motor de redacción dual (recorrido de AST JSON + regex) reemplaza valores por `[REDACTED]`. |
| **Spoofing de `X-Forwarded-For`** | Integridad | Un cliente malicioso inyecta cabeceras `X-Forwarded-For` falsificadas para enmascarar su IP real. | ReqLens realiza siempre *append* de la IP física del socket TCP entrante, preservando la cadena real sin permitir sustitución. |
| **Denegación de Servicio (DoS por Payload Gigante)** | Disponibilidad | Envío de cuerpos HTTP masivos (ej. cientos de MB) para agotar la memoria RAM del proxy. | Límite estricto de captura en buffer (`--max-body`, default 64 KB); el reenvío al upstream se ejecuta en streaming continuo. |
| **HTTP Request Smuggling (CL/TE)** | Integridad | Desincronización entre proxy y Apache por discrepancias en `Content-Length` o `Transfer-Encoding`. | `hyper` normaliza y valida estrictamente las cabeceras HTTP antes de procesar el stream hacia el backend. |
| **Acceso No Autorizado a la Base de Datos** | Confidencialidad | Lectura no autorizada del archivo `.db` en disco por otros procesos del sistema. | Creación del archivo con permisos POSIX `0600` (`rw-------`) y ejecución en sandbox systemd (`ProtectSystem=strict`, `NoNewPrivileges=true`). |
| **Inyección SQL** | Integridad | Cuerpos maliciosos que intenten manipular el motor de persistencia. | **No aplicable por diseño:** Todas las inserciones en SQLite utilizan parámetros vinculados (*prepared statements*); cero concatenación de strings. |
| **Server-Side Request Forgery (SSRF)** | Integridad | Redirección de peticiones del proxy hacia recursos internos no deseados. | **No aplicable:** El upstream es una dirección fija configurada por el operador en el arranque, nunca un valor extraído de la petición del cliente. |

---

## 2. Motor de Redacción de Secretos (*Fail-Safe*)

La redacción de credenciales y datos sensibles está **habilitada por defecto** y opera bajo una estrategia en dos capas:

```
                      ┌────────────────────────────┐
                      │    Cuerpo HTTP Recibido    │
                      └─────────────┬──────────────┘
                                    │
                         ¿Es JSON válido UTF-8?
                                    │
                    ┌───────────────┴───────────────┐
                 SÍ │                               │ NO
                    ▼                               ▼
    ┌───────────────────────────────┐   ┌───────────────────────────────┐
    │     Recorrido del AST JSON    │   │  Escaneo por Regex de Respaldo│
    │  Reemplaza valores de claves  │   │   Detecta pares clave=valor   │
    │  sensibles por `[REDACTED]`   │   │   y "clave":"valor" sensibles │
    └───────────────┬───────────────┘   └───────────────┬───────────────┘
                    │                                   │
                    └───────────────┬───────────────────┘
                                    ▼
                      ┌────────────────────────────┐
                      │ Payload Redactado Seguro   │
                      └────────────────────────────┘
```

### Claves Sensibles Identificadas Automáticamente
Por defecto, cualquier clave coincidente (case-insensitive) con los siguientes patrones tendrá su valor sustituido por `[REDACTED]`:
`password`, `pass`, `token`, `secret`, `api_key`, `apikey`, `authorization`, `auth`, `access_token`, `refresh_token`, `private_key`, `client_secret`, `credit_card`.

> ⚠️ **Desactivación de Redacción:** El uso de `--no-redact` desactiva este motor y emite inmediatamente un mensaje de advertencia `warn!` en los logs de arranque, dejando constancia explícita de que el operador asumió el riesgo de almacenar secretos en disco.

---

## 3. Política de Encabezados (Allowlist Estricta)

Para blindar la base de datos contra la captura accidental de tokens de sesión y cabeceras de autorización, ReqLens aplica una política estricta de filtrado:

### Cabeceras Prohibidas (Blacklist Inmutable)
Estas cabeceras son **descartadas antes de la serialización** y jamás se registrarán en `req_headers` ni `resp_headers`:
- `authorization`
- `cookie`
- `set-cookie`
- `proxy-authorization`
- `proxy-authenticate`

### Cabeceras Permitidas por Defecto (Allowlist)
Únicamente se capturan encabezados esenciales para el diagnóstico y la correlación:
- `content-type`, `content-length`, `accept`, `user-agent`, `referer`, `origin`, `host`, `x-request-id`, `x-forwarded-for`, `x-forwarded-proto`.

---

## 4. Protección contra Cargas Binarias y Comprimidas

1. **Aislamiento de Cargas Binarias (`[BINARY]`):** Si un cuerpo no puede decodificarse como UTF-8 válido o su `Content-Type` corresponde a medios binarios (ej. `application/octet-stream`, imágenes, ejecutables), se almacena el marcador `[BINARY]`, evitando corromper la base con volcados binarios.
2. **Defensa contra Bombas de Descompresión (`[COMPRESSED]`):** Si la respuesta incluye encabezados `Content-Encoding: gzip`, `br` o `deflate`, ReqLens registra el marcador `[COMPRESSED]` en lugar de descomprimir el flujo. Esto protege la CPU del host contra ataques de bombas de descompresión (*zip bombs*).

---

## 5. Hardening del Proceso y Aislamiento en Producción

- **Principio de Mínimo Privilegio:** ReqLens no requiere privilegios de `root` ni capacidades especiales de Linux (`CAP_NET_BIND_SERVICE`). Se ejecuta bajo el usuario dedicado `reqlens`.
- **Restricción del Sistema de Archivos:** La directiva `ProtectSystem=strict` de systemd monta el sistema de archivos en modo solo lectura, permitiendo escritura exclusivamente en la ruta delegada de base de datos (`ReadWritePaths=/var/lib/reqlens`).
- **Inhabilitación de Elevación de Privilegios:** La bandera `NoNewPrivileges=true` impide que el proceso o cualquier subproceso obtenga privilegios adicionales mediante binarios `setuid`.

