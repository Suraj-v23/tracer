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

export async function graphAddIndexedFolder(path: string): Promise<void> {
    return _invoke('graph_add_indexed_folder', { path }) as Promise<void>;
}

export async function graphRemoveIndexedFolder(path: string): Promise<void> {
    return _invoke('graph_remove_indexed_folder', { path }) as Promise<void>;
}

export async function graphListIndexedFolders(): Promise<string[]> {
    return _invoke('graph_list_indexed_folders') as Promise<string[]>;
}

export async function graphContentSearch(query: string): Promise<GraphSearchResult[]> {
    return _invoke('graph_content_search', { query }) as Promise<GraphSearchResult[]>;
}

export interface DepTree {
    path:    string;
    name:    string;
    imports: DepTree[];
}

export async function graphGetImports(path: string): Promise<GraphSearchResult[]> {
    return _invoke('graph_get_imports', { path }) as Promise<GraphSearchResult[]>;
}

export async function graphGetImporters(path: string): Promise<GraphSearchResult[]> {
    return _invoke('graph_get_importers', { path }) as Promise<GraphSearchResult[]>;
}

export async function graphGetDepTree(path: string, depth?: number): Promise<DepTree> {
    return _invoke('graph_get_dep_tree', { path, depth }) as Promise<DepTree>;
}

export interface EmbedConfig {
    provider: 'ollama' | 'remote';
    base_url: string;
    model:    string;
    api_key?: string;
    dims:     number;
}

export async function graphSetEmbeddingProvider(config: EmbedConfig): Promise<void> {
    return _invoke('graph_set_embedding_provider', { config }) as Promise<void>;
}

export async function graphSemanticSearch(query: string, k?: number): Promise<GraphSearchResult[]> {
    return _invoke('graph_semantic_search', { query, k }) as Promise<GraphSearchResult[]>;
}

export async function graphFindSimilar(path: string, k?: number): Promise<GraphSearchResult[]> {
    return _invoke('graph_find_similar', { path, k }) as Promise<GraphSearchResult[]>;
}

export async function graphEmbedFolder(path: string): Promise<void> {
    return _invoke('graph_embed_folder', { path }) as Promise<void>;
}
