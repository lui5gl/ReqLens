# ReqLens — Arquitectura del sistema

> Especificación de la implementación actual. ReqLens ofrece captura pasiva
> como modo recomendado y conserva el reverse proxy como modo de compatibilidad.

## 1. Invariantes

1. En modo `sniff`, ReqLens no abre, redirige ni reenvía el puerto observado.
2. Apache continúa atendiendo directamente a sus clientes aunque ReqLens falle.
3. La captura nunca bloquea el tráfico: los eventos se descartan si la cola de
   telemetría está llena.
4. La memoria destinada a flujos, segmentos fuera de orden y bodies está acotada.
5. Los secretos se redactan antes de escribir en SQLite.

## 2. Modo pasivo (`reqlens sniff`)

```text
Cliente ───────────────▶ Apache :80 ─────────▶ PHP
            copia IPv4/TCP │
                           ▼
                    AF_PACKET + BPF
                           │
                           ▼
               reensamblado TCP bidireccional
                           │
                           ▼
                 parser HTTP/1.x incremental
                           │
                           ▼
               correlación request ↔ response
                           │
                           ▼
              redacción → MPSC → SQLite WAL
```

El socket `AF_PACKET/SOCK_DGRAM` recibe datagramas IPv4 sin encabezado de enlace.
Un filtro BPF en el kernel entrega solamente TCP con el puerto observado como
origen o destino. ReqLens clasifica cada conexión por las direcciones y puertos
de cliente y servidor, reconstruye ambas direcciones usando sequence numbers y
correlaciona las respuestas con una cola de requests por conexión keep-alive.

### Límites de recursos

| Recurso | Límite |
|---|---:|
| Flujos simultáneos observados | 16,384 |
| Stream HTTP pendiente por dirección | 512 KiB |
| Segmentos fuera de orden por dirección | 256 KiB |
| Inactividad antes de expirar un flujo | 60 s |
| Cola de eventos hacia SQLite | 1,024 |
| Body persistido por defecto | 64 KiB |

Al alcanzar un límite se pierde telemetría, nunca tráfico de Apache.

### Alcance protocolario

- Linux, IPv4 y TCP.
- HTTP/1.0 y HTTP/1.1 plaintext.
- `Content-Length` y `Transfer-Encoding: chunked`.
- Requests múltiples sobre keep-alive.
- Retransmisiones y segmentos fuera de orden dentro de los límites configurados.
- No descifra HTTPS/TLS.
- No implementa HTTP/2, HTTP/3 ni reensamblado de fragmentos IPv4.
- Si ReqLens comienza a mitad de una conexión, puede descartar ese intercambio
  hasta reconocer un límite HTTP válido.

La captura requiere root o `CAP_NET_RAW`. El instalador ejecuta el servicio como
root para compatibilidad con systemd y SysV antiguos; reducir privilegios tras
abrir el socket queda como hardening futuro.

## 3. Modo proxy de compatibilidad (`reqlens proxy`)

```text
Cliente ──▶ ReqLens listener ──▶ Apache upstream
```

El proxy usa `std::net::TcpListener`, `TcpStream` y un hilo acotado por conexión.
Este modo ofrece mayor fidelidad HTTP, pero ReqLens forma parte del camino
crítico. Listener y upstream deben utilizar puertos diferentes; una configuración
local sobre el mismo puerto se rechaza para impedir recursión y consumo de CPU.

## 4. Pipeline común

Ambos motores producen `capture::HttpEvent`. Los headers sensibles y cuerpos se
normalizan y redactan, y después `IngestSender::try_send` intenta introducir el
evento en un canal síncrono acotado. El writer monohilo agrupa hasta 100 eventos
o 250 ms y confirma una transacción SQLite en modo WAL.

```text
sniff ─┐
       ├─▶ HttpEvent ─▶ redacción ─▶ sync_channel(1024) ─▶ SQLite WAL
proxy ─┘
```

La persistencia puede perder eventos bajo presión o error de disco. Esa semántica
`at-most-once` mantiene acotados CPU y memoria.

## 5. Cierre

SIGINT y SIGTERM cambian atómicamente el estado `running` a `false`. Los loops de
captura y proxy despiertan como máximo tras su timeout, dejan de aceptar trabajo,
cierran sus senders y permiten que el writer vacíe la cola antes de terminar.

## 6. Estructura

```text
src/
├── sniff/      # AF_PACKET, BPF, IPv4/TCP, reensamblado y correlación HTTP
├── proxy/      # Reverse proxy opcional basado en sockets POSIX
├── capture/    # HttpEvent, headers y redacción
├── ingest/     # Canal acotado, schema y writer SQLite
├── tui/        # Visor local de SQLite (compatibilidad)
├── config/     # CLI, variables de entorno y validación
└── ops/        # Instalación systemd/SysV y ciclo de vida
```

## 7. Verificación de no intrusión

En modo `sniff`, `ss -lntp` debe seguir mostrando solamente Apache en el puerto
observado. Eliminar ReqLens con `kill -9` no debe cambiar el resultado de una
petición directa a Apache. No se requieren reglas `iptables`/NAT.
