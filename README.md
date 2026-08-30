# ReqLens

> **Observabilidad granular y forense de tráfico HTTP para Apache:** captura la correlación completa de cada petición — quién envía, a qué endpoint, payload exacto de entrada y respuesta del backend (código de estado + body) — persistido en SQLite y consultable mediante SQL estándar.

---

## 🎯 El Problema: El Punto Ciego de Apache

En infraestructuras basadas en Apache, el registro estándar (`access.log`) únicamente captura metadatos superficiales: método, ruta, status code y bytes transmitidos. 

Frente a incidentes en producción, bugs intermitentes o auditorías de seguridad, **`access.log` no responde las preguntas críticas:**
- ¿Qué JSON o payload exacto envió el cliente que provocó un error 500?
- ¿Qué mensaje de error o respuesta devolvió la aplicación backend?
- ¿Qué parámetros específicos causaron una mutación inesperada de datos?

Habilitar herramientas como `mod_dumpio` requiere reconfigurar Apache, arriesga la estabilidad del proceso principal y satura los logs con volcados de texto difíciles de estructurar y consultar.

---

## 💡 ¿Cómo Funciona ReqLens?

ReqLens actúa como un **proxy reverso transparente** interpuesto entre los clientes y el servidor Apache, capturando el tráfico en vuelo sin perturbar el entorno existente.

```
┌──────────┐        ┌──────────────────┐        ┌──────────────┐        ┌─────────────┐
│ Cliente  │───────▶│ ReqLens (:8080)  │───────▶│ Apache (:80) │───────▶│ App Backend │
└──────────┘        └────────┬─────────┘        └──────────────┘        └─────────────┘
                             │
                  [Ingestión Asíncrona]
                             │
                             ▼
                    ┌─────────────────┐
                    │ SQLite (Modo WAL)│
                    └─────────────────┘
```

### Principios Fundamentales de Operación

1. **No Invasivo (Zero-Config Apache):** No requiere instalar módulos en Apache, alterar archivos `httpd.conf` ni reiniciar servicios.
2. **Fail-Open (Aislamiento Total del Tráfico):** Si la base de datos se bloquea, el disco se satura o la persistencia falla, **el tráfico HTTP jamás se interrumpe**.
3. **Persistencia Desacoplada (Asíncrona):** Las peticiones y respuestas se capturan en memoria mediante un canal acotado; la latencia de escritura a disco nunca penaliza el tiempo de respuesta del cliente.
4. **Seguridad y Privacidad por Defecto (*Fail-Safe*):** Redacción automática de credenciales y datos sensibles (`password`, `token`, `secret`, `api_key`) y exclusión inmutable de cookies y cabeceras de autorización.

---

## 📊 Capacidades y Casos de Uso

- **Diagnóstico y Debugging Forense:** Inspecciona el payload exacto de peticiones que generaron excepciones en backend.
- **Auditoría de Endpoints Críticos:** Registra la actividad y modificaciones en APIs sensibles con trazabilidad de IP real del cliente (`X-Forwarded-For`).
- **Análisis de Rendimiento:** Mide latencias punto a punto (`duration_ms`) y detecta cuellos de botella por endpoint.
- **Consultas Analíticas Inmediatas:** Utiliza SQL estándar sobre SQLite para filtrar, agrupar y diagnosticar sin necesidad de montar clústeres externos de telemetría (Elasticsearch, OpenTelemetry, etc.).

---

## 🛡️ Garantías y Resiliencia

ReqLens implementa una semántica **at-most-once** diseñada específicamente para telemetría de alta velocidad:

| Escenario | Comportamiento del Sistema |
| :--- | :--- |
| **Pico de tráfico saturado (>1k req/s)** | Los eventos excedentes en cola se descartan ordenadamente; el proxy sigue atendiendo tráfico a velocidad de cable. |
| **Caída o desconexión de disco** | El lote de inserción se revierte y se reporta en logs; las peticiones HTTP siguen fluyendo. |
| **Cierre controlado (SIGTERM/SIGINT)** | Drenado automático de peticiones en vuelo y flush de eventos pendientes a disco. |

---

## 📚 Documentación Técnica Detallada

La documentación técnica está estructurada por responsabilidades específicas:

- 🏗️ **[ARCHITECTURE.md](ARCHITECTURE.md)** — Decisiones de diseño (ADRs), flujo de datos, modelo de concurrencia y estrategia de pruebas.
- 🗄️ **[docs/DATA.md](docs/DATA.md)** — Esquema DDL de SQLite, diccionario de columnas, marcadores de payload y catálogo de consultas SQL.
- ⚙️ **[docs/OPERATIONS.md](docs/OPERATIONS.md)** — Guía de despliegue, configuración CLI/env, servicio Systemd, backups WAL y troubleshooting.
- 🔒 **[docs/SECURITY.md](docs/SECURITY.md)** — Modelo de amenazas, matriz de mitigaciones, motor de redacción de secretos y allowlist de encabezados.

---

## 🗺️ Visión del Proyecto (Roadmap)

- Particionado automático de bases de datos por rangos de fechas y políticas de retención.
- Endpoint de observabilidad interna (`/metrics` y `/healthz`).
- Streaming y exportación periódica a formatos NDJSON / CSV.
- Filtros configurables de captura por ruta para ignorar healthchecks recurrentes.

---

## 📄 Licencia

[MIT](LICENSE) © 2026.

