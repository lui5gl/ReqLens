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

ReqLens observa de forma **pasiva** una copia del tráfico HTTP/1.x plaintext
mediante `AF_PACKET` en Linux. No abre el puerto observado, no reenvía las
conexiones y no modifica el camino entre los clientes y Apache.

```
┌──────────┐                   ┌──────────────┐        ┌─────────────┐
│ Cliente  │──────────────────▶│ Apache (:80) │───────▶│ PHP / App   │
└──────────┘       HTTP        └──────┬───────┘        └─────────────┘
                                     │ copia de paquetes (sin modificar tráfico)
                                     ▼
                            ┌──────────────────┐
                            │ ReqLens sniff    │──────▶ SQLite (WAL)
                            └──────────────────┘
```

### Principios Fundamentales de Operación

1. **No Invasivo (Zero-Config Apache):** No requiere instalar módulos, alterar `httpd.conf`, cambiar el puerto 80, reiniciar Apache ni agregar reglas NAT.
2. **Fail-Open Real:** ReqLens recibe una copia de los paquetes; detenerlo o matarlo no interrumpe Apache ni PHP.
3. **Persistencia Desacoplada (Asíncrona):** Las peticiones y respuestas se capturan en memoria mediante un canal acotado; la latencia de escritura a disco nunca penaliza el tiempo de respuesta del cliente.
4. **Seguridad y Privacidad por Defecto (*Fail-Safe*):** Redacción automática de credenciales y datos sensibles (`password`, `token`, `secret`, `api_key`) y exclusión inmutable de cookies y cabeceras de autorización.

---

## 📊 Capacidades y Casos de Uso

- **Dashboard Interactivo Integrado (TUI):** Visualiza peticiones en tiempo real, filtra errores (≥400), detecta peticiones lentas (≥500ms) e inspecciona headers y cuerpos completos sin abrir `sqlite3` (activable mediante `--tui`).
- **Modo Servidor Headless (Producción):** Ejecución sin UI, ideal para demonios de `systemd`, contenedores y entornos desatendidos.
- **Diagnóstico y Debugging Forense:** Inspecciona el payload exacto de peticiones que generaron excepciones en backend.
- **Auditoría de Endpoints Críticos:** Registra la actividad y modificaciones en APIs sensibles con trazabilidad de IP real del cliente (`X-Forwarded-For`).
- **Análisis de Rendimiento:** Mide latencias punto a punto (`duration_ms`) y detecta cuellos de botella por endpoint.
- **Consultas Analíticas Inmediatas:** Utiliza SQL estándar sobre SQLite para filtrar, agrupar y diagnosticar sin necesidad de montar clústeres externos de telemetría (Elasticsearch, OpenTelemetry, etc.).
- **Gestión Nativa de Ciclo de Vida:** Comandos directos integrados en la CLI (`reqlens status`, `reqlens restart`, `reqlens disable`, `reqlens uninstall`).

### Inicio rápido: captura pasiva

```bash
# Observa HTTP plaintext en el puerto 80 sin ocuparlo. Requiere root o CAP_NET_RAW.
sudo reqlens sniff \
  --interface any \
  --server-ip 172.23.25.36 \
  --port 80 \
  --db-path /var/lib/reqlens/reqlens.db \
  --tui

# Instala el mismo modo como servicio de arranque automático (modo por defecto).
sudo reqlens install \
  --mode sniff \
  --interface any \
  --server-ip 172.23.25.36 \
  --port 80
```

Apache debe continuar siendo el único proceso escuchando en `:80`. El modo
pasivo soporta IPv4 y HTTP/1.0–HTTP/1.1 sin TLS. HTTPS cifra los headers y bodies
y no puede inspeccionarse pasivamente. El proxy histórico se conserva de forma
explícita con `reqlens start` (alias conceptual `proxy`) para instalaciones que
sí acepten colocarlo en el camino crítico.



---

## 🛡️ Garantías y Resiliencia

ReqLens implementa una semántica **at-most-once** diseñada específicamente para telemetría de alta velocidad:

| Escenario | Comportamiento del Sistema |
| :--- | :--- |
| **Pico de tráfico saturado (>1k req/s)** | Los eventos excedentes en cola se descartan; Apache no depende del observador. |
| **Caída o desconexión de disco** | El lote se revierte y se reporta; las peticiones HTTP siguen llegando directamente a Apache. |
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
