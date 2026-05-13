// ─── Types ────────────────────────────────────────────────────────────────────

export interface GraphSearchResult {
    path:          string;
    name:          string;
    kind:          'file' | 'directory';
    size:          number;
    size_human:    string;
    extension?:    string;
    modified_secs?: number;
    snippet?:      string;
    score:         number;
}

export interface IndexStats {
    total:    number;
    indexed:  number;
    errors:   number;
    watching: boolean;
}

export interface LlmConfig {
    provider: 'ollama' | 'remote';
    base_url: string;
    model:    string;
    api_key?: string;
}

// ─── API ──────────────────────────────────────────────────────────────────────

function _invoke(cmd: string, args?: Record<string, unknown>): Promise<unknown> {
    return (window as any).__TAURI_INTERNALS__.invoke(cmd, args);
}

export async function graphSearch(queryStr: string): Promise<GraphSearchResult[]> {
    return _invoke('graph_search', { queryStr }) as Promise<GraphSearchResult[]>;
}

export async function graphGetRelated(path: string): Promise<GraphSearchResult[]> {
    return _invoke('graph_get_related', { path }) as Promise<GraphSearchResult[]>;
}

export async function graphGetDuplicates(path: string): Promise<GraphSearchResult[]> {
    return _invoke('graph_get_duplicates', { path }) as Promise<GraphSearchResult[]>;
}

export async function graphIndexStatus(): Promise<IndexStats> {
    return _invoke('graph_index_status') as Promise<IndexStats>;
}

export async function graphSetRoot(path: string): Promise<void> {
    return _invoke('graph_set_root', { path }) as Promise<void>;
}

export async function graphSetLlm(config: LlmConfig): Promise<void> {
    return _invoke('graph_set_llm', { config }) as Promise<void>;
}
