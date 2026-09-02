<script lang="ts">
  import { Dialog, Tabs } from 'bits-ui';
  import {
    IconActivity,
    IconAlertTriangle,
    IconArrowDown,
    IconArrowUp,
    IconBraces,
    IconChevronsLeft,
    IconChevronsRight,
    IconChevronLeft,
    IconChevronRight,
    IconClipboard,
    IconClock,
    IconCode,
    IconDeviceDesktop,
    IconFileText,
    IconHash,
    IconListDetails,
    IconMapPin,
    IconNetwork,
    IconRefresh,
    IconRoute,
    IconSearch,
    IconServer,
    IconX
  } from '@tabler/icons-svelte';
  import { onMount } from 'svelte';
  import type {
    DashboardStats,
    RequestDetail,
    RequestPage,
    RequestSummary
  } from './types';

  let requests: RequestSummary[] = [];
  let stats: DashboardStats = { total_requests: 0, error_count: 0, avg_latency_ms: 0 };
  let selected: RequestDetail | null = null;
  let isDetailOpen = false;
  let isLoading = true;
  let error = '';
  let copyNotice = '';
  let search = '';
    { value: 'is', label: '=' },
    { value: 'not_equal', label: '<>' },
    { value: 'gt', label: '>' },
    { value: 'gte', label: '>=' },
    { value: 'lt', label: '<' },
    { value: 'lte', label: '<=' }
  ];

  const localTimestampFormatter = new Intl.DateTimeFormat(undefined, {
    dateStyle: 'medium',
    timeStyle: 'medium'
  });

  const formatTime = (timestamp: string) => {
    const date = new Date(timestamp);
    return Number.isNaN(date.getTime()) ? timestamp : localTimestampFormatter.format(date);
  };
  const statusTone = (status: number) => status >= 500 ? 'failure' : status >= 400 ? 'warning' : 'success';
  const requestTarget = (request: RequestDetail) => `${request.path}${request.query ? `?${request.query}` : ''}`;
  const responseLabel = (status: number) => status ? `HTTP ${status}` : 'Respuesta incompleta';

  function formatHeaders(headers: string) {
    try {
      return JSON.stringify(JSON.parse(headers), null, 2);
    } catch {
      return headers || '(sin headers capturados)';
    }
  }

  function rawRecord(request: RequestDetail) {
    return JSON.stringify(request, null, 2);
  }

  async function loadRequests(page = currentPage) {
    isLoading = true;
    error = '';
    try {
      const params = new URLSearchParams({
        sort: sortField,
        direction: sortDirection,
        page: page.toString(),
        page_size: pageSize.toString(),
        search
      });
      const [requestsResponse, statsResponse] = await Promise.all([
        fetch(`/api/requests?${params}`),
        fetch('/api/stats')
      ]);
      if (!requestsResponse.ok || !statsResponse.ok) throw new Error('No se pudieron cargar las solicitudes.');
      const requestPage: RequestPage = await requestsResponse.json();
      requests = requestPage.items;
      currentPage = requestPage.page;
      requestedPage = requestPage.page.toString();
      pageSize = requestPage.page_size;
      totalItems = requestPage.total_items;
      totalPages = requestPage.total_pages;
      stats = await statsResponse.json();
    } catch (cause) {
      error = cause instanceof Error ? cause.message : 'Error inesperado.';
    } finally {
      isLoading = false;
    }
  }

  function reloadFromFirstPage() {
    loadRequests(1);
  }

  function jumpToRequestedPage() {
    const page = Number.parseInt(requestedPage, 10);
    if (Number.isNaN(page) || page < 1) {
      requestedPage = currentPage.toString();
      return;
    }
    loadRequests(page);
  }

  function toggleSort(field: string) {
    if (sortField === field) {
      sortDirection = sortDirection === 'asc' ? 'desc' : 'asc';
    } else {
      sortField = field;
      sortDirection = 'asc';
    }
    reloadFromFirstPage();
  }

  function sortAriaLabel(field: string, label: string) {
    if (sortField !== field) return `Ordenar por ${label}`;
    const nextDirection = sortDirection === 'asc' ? 'descendente' : 'ascendente';
    return `Ordenado por ${label} de forma ${sortDirection === 'asc' ? 'ascendente' : 'descendente'}. Cambiar a ${nextDirection}`;
  }

  async function showDetail(id: number) {
    const response = await fetch(`/api/requests/${id}`);
    if (!response.ok) {
      error = 'No se pudo cargar el detalle.';
      return;
    }
    selected = await response.json();
    copyNotice = '';
    isDetailOpen = true;
  }

  async function copyDetail() {
    if (!selected) return;

    try {
      await navigator.clipboard.writeText(rawRecord(selected));
      copyNotice = 'Detalle copiado.';
    } catch {
      copyNotice = 'No se pudo copiar desde este navegador.';
    }
  }

  onMount(() => {
    loadRequests();
    const interval = window.setInterval(loadRequests, 5000);
    return () => window.clearInterval(interval);
  });
