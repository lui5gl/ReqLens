export interface RequestSummary {
  id: number;
  timestamp: string;
  duration_ms: number;
  client_ip: string;
  client_ua: string | null;
  method: string;
  path: string;
  query: string | null;
  resp_status: number;
}

export interface RequestPage {
  items: RequestSummary[];
  page: number;
  page_size: number;
  total_items: number;
  total_pages: number;
}

export type AdvancedRuleField = 'id' | 'method' | 'status' | 'duration' | 'client_ip' | 'path' | 'query' | 'timestamp';
export type AdvancedRuleOperator = 'is' | 'not_equal' | 'contains' | 'not_contains' | 'starts_with' | 'gt' | 'gte' | 'lt' | 'lte';

export interface AdvancedRule {
  id: AdvancedRuleField;
  field: AdvancedRuleField;
  operator: AdvancedRuleOperator;
  value: string;
}

export interface RequestDetail extends RequestSummary {
  client_ua: string | null;
  query: string | null;
  req_headers: string;
  req_body: string | null;
  resp_headers: string;
  resp_body: string | null;
}

export interface DashboardStats {
  total_requests: number;
  error_count: number;
  avg_latency_ms: number;
}