</script>

<svelte:head><title>ReqLens</title></svelte:head>

<main>
  <header class="masthead">
    <div>
      <p class="eyebrow">Observabilidad local</p>
      <h1>ReqLens</h1>
    </div>
    <button class="icon-button refresh" onclick={() => loadRequests()} disabled={isLoading} aria-label="Actualizar solicitudes" title="Actualizar solicitudes">
      <IconRefresh size={18} stroke={1.8} />
    </button>
  </header>

  <section class="stats-grid" aria-label="Resumen de captura">
    <div>
      <span><IconListDetails size={15} stroke={1.8} /> Total</span>
      <strong>{stats.total_requests}</strong>
    </div>
    <div>
      <span><IconAlertTriangle size={15} stroke={1.8} /> Errores</span>
      <strong class:failure={stats.error_count > 0}>{stats.error_count}</strong>
    </div>
    <div>
      <span><IconActivity size={15} stroke={1.8} /> Latencia media</span>
      <strong>{stats.avg_latency_ms.toFixed(1)} ms</strong>
    </div>
  </section>

  {#if error}
    <p class="message error">{error}</p>
  {/if}

  <section class="request-panel" aria-label="Solicitudes capturadas">
    <div class="panel-heading">
      <h2>Solicitudes recientes</h2>
      <span>{totalItems} registros</span>
    </div>
    <div class="quick-search">
      <label>
        <span><IconSearch size={16} stroke={1.8} /> Buscar</span>
        <input bind:value={search} oninput={reloadFromFirstPage} placeholder="Buscar en todas las columnas..." />
      </label>
    </div>
    {#if isLoading}
      <p class="message">Cargando solicitudes...</p>
    {:else if requests.length === 0}
      <p class="message">No hay solicitudes capturadas todavía.</p>
    {:else}
      <div class="table-wrap">
        <table>
          <thead>
            <tr>
              <th aria-sort={sortField === 'id' ? (sortDirection === 'asc' ? 'ascending' : 'descending') : 'none'}><button class="table-sort" onclick={() => toggleSort('id')} aria-label={sortAriaLabel('id', 'ID')}><IconHash size={14} stroke={1.8} /> ID {#if sortField === 'id'}{#if sortDirection === 'asc'}<IconArrowUp size={14} stroke={1.8} />{:else}<IconArrowDown size={14} stroke={1.8} />{/if}{/if}</button></th>
              <th aria-sort={sortField === 'timestamp' ? (sortDirection === 'asc' ? 'ascending' : 'descending') : 'none'}><button class="table-sort" onclick={() => toggleSort('timestamp')} aria-label={sortAriaLabel('timestamp', 'Hora local')}><IconClock size={14} stroke={1.8} /> Hora local {#if sortField === 'timestamp'}{#if sortDirection === 'asc'}<IconArrowUp size={14} stroke={1.8} />{:else}<IconArrowDown size={14} stroke={1.8} />{/if}{/if}</button></th>
              <th aria-sort={sortField === 'method' ? (sortDirection === 'asc' ? 'ascending' : 'descending') : 'none'}><button class="table-sort" onclick={() => toggleSort('method')} aria-label={sortAriaLabel('method', 'Método')}><IconCode size={14} stroke={1.8} /> Método {#if sortField === 'method'}{#if sortDirection === 'asc'}<IconArrowUp size={14} stroke={1.8} />{:else}<IconArrowDown size={14} stroke={1.8} />{/if}{/if}</button></th>
              <th aria-sort={sortField === 'status' ? (sortDirection === 'asc' ? 'ascending' : 'descending') : 'none'}><button class="table-sort" onclick={() => toggleSort('status')} aria-label={sortAriaLabel('status', 'Status')}><IconServer size={14} stroke={1.8} /> Status {#if sortField === 'status'}{#if sortDirection === 'asc'}<IconArrowUp size={14} stroke={1.8} />{:else}<IconArrowDown size={14} stroke={1.8} />{/if}{/if}</button></th>
              <th aria-sort={sortField === 'duration' ? (sortDirection === 'asc' ? 'ascending' : 'descending') : 'none'}><button class="table-sort" onclick={() => toggleSort('duration')} aria-label={sortAriaLabel('duration', 'Latencia')}><IconActivity size={14} stroke={1.8} /> Latencia {#if sortField === 'duration'}{#if sortDirection === 'asc'}<IconArrowUp size={14} stroke={1.8} />{:else}<IconArrowDown size={14} stroke={1.8} />{/if}{/if}</button></th>
              <th aria-sort={sortField === 'client_ip' ? (sortDirection === 'asc' ? 'ascending' : 'descending') : 'none'}><button class="table-sort" onclick={() => toggleSort('client_ip')} aria-label={sortAriaLabel('client_ip', 'Cliente')}><IconNetwork size={14} stroke={1.8} /> Cliente {#if sortField === 'client_ip'}{#if sortDirection === 'asc'}<IconArrowUp size={14} stroke={1.8} />{:else}<IconArrowDown size={14} stroke={1.8} />{/if}{/if}</button></th>
              <th aria-sort={sortField === 'path' ? (sortDirection === 'asc' ? 'ascending' : 'descending') : 'none'}><button class="table-sort" onclick={() => toggleSort('path')} aria-label={sortAriaLabel('path', 'Path')}><IconRoute size={14} stroke={1.8} /> Path {#if sortField === 'path'}{#if sortDirection === 'asc'}<IconArrowUp size={14} stroke={1.8} />{:else}<IconArrowDown size={14} stroke={1.8} />{/if}{/if}</button></th>
              <th><span class="table-label"><IconBraces size={14} stroke={1.8} /> Query params</span></th>
            </tr>
          </thead>
          <tbody>
            {#each requests as request}
              <tr onclick={() => showDetail(request.id)} onkeydown={(event) => event.key === 'Enter' && showDetail(request.id)} tabindex="0">
                <td>#{request.id}</td><td>{formatTime(request.timestamp)}</td><td>{request.method}</td>
                <td><span class:success={statusTone(request.resp_status) === 'success'} class:warning={statusTone(request.resp_status) === 'warning'} class:failure={statusTone(request.resp_status) === 'failure'}>{request.resp_status || '—'}</span></td>
                <td>{request.duration_ms} ms</td><td>{request.client_ip}</td><td class="path">{request.path}</td>
                <td class="query">{request.query || '—'}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
      <nav class="pagination" aria-label="Paginación de solicitudes">
        <label class="page-size-control">
          <span>Por página</span>
          <select bind:value={pageSize} onchange={reloadFromFirstPage} aria-label="Filas por página">
            <option value={10}>10</option>
            <option value={25}>25</option>
            <option value={50}>50</option>
            <option value={100}>100</option>
          </select>
        </label>
        <form class="page-jump" onsubmit={(event) => { event.preventDefault(); jumpToRequestedPage(); }}>
          <span>Página {currentPage} de {totalPages}</span>
          <label>
            <span>Ir a</span>
            <input bind:value={requestedPage} type="number" min="1" max={totalPages} inputmode="numeric" aria-label="Ir a página" />
          </label>
          <button class="text-button" type="submit" disabled={isLoading}>Ir</button>
        </form>
        <div class="page-actions">
          <button class="text-button" onclick={() => loadRequests(1)} disabled={isLoading || currentPage === 1}>
            <IconChevronsLeft size={16} stroke={1.8} /> Primera
          </button>
          <button class="text-button" onclick={() => loadRequests(currentPage - 1)} disabled={isLoading || currentPage === 1}>
            <IconChevronLeft size={16} stroke={1.8} /> Anterior
          </button>
          <button class="text-button" onclick={() => loadRequests(currentPage + 1)} disabled={isLoading || currentPage === totalPages}>
            Siguiente <IconChevronRight size={16} stroke={1.8} />
          </button>
          <button class="text-button" onclick={() => loadRequests(totalPages)} disabled={isLoading || currentPage === totalPages}>
            Última <IconChevronsRight size={16} stroke={1.8} />
          </button>
        </div>
      </nav>
    {/if}
  </section>
</main>

<Dialog.Root bind:open={isDetailOpen}>
  <Dialog.Portal>
    <Dialog.Overlay class="dialog-overlay" />
    <Dialog.Content class="dialog-content">
      <div class="detail-heading">
        <div class="detail-title">
          <span class="method-chip">{selected?.method ?? 'HTTP'}</span>
          <div>
            <p class="eyebrow">Inspección de solicitud</p>
            <Dialog.Title>{selected ? requestTarget(selected) : 'Detalle'}</Dialog.Title>
          </div>
        </div>
        <div class="detail-actions">
          <button class="icon-button copy" onclick={copyDetail} aria-label="Copiar detalle como JSON" title="Copiar detalle como JSON">
            <IconClipboard size={18} stroke={1.8} />
          </button>
          <Dialog.Close class="icon-button close" aria-label="Cerrar detalle" title="Cerrar detalle">
            <IconX size={18} stroke={1.8} />
          </Dialog.Close>
        </div>
      </div>
      {#if selected}
        <div class="detail-layout">
          <aside class="detail-sidebar" aria-label="Metadatos de la solicitud">
            <div class="response-status" class:success={statusTone(selected.resp_status) === 'success'} class:warning={statusTone(selected.resp_status) === 'warning'} class:failure={statusTone(selected.resp_status) === 'failure'}>
              <IconServer size={19} stroke={1.8} />
              <div><span>Respuesta</span><strong>{responseLabel(selected.resp_status)}</strong></div>
            </div>
            <dl class="request-facts">
              <div><dt><IconFileText size={15} stroke={1.8} /> Registro</dt><dd>#{selected.id}</dd></div>
              <div><dt><IconClock size={15} stroke={1.8} /> Hora local</dt><dd>{formatTime(selected.timestamp)}</dd></div>
              <div><dt><IconMapPin size={15} stroke={1.8} /> Cliente</dt><dd>{selected.client_ip}</dd></div>
              <div><dt><IconActivity size={15} stroke={1.8} /> Latencia</dt><dd>{selected.duration_ms} ms</dd></div>
              <div><dt><IconDeviceDesktop size={15} stroke={1.8} /> User-Agent</dt><dd>{selected.client_ua ?? 'No capturado'}</dd></div>
            </dl>
            {#if copyNotice}
              <p class="copy-notice">{copyNotice}</p>
            {/if}
          </aside>
          <div class="detail-evidence">
            <section class="target-summary">
              <IconRoute size={20} stroke={1.8} />
              <div><span>Destino</span><code>{requestTarget(selected)}</code></div>
            </section>
            <Tabs.Root value="request" class="detail-tabs">
              <Tabs.List aria-label="Contenido del detalle">
                <Tabs.Trigger value="request"><IconCode size={16} stroke={1.8} /> Request</Tabs.Trigger>
                <Tabs.Trigger value="response"><IconServer size={16} stroke={1.8} /> Response</Tabs.Trigger>
                <Tabs.Trigger value="raw"><IconBraces size={16} stroke={1.8} /> Raw JSON</Tabs.Trigger>
              </Tabs.List>
              <Tabs.Content value="request">
                <section class="evidence-section">
                  <h3>Request Headers</h3>
                  <pre>{formatHeaders(selected.req_headers)}</pre>
                </section>
                <section class="evidence-section">
                  <h3>Request Body</h3>
                  <pre>{selected.req_body ?? '(vacío)'}</pre>
                </section>
              </Tabs.Content>
              <Tabs.Content value="response">
                <section class="evidence-section">
                  <h3>Response Status</h3>
                  <code>{selected.resp_status || 'Incompleto: no se capturo una respuesta HTTP completa'}</code>
                </section>
                <section class="evidence-section">
                  <h3>Response Headers</h3>
                  <pre>{formatHeaders(selected.resp_headers)}</pre>
                </section>
                <section class="evidence-section">
                  <h3>Response Body</h3>
                  <pre>{selected.resp_body ?? '(vacío)'}</pre>
                </section>
              </Tabs.Content>
              <Tabs.Content value="raw">
                <section class="evidence-section">
                  <h3>Registro persistido</h3>
                  <pre>{rawRecord(selected)}</pre>
                </section>
              </Tabs.Content>
            </Tabs.Root>
          </div>
        </div>
      {/if}
    </Dialog.Content>
  </Dialog.Portal>
</Dialog.Root